use minijinja::{context, Environment};
use plotters::prelude::*;
use plotters::series::Histogram;
use plotters::style::register_font;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process;

const FONT_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

fn init_fonts() -> Result<(), String> {
    let font_data =
        fs::read(FONT_PATH).map_err(|e| format!("Cannot read font {}: {}", FONT_PATH, e))?;
    let font_data: &'static [u8] = Box::leak(font_data.into_boxed_slice());
    register_font("sans-serif", FontStyle::Normal, font_data)
        .map_err(|_| "Failed to register font".to_string())?;
    register_font("sans-serif", FontStyle::Bold, font_data)
        .map_err(|_| "Failed to register font".to_string())?;
    Ok(())
}

struct TreeNode {
    name: String,
    cpu_total: f64,
    cpu_self: f64,
    children: Vec<String>,
}

fn build_vtune_tree(csv_file: &str) -> Result<(HashMap<String, TreeNode>, Vec<String>), String> {
    let file = fs::File::open(csv_file).map_err(|e| format!("Cannot open file: {}", e))?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let header_line = lines
        .next()
        .ok_or("Empty CSV file")?
        .map_err(|e| format!("Read error: {}", e))?;
    let headers: Vec<&str> = header_line.split('\t').collect();

    let col_func = headers
        .iter()
        .position(|h| *h == "Function Stack")
        .ok_or("Missing 'Function Stack' column")?;
    let col_total = headers
        .iter()
        .position(|h| *h == "CPU Time:Total")
        .ok_or("Missing 'CPU Time:Total' column")?;
    let col_self = headers
        .iter()
        .position(|h| *h == "CPU Time:Self")
        .ok_or("Missing 'CPU Time:Self' column")?;

    let mut nodes: HashMap<String, TreeNode> = HashMap::new();
    let mut level_stack: Vec<(usize, String)> = Vec::new();
    let mut root_nodes: Vec<String> = Vec::new();
    // Preserve insertion order for HTML generation
    let mut node_order: Vec<String> = Vec::new();

    for (index, line_result) in lines.enumerate() {
        let line = line_result.map_err(|e| format!("Read error at line {}: {}", index + 2, e))?;
        if line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        let function_line = fields.get(col_func).unwrap_or(&"");
        let cpu_total: f64 = fields
            .get(col_total)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        let cpu_self: f64 = fields
            .get(col_self)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);

        let leading_spaces = function_line.len() - function_line.trim_start_matches(' ').len();
        let function_name = function_line.trim().to_string();
        let node_id = format!("node_{}", index);

        // Pop stack to maintain hierarchy
        while let Some((level, _)) = level_stack.last() {
            if *level >= leading_spaces {
                level_stack.pop();
            } else {
                break;
            }
        }

        let parent_id = level_stack.last().map(|(_, id)| id.clone());

        nodes.insert(
            node_id.clone(),
            TreeNode {
                name: function_name,
                cpu_total,
                cpu_self,
                children: Vec::new(),
            },
        );
        node_order.push(node_id.clone());

        if let Some(ref pid) = parent_id {
            if let Some(parent) = nodes.get_mut(pid) {
                parent.children.push(node_id.clone());
            }
        } else {
            root_nodes.push(node_id.clone());
        }

        level_stack.push((leading_spaces, node_id));
    }

    Ok((nodes, root_nodes))
}

fn generate_tree_html(nodes: &HashMap<String, TreeNode>, node_id: &str) -> String {
    let node = match nodes.get(node_id) {
        Some(n) => n,
        None => return String::new(),
    };

    let has_children = !node.children.is_empty();
    let arrow = if has_children { ">" } else { "" };
    let collapsed_class = if has_children { "collapsed" } else { "" };

    // Escape HTML entities in name
    let escaped_name = node
        .name
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");

    let mut html = format!(
        r#"<li class="tree-node {collapsed_class}" data-node="{node_id}">
  <span class="node-content" onclick="toggleNode('{node_id}')">
    <span class="arrow">{arrow}</span>
    <span class="name">{escaped_name}</span>
    <span class="cpu-total">{cpu_total:.1}%</span>
    <span class="cpu-self">{cpu_self:.1}s</span>
  </span>
"#,
        collapsed_class = collapsed_class,
        node_id = node_id,
        arrow = arrow,
        escaped_name = escaped_name,
        cpu_total = node.cpu_total,
        cpu_self = node.cpu_self,
    );

    if has_children {
        html.push_str(&format!(
            r#"  <ul class="children" id="children_{node_id}" style="display: none;">
"#,
            node_id = node_id,
        ));
        for child_id in &node.children {
            html.push_str(&generate_tree_html(nodes, child_id));
        }
        html.push_str("  </ul>\n");
    }

    html.push_str("</li>\n");
    html
}

fn generate_complete_html(
    nodes: &HashMap<String, TreeNode>,
    root_nodes: &[String],
    output_file: &str,
) -> Result<(), String> {
    let tree_html: String = root_nodes
        .iter()
        .map(|root_id| generate_tree_html(nodes, root_id))
        .collect();

    let template_source = include_str!("../../utils/templates/vtune.html.jinja");

    let mut env = Environment::new();
    env.add_template("vtune", template_source)
        .map_err(|e| format!("Template parse error: {}", e))?;

    let tmpl = env
        .get_template("vtune")
        .map_err(|e| format!("Template error: {}", e))?;

    let rendered = tmpl
        .render(context! {
            nodes_count => nodes.len(),
            roots_count => root_nodes.len(),
            tree_html => tree_html,
        })
        .map_err(|e| format!("Template render error: {}", e))?;

    fs::write(output_file, rendered).map_err(|e| format!("Failed to write HTML: {}", e))
}

