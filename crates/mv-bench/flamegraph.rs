use inferno::collapse::perf::{Folder, Options as CollapseOptions};
use inferno::collapse::Collapse;
use inferno::flamegraph::{self, Options as FlamegraphOptions};
use minijinja::{context, Environment};
use std::env;
use std::fs;
use std::io::BufReader;
use std::process::Command;

use crate::full_benchmark::BenchmarkRunner;


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

pub fn write_html(
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

impl BenchmarkRunner {
    #[cfg(windows)]
    pub fn flamegraph(&self) {
        // Uses VTune software sampling (no ETW, no admin) to collect stacks,
        // then converts the top-down report to inferno folded format and renders
        // a self-contained HTML flamegraph — same pipeline as Linux/perf.
        println!("Generating flamegraph for extractor4 (VTune sw-sampling, no admin required)...");

        let vtune = match Self::find_vtune() {
            Some(v) => v,
            None => {
                eprintln!("vtune not found. Install VTune and re-run (step 2 checks the same paths).");
                return;
            }
        };

        let flamegraph_dir = self.results_dir.join("flamegraph");
        fs::create_dir_all(&flamegraph_dir).ok();

        let vtune_dir   = flamegraph_dir.join("vtune_fg");
        let topdown_csv = vtune_dir.join("topdown.csv");
        let output_csv  = flamegraph_dir.join("method4_output_flamegraph.csv");
        let output_html = flamegraph_dir.join("extractor4_flamegraph.html");
        let extractor   = self.extractor_executables.join("cust").join("extractor4.exe");

        fs::create_dir_all(&vtune_dir).ok();

        // Collect hotspots with software sampling — no admin needed
        let collect = Command::new(&vtune)
            .args([
                "-collect", "hotspots",
                "-knob", "sampling-mode=sw",
                "-result-dir", &vtune_dir.to_string_lossy(),
                "--",
                &extractor.to_string_lossy(),
                &self.video_file, "1",
                &output_csv.to_string_lossy(),
                "1", "0",
            ])
            .stdin(std::process::Stdio::null())
            .status();

        match collect {
            Ok(s) if s.success() => {}
            Ok(s) => { eprintln!("vtune collect exited: {}", s); return; }
            Err(e) => { eprintln!("vtune failed: {}", e); return; }
        }

        // Export the top-down tree in the tab-separated format build_vtune_tree expects
        Command::new(&vtune)
            .args([
                "-report", "top-down",
                "-result-dir", &vtune_dir.to_string_lossy(),
                "-format", "csv",
                "-report-output", &topdown_csv.to_string_lossy(),
            ])
            .stdin(std::process::Stdio::null())
            .status()
            .ok();

        // Parse the tree, convert to inferno folded format, render SVG + HTML
        let (nodes, root_nodes) = match crate::vtune_hotspots_plot::build_vtune_tree(
            &topdown_csv.to_string_lossy(),
        ) {
            Ok(r) => r,
            Err(e) => { eprintln!("Failed to parse VTune output: {}", e); return; }
        };

        let folded = crate::vtune_hotspots_plot::vtune_tree_to_folded(&nodes, &root_nodes);
        if folded.is_empty() {
            eprintln!("No samples in VTune output — nothing to render.");
            return;
        }

        let mut svg: Vec<u8> = Vec::new();
        {
            let mut opts = FlamegraphOptions::default();
            opts.title = "extractor4 Flamegraph (VTune sw-sampling)".to_string();
            opts.count_name = "ms".to_string();
            if let Err(e) = flamegraph::from_reader(&mut opts, BufReader::new(folded.as_slice()), &mut svg) {
                eprintln!("Flamegraph render failed: {}", e);
                return;
            }
        }

        let svg_str = match String::from_utf8(svg) {
            Ok(s) => s,
            Err(e) => { eprintln!("SVG encoding error: {}", e); return; }
        };

        match write_html(&svg_str, &output_html.to_string_lossy(), "extractor4 Flamegraph", 1.0) {
            Ok(()) => println!("Flamegraph saved to: {}", output_html.display()),
            Err(e) => eprintln!("Failed to write flamegraph HTML: {}", e),
        }
    }

    #[cfg(unix)]
    pub fn flamegraph(&self) {
        println!("Generating flamegraph for extractor4...");

        let flamegraph_dir = self.results_dir.join("flamegraph");
        fs::create_dir_all(&flamegraph_dir).ok();

        let output_html = flamegraph_dir.join("extractor4_flamegraph.html");

        if let Some(perf_bin) = find_perf() {
            println!("Using perf binary: {}", perf_bin);

            let ffmpeg_lib = self
                .current_dir
                .join("ffmpeg")
                .join("FFmpeg-8.0-custom")
                .join("lib");

            let perf_data = flamegraph_dir.join("perf.data");
            let output_csv = self.results_dir.join("method4_output_flamegraph.csv");
            let extractor_exec = self.extractor_executables.join("extractor4");

            let existing_ld = env::var("LD_LIBRARY_PATH").unwrap_or_default();
            let ld_path = format!("{}:{}", ffmpeg_lib.display(), existing_ld);

            let perf_cmd = format!(
                "LD_LIBRARY_PATH={} {} record -g --call-graph dwarf -F 99 -o {} -- {} {} 1 {} 1 1",
                ld_path,
                perf_bin,
                perf_data.display(),
                extractor_exec.display(),
                self.video_file,
                output_csv.display(),
            );

            println!("Running: perf record on extractor4...");
            if !self.run_shell_command(&perf_cmd, Some(&self.extractor_executables), None) {
                eprintln!("perf record failed.");
                return;
            }

            if let Err(e) = flamegraph_from_perf(
                &perf_bin,
                &perf_data.to_string_lossy(),
                &output_html.to_string_lossy(),
                "extractor4 Flamegraph (perf)",
            ) {
                eprintln!("Flamegraph generation failed: {}", e);
                return;
            }
        } else {
            println!("Perf not found.")
        }

        println!(
            "Flamegraph complete.\n  HTML: {}",
            output_html.display()
        );
    }
}
