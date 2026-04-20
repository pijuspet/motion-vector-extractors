use inferno::collapse::perf::{Folder, Options as CollapseOptions};
use inferno::collapse::Collapse;
use inferno::flamegraph::{self, Options as FlamegraphOptions};
use minijinja::{context, Environment};
use std::io::BufReader;
use std::process::Command;


const PERF_SAMPLE_HZ: f64 = 99.0;

pub fn find_perf() -> Option<String> {
    if Command::new("perf")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some("perf".to_string());
    }

    let tools_dir = std::path::Path::new("/usr/lib/linux-tools");
    if let Ok(entries) = std::fs::read_dir(tools_dir) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("perf");
            if candidate.is_file() {
                if Command::new(&candidate)
                    .arg("--version")
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
                {
                    return Some(candidate.to_string_lossy().to_string());
                }
            }
        }
    }

    None
}

pub fn flamegraph_from_perf(
    perf_bin: &str,
    perf_data: &str,
    output_html: &str,
    title: &str,
) -> Result<(), String> {
    let output = Command::new(perf_bin)
        .args(["script", "-i", perf_data])
        .output()
        .map_err(|e| format!("Failed to run `{} script`: {}", perf_bin, e))?;

    if !output.status.success() {
        return Err(format!(
            "{} script failed: {}",
            perf_bin,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mut folder = Folder::from(CollapseOptions::default());
    let mut collapsed = Vec::new();
    folder
        .collapse(&output.stdout[..], &mut collapsed)
        .map_err(|e| format!("Stack collapse failed: {}", e))?;

    let factor = 1000.0 / PERF_SAMPLE_HZ;
    let sample_duration_ms = 1000.0 / PERF_SAMPLE_HZ;

    write_outputs(&collapsed, output_html, title, "ms", factor, sample_duration_ms)
}

fn write_outputs(
    collapsed: &[u8],
    output_html: &str,
    title: &str,
    count_name: &str,
    factor: f64,
    sample_duration_ms: f64,
) -> Result<(), String> {
    let svg_bytes = generate_svg(collapsed, title, count_name, factor)?;

    let svg_content = String::from_utf8(svg_bytes)
        .map_err(|e| format!("SVG is not valid UTF-8: {}", e))?;
    write_html(&svg_content, output_html, title, sample_duration_ms)?;
    println!("Interactive flamegraph saved to: {}", output_html);

    Ok(())
}

fn generate_svg(
    collapsed: &[u8],
    title: &str,
    count_name: &str,
    factor: f64,
) -> Result<Vec<u8>, String> {
    let mut opts = FlamegraphOptions::default();
    opts.title = title.to_string();
    opts.count_name = count_name.to_string();
    opts.factor = factor;

    let reader = BufReader::new(collapsed);
    let mut svg_bytes: Vec<u8> = Vec::new();

    flamegraph::from_reader(&mut opts, reader, &mut svg_bytes)
        .map_err(|e| format!("Flamegraph generation failed: {}", e))?;

    Ok(svg_bytes)
}

fn write_html(
    svg_content: &str,
    output_html: &str,
    title: &str,
    sample_duration_ms: f64,
) -> Result<(), String> {
    let template_source = include_str!("templates/flamegraph.html.jinja");

    let mut env = Environment::new();
    env.add_template("flamegraph", template_source)
        .map_err(|e| format!("Template parse error: {}", e))?;

    let tmpl = env
        .get_template("flamegraph")
        .map_err(|e| format!("Template error: {}", e))?;

    let rendered = tmpl
        .render(context! {
            title => title,
            svg_content => svg_content,
            sample_duration_ms => sample_duration_ms,
        })
        .map_err(|e| format!("Template render error: {}", e))?;

    std::fs::write(output_html, rendered)
        .map_err(|e| format!("Failed to write HTML: {}", e))?;

    Ok(())
}