fn generate_hotspots_chart(csv_file: &str, output_directory: &str) -> Result<(), String> {
    init_fonts()?;

    let file = fs::File::open(csv_file).map_err(|e| format!("Cannot open file: {}", e))?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let header_line = lines
        .next()
        .ok_or("Empty CSV file")?
        .map_err(|e| format!("Read error: {}", e))?;
    let headers: Vec<&str> = header_line.split('\t').collect();

    let col_func = headers
        .iter()
        .position(|h| *h == "Function Stack")
        .ok_or("Missing 'Function Stack' column")?;
    let col_total = headers
        .iter()
        .position(|h| *h == "CPU Time:Total")
        .ok_or("Missing 'CPU Time:Total' column")?;

    // Aggregate CPU time by function name
    let mut aggregated: HashMap<String, f64> = HashMap::new();

    for line_result in lines {
        let line = line_result.map_err(|e| format!("Read error: {}", e))?;
        if line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        let func_name = fields
            .get(col_func)
            .unwrap_or(&"")
            .trim()
            .to_string();
        let cpu_total: f64 = fields
            .get(col_total)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);

        if func_name.to_lowercase() == "total" {
            continue;
        }

        *aggregated.entry(func_name).or_insert(0.0) += cpu_total;
    }

    // Sort by CPU time descending and take top 30
    let mut sorted: Vec<(String, f64)> = aggregated.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted.truncate(30);

    if sorted.is_empty() {
        return Ok(());
    }

    // Reverse for bottom-to-top bar chart (plotters draws top-down)
    sorted.reverse();

    let max_value = sorted
        .iter()
        .map(|(_, v)| *v)
        .fold(0.0f64, f64::max)
        * 1.15;

    let png_file = Path::new(output_directory).join("vtune_hotspots.png");

    let root = BitMapBackend::new(&png_file, (1960, 1400)).into_drawing_area();
    root.fill(&WHITE)
        .map_err(|e| format!("Drawing error: {}", e))?;

    let mut chart = ChartBuilder::on(&root)
        .caption("Top 30 VTune Hotspots", ("sans-serif", 28, FontStyle::Bold).into_font())
        .margin(15)
        .x_label_area_size(50)
        .y_label_area_size(400)
        .build_cartesian_2d(0.0..max_value, (0..sorted.len()).into_segmented())
        .map_err(|e| format!("Chart build error: {}", e))?;

    chart
        .configure_mesh()
        .x_desc("CPU Time (%)")
        .x_label_style(("sans-serif", 15))
        .y_label_style(("sans-serif", 13, FontStyle::Bold))
        .y_labels(sorted.len())
        .y_label_formatter(&|pos| {
            if let SegmentValue::CenterOf(idx) = pos {
                if *idx < sorted.len() {
                    // Truncate long names for the y-axis label
                    let name = &sorted[*idx].0;
                    if name.len() > 50 {
                        return format!("{}...", &name[..47]);
                    }
                    return name.clone();
                }
            }
            String::new()
        })
        .draw()
        .map_err(|e| format!("Mesh draw error: {}", e))?;

    chart
        .draw_series(
            Histogram::horizontal(&chart)
                .style(RGBColor(41, 128, 185).filled())
                .margin(8)
                .data(
                    sorted
                        .iter()
                        .enumerate()
                        .map(|(idx, (_, value))| (idx, *value)),
                ),
        )
        .map_err(|e| format!("Series draw error: {}", e))?;

    // Draw value labels on bars
    chart
        .draw_series(sorted.iter().enumerate().map(|(idx, (_, value))| {
            Text::new(
                format!("{:.1}%", value),
                (*value + max_value * 0.01, SegmentValue::CenterOf(idx)),
                ("sans-serif", 13).into_font(),
            )
        }))
        .map_err(|e| format!("Label draw error: {}", e))?;

    root.present()
        .map_err(|e| format!("Present error: {}", e))?;

    println!("Hotspots bar chart saved to: {}", png_file.display());
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: vtune_hotspots_plot <csv_file>");
        process::exit(1);
    }

    let csv_file = &args[1];

    if !Path::new(csv_file).is_file() {
        eprintln!("Error: File '{}' not found.", csv_file);
        process::exit(1);
    }

    println!("Building VTune call tree...");

    let (nodes, root_nodes) = match build_vtune_tree(csv_file) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    println!(
        "Built tree with {} nodes and {} root functions",
        nodes.len(),
        root_nodes.len()
    );

    let output_directory = Path::new(csv_file)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let html_file = format!("{}/call_tree.html", output_directory);

    if let Err(e) = generate_complete_html(&nodes, &root_nodes, &html_file) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }

    if let Err(e) = generate_hotspots_chart(csv_file, &output_directory) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }

    println!("HTML call tree saved to: {}", html_file);
}
