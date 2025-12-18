import pandas as pd
import sys
import os
import matplotlib.pyplot as plt

def build_vtune_tree(csv_file):
    """Build proper hierarchical tree from VTune CSV indentation"""
    df = pd.read_csv(csv_file, delimiter='\t')
    
    # Clean data
    df['CPU Time:Total'] = pd.to_numeric(df['CPU Time:Total'], errors='coerce').fillna(0)
    df['CPU Time:Self'] = pd.to_numeric(df['CPU Time:Self'], errors='coerce').fillna(0)
    
    nodes = {}
    stack = []  # Stack of (level, node_id) pairs
    roots = []
    
    for idx, row in df.iterrows():
        func_line = row['Function Stack']
        cpu_total = float(row['CPU Time:Total'])
        cpu_self = float(row['CPU Time:Self'])
        
        # Calculate indentation level
        leading_spaces = len(func_line) - len(func_line.lstrip(' '))
        level = leading_spaces // 2
        func_name = func_line.strip()
        
        # Create unique node ID
        node_id = f"node_{idx}"
        
        # Pop stack to current level
        while stack and stack[-1][0] >= level:
            stack.pop()
        
        # Get parent
        parent_id = stack[-1][1] if stack else None
        
        # Create node
        nodes[node_id] = {
            'name': func_name,
            'cpu_total': cpu_total,
            'cpu_self': cpu_self,
            'level': level,
            'children': [],
            'parent': parent_id
        }
        
        # Add to parent's children or roots
        if parent_id:
            nodes[parent_id]['children'].append(node_id)
        else:
            roots.append(node_id)
        
        # Push current node to stack
        stack.append((level, node_id))
    
    return nodes, roots

def generate_tree_html(nodes, node_id):
    """Generate properly nested HTML tree"""
    node = nodes[node_id]
    children = node['children']
    has_children = len(children) > 0
    
    # Generate the list item
    arrow = '▶' if has_children else ''
    collapsed_class = 'collapsed' if has_children else ''
    
    html = f'<li class="tree-node {collapsed_class}" data-node="{node_id}">\n'
    html += f'  <span class="node-content" onclick="toggleNode(\'{node_id}\')">\n'
    html += f'    <span class="arrow">{arrow}</span>\n'
    html += f'    <span class="name">{node["name"]}</span>\n'
    html += f'    <span class="cpu-total">{node["cpu_total"]:.1f}%</span>\n'
    html += f'    <span class="cpu-self">{node["cpu_self"]:.1f}s</span>\n'
    html += f'  </span>\n'
    
    # Add children if they exist
    if has_children:
        html += f'  <ul class="children" id="children_{node_id}" style="display: none;">\n'
        for child_id in children:
            html += generate_tree_html(nodes, child_id)
        html += f'  </ul>\n'
    
    html += '</li>\n'
    return html

def generate_complete_html(nodes, roots, output_file):
    """Generate complete HTML file with working tree"""
    tree_html = "".join(generate_tree_html(nodes, r) for r in roots)

    template_path = os.path.join(
        os.path.dirname(__file__),
        "vtune.html"
    )

    with open(template_path, "r", encoding="utf-8") as f:
        template = f.read()

    html = (template
            .replace("{{TREE_HTML}}", tree_html)
            .replace("{{NODES_COUNT}}", str(len(nodes)))
            .replace("{{ROOTS_COUNT}}", str(len(roots))))

    with open(output_file, "w", encoding="utf-8") as f:
        f.write(html)


def generate_hotspots_chart(csv_file, output_dir):
    """Generate PNG bar chart of top hotspots"""
    df = pd.read_csv(csv_file, delimiter='\t')
    df['CPU Time:Total'] = pd.to_numeric(df['CPU Time:Total'], errors='coerce').fillna(0)
    
    # Remove 'Total' and get top functions
    df_filtered = df[df['Function Stack'].str.strip().str.lower() != 'total'].copy()
    df_filtered['Function Clean'] = df_filtered['Function Stack'].str.strip()
    df_grouped = df_filtered.groupby('Function Clean')['CPU Time:Total'].sum().reset_index()
    df_top = df_grouped.sort_values('CPU Time:Total', ascending=False).head(30)
    
    plt.figure(figsize=(14, 10))
    bars = plt.barh(df_top['Function Clean'], df_top['CPU Time:Total'], 
                    color='#2980b9', height=0.6)
    
    plt.xlabel('CPU Time (%)', fontsize=13, fontweight='bold')
    plt.title('Top 30 VTune Hotspots', fontsize=18, fontweight='bold', pad=15)
    plt.gca().invert_yaxis()
    plt.grid(axis='x', linestyle='--', alpha=0.4)
    
    for bar, value in zip(bars, df_top['CPU Time:Total']):
        plt.text(bar.get_width() + 1, bar.get_y() + bar.get_height()/2,
                f"{value:.1f}%", va='center', ha='left', fontsize=11, fontweight='bold')
    
    plt.tight_layout()
    png_file = os.path.join(output_dir, "vtune_hotspots.png")
    plt.savefig(png_file, dpi=140, bbox_inches='tight')
    plt.close()
    print(f"Hotspots bar chart saved to: {png_file}")

def main():
    if len(sys.argv) < 2:
        print("Usage: python vtune_tree.py <topdown.csv>")
        sys.exit(1)
    
    csv_file = sys.argv[1]
    print("Building VTune call tree...")
    
    nodes, roots = build_vtune_tree(csv_file)
    print(f"Built tree with {len(nodes)} nodes and {len(roots)} root functions")
    
    output_dir = os.path.dirname(os.path.abspath(csv_file))
    html_file = os.path.join(output_dir, "call_tree.html")
    
    generate_complete_html(nodes, roots, html_file)
    generate_hotspots_chart(csv_file, output_dir)
    
    print(f"HTML call tree saved to: {html_file}")

if __name__ == "__main__":
    main()
