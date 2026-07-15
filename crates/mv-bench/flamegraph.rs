use inferno::collapse::perf::{Folder, Options as CollapseOptions};
use inferno::collapse::Collapse;
use inferno::flamegraph::{self, Options as FlamegraphOptions};
use minijinja::{context, Environment};
#[cfg(unix)]
use std::env;
use std::fs;
use std::io::BufReader;
use std::process::Command;

use crate::full_benchmark::BenchmarkRunner;

/// Convert a path to its Windows 8.3 short form so that tools like VTune,
/// which cannot handle spaces in application paths, can find the file.
/// Falls back to the original path if the conversion fails or if 8.3 names
/// are disabled on the volume.
#[cfg(windows)]
pub fn win_short_path(path: &std::path::Path) -> std::path::PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    extern "system" {
        fn GetShortPathNameW(
            lpszLongPath: *const u16,
            lpszShortPath: *mut u16,
            cchBuffer: u32,
        ) -> u32;
    }

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0u16)).collect();
    let needed = unsafe { GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    if needed == 0 {
        return path.to_path_buf();
    }
    let mut buf = vec![0u16; needed as usize];
    let len = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), needed) };
    if len == 0 || len >= needed {
        return path.to_path_buf();
    }
    buf.truncate(len as usize);
    std::path::PathBuf::from(OsString::from_wide(&buf))
}

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
    total_duration_ms: f64,
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

    write_outputs(&collapsed, output_html, title, total_duration_ms)
}

fn write_outputs(
    collapsed: &[u8],
    output_html: &str,
    title: &str,
    total_duration_ms: f64,
) -> Result<(), String> {
    let svg_bytes = generate_svg(collapsed, title)?;

    let svg_content = String::from_utf8(svg_bytes)
        .map_err(|e| format!("SVG is not valid UTF-8: {}", e))?;
    write_html(&svg_content, output_html, title, total_duration_ms)?;
    println!("Interactive flamegraph saved to: {}", output_html);

    Ok(())
}

fn generate_svg(
    collapsed: &[u8],
    title: &str,
) -> Result<Vec<u8>, String> {
    let mut opts = FlamegraphOptions::default();
    opts.title = title.to_string();

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
    total_duration_ms: f64,
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
            total_duration_ms => total_duration_ms,
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
        let extractor_name = format!("extractor{}", self.profiler_extractor);
        println!("Generating flamegraph for {} (VTune sw-sampling, no admin required)...", extractor_name);

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
        let output_csv  = flamegraph_dir.join(format!("method{}_output_flamegraph.csv", self.profiler_extractor));
        let output_html = flamegraph_dir.join(format!("{}_flamegraph.html", extractor_name));
        let subdir = if self.profiler_extractor <= 2 { "sys" } else { "cust" };
        let extractor   = self.extractor_executables.join(subdir).join(format!("{}.exe", extractor_name));
        // VTune cannot launch applications whose path contains spaces; use the
        // 8.3 short form to avoid that limitation.
        let extractor   = win_short_path(&extractor);

        fs::create_dir_all(&vtune_dir).ok();

        let tc_str = self.thread_count.to_string();
        let kf_str = if self.keyframes_only { "1" } else { "0" };

        // Collect hotspots with software sampling — no admin needed
        let collect_start = std::time::Instant::now();
        let collect = Command::new(&vtune)
            .args([
                "-collect", "hotspots",
                "-knob", "sampling-mode=sw",
                "-result-dir", &vtune_dir.to_string_lossy(),
                "--",
                &extractor.to_string_lossy(),
                &self.video_file, "0",
                &output_csv.to_string_lossy(),
                "1", &tc_str, kf_str,
            ])
            .stdin(std::process::Stdio::null())
            .status();
        let total_duration_ms = collect_start.elapsed().as_secs_f64() * 1000.0;

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
            opts.title = format!("{} Flamegraph (VTune sw-sampling)", extractor_name);
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

        match write_html(&svg_str, &output_html.to_string_lossy(), &format!("{} Flamegraph", extractor_name), total_duration_ms) {
            Ok(()) => println!("Flamegraph saved to: {}", output_html.display()),
            Err(e) => eprintln!("Failed to write flamegraph HTML: {}", e),
        }
    }

    #[cfg(unix)]
    pub fn flamegraph(&self) {
        let extractor_name = format!("extractor{}", self.profiler_extractor);
        println!("Generating flamegraph for {}...", extractor_name);

        let flamegraph_dir = self.results_dir.join("flamegraph");
        fs::create_dir_all(&flamegraph_dir).ok();

        let output_html = flamegraph_dir.join(format!("{}_flamegraph.html", extractor_name));

        if let Some(perf_bin) = find_perf() {
            println!("Using perf binary: {}", perf_bin);

            let ffmpeg_variant = if self.profiler_extractor >= 3 { "FFmpeg-8.0-custom" } else { "FFmpeg-8.0" };
            let ffmpeg_lib = self.current_dir.join("ffmpeg").join(ffmpeg_variant).join("lib");

            let perf_data = flamegraph_dir.join("perf.data");
            let output_csv = self.results_dir.join(format!("method{}_output_flamegraph.csv", self.profiler_extractor));
            let extractor_exec = self.extractor_executables.join(&extractor_name);

            let existing_ld = env::var("LD_LIBRARY_PATH").unwrap_or_default();
            let ld_path = format!("{}:{}", ffmpeg_lib.display(), existing_ld);

            let tc_str = self.thread_count.to_string();
            let kf_str = if self.keyframes_only { "1" } else { "0" };
            let perf_cmd = format!(
                "LD_LIBRARY_PATH={} {} record -g --call-graph dwarf -F 99 -o {} -- {} {} 0 {} 1 {} {}",
                ld_path,
                perf_bin,
                perf_data.display(),
                extractor_exec.display(),
                self.video_file,
                output_csv.display(),
                tc_str,
                kf_str,
            );

            println!("Running: perf record on {}...", extractor_name);
            let perf_start = std::time::Instant::now();
            if !self.run_shell_command(&perf_cmd, Some(&self.extractor_executables), None) {
                eprintln!("perf record failed.");
                return;
            }
            let total_duration_ms = perf_start.elapsed().as_secs_f64() * 1000.0;

            if let Err(e) = flamegraph_from_perf(
                &perf_bin,
                &perf_data.to_string_lossy(),
                &output_html.to_string_lossy(),
                &format!("{} Flamegraph (perf)", extractor_name),
                total_duration_ms,
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
