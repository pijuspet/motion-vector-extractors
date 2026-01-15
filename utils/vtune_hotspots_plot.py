import pandas as pd
import os
import matplotlib.pyplot as plt
from typing import Dict, List, Tuple, Optional
from dataclasses import dataclass
from jinja2 import Template

@dataclass
class TreeNode:
    name: str
    cpu_total: float
    cpu_self: float
    level: int
    children: List[str]
    parent: Optional[str]


def build_vtune_tree(csv_file: str) -> Tuple[Dict[str, TreeNode], List[str]]:
    dataframe = pd.read_csv(csv_file, delimiter="\t")

    # Clean and convert numeric columns
    dataframe["CPU Time:Total"] = pd.to_numeric(
        dataframe["CPU Time:Total"], errors="coerce"
    ).fillna(0)
    dataframe["CPU Time:Self"] = pd.to_numeric(
        dataframe["CPU Time:Self"], errors="coerce"
    ).fillna(0)

    nodes: Dict[str, TreeNode] = {}
    level_stack: List[Tuple[int, str]] = []
    root_nodes: List[str] = []

    for index, row in dataframe.iterrows():
        function_line = row["Function Stack"]
        cpu_total = float(row["CPU Time:Total"])
        cpu_self = float(row["CPU Time:Self"])

        # Calculate indentation level (2 spaces per level)
        leading_spaces = len(function_line) - len(function_line.lstrip(" "))
        level = leading_spaces // 2
        function_name = function_line.strip()

        node_id = f"node_{index}"

        # Pop stack to maintain hierarchy at current level
        while level_stack and level_stack[-1][0] >= level:
            level_stack.pop()

        # Determine parent from stack
        parent_id = level_stack[-1][1] if level_stack else None

        nodes[node_id] = TreeNode(
            name=function_name,
            cpu_total=cpu_total,
            cpu_self=cpu_self,
            level=level,
            children=[],
            parent=parent_id,
        )

        if parent_id:
            nodes[parent_id].children.append(node_id)
        else:
            root_nodes.append(node_id)

        # Push current node to stack
        level_stack.append((level, node_id))

    return nodes, root_nodes


def generate_tree_html(nodes: Dict[str, TreeNode], node_id: str) -> str:
    node = nodes[node_id]
    has_children = len(node.children) > 0

    arrow = ">" if has_children else ""
    collapsed_class = "collapsed" if has_children else ""

    html_parts = [
        f'<li class="tree-node {collapsed_class}" data-node="{node_id}">\n',
        f'  <span class="node-content" onclick="toggleNode(\'{node_id}\')">\n',
        f'    <span class="arrow">{arrow}</span>\n',
        f'    <span class="name">{node.name}</span>\n',
        f'    <span class="cpu-total">{node.cpu_total:.1f}%</span>\n',
        f'    <span class="cpu-self">{node.cpu_self:.1f}s</span>\n',
        f"  </span>\n",
    ]

    if has_children:
        html_parts.append(
            f'  <ul class="children" id="children_{node_id}" style="display: none;">\n'
        )
        for child_id in node.children:
            html_parts.append(generate_tree_html(nodes, child_id))
        html_parts.append("  </ul>\n")

    html_parts.append("</li>\n")
    return "".join(html_parts)


def generate_complete_html(
    nodes: Dict[str, TreeNode], root_nodes: List[str], output_file: str
) -> None:
    tree_html = "".join(generate_tree_html(nodes, root_id) for root_id in root_nodes)
    
    template_path = os.path.join(os.path.dirname(__file__), "templates", "vtune.html.jinja")
    with open(template_path, "r") as f:
            template = Template(f.read())
    rendered = template.render(
        nodes_count=len(nodes),
        roots_count=len(root_nodes),
        tree_html=tree_html
    )

    with open(output_file, "w", encoding="utf-8") as output:
        output.write(rendered)


def generate_hotspots_chart(csv_file: str, output_directory: str) -> None:
    dataframe = pd.read_csv(csv_file, delimiter="\t")
    dataframe["CPU Time:Total"] = pd.to_numeric(
        dataframe["CPU Time:Total"], errors="coerce"
    ).fillna(0)

    # Filter and aggregate
    filtered_data = dataframe[
        dataframe["Function Stack"].str.strip().str.lower() != "total"
    ].copy()
    filtered_data["Function Clean"] = filtered_data["Function Stack"].str.strip()

    grouped_data = (
        filtered_data.groupby("Function Clean")["CPU Time:Total"].sum().reset_index()
    )
    top_functions = grouped_data.sort_values("CPU Time:Total", ascending=False).head(30)

    plt.figure(figsize=(14, 10))
    bars = plt.barh(
        top_functions["Function Clean"],
        top_functions["CPU Time:Total"],
        color="#2980b9",
        height=0.6,
    )

    plt.xlabel("CPU Time (%)", fontsize=13, fontweight="bold")
    plt.title("Top 30 VTune Hotspots", fontsize=18, fontweight="bold", pad=15)
    plt.gca().invert_yaxis()
    plt.grid(axis="x", linestyle="--", alpha=0.4)

    for bar, value in zip(bars, top_functions["CPU Time:Total"]):
        plt.text(
            bar.get_width() + 1,
            bar.get_y() + bar.get_height() / 2,
            f"{value:.1f}%",
            va="center",
            ha="left",
            fontsize=11,
            fontweight="bold",
        )

    plt.tight_layout()
    png_file = os.path.join(output_directory, "vtune_hotspots.png")
    plt.savefig(png_file, dpi=140, bbox_inches="tight")
    plt.close()

    print(f"Hotspots bar chart saved to: {png_file}")

def build_tree(csv_file):
    print("Building VTune call tree...")

    nodes, root_nodes = build_vtune_tree(csv_file)
    print(f"Built tree with {len(nodes)} nodes and {len(root_nodes)} root functions")

    output_directory = os.path.dirname(os.path.abspath(csv_file))
    html_file = os.path.join(output_directory, "call_tree.html")

    generate_complete_html(nodes, root_nodes, html_file)
    generate_hotspots_chart(csv_file, output_directory)

    print(f"HTML call tree saved to: {html_file}")