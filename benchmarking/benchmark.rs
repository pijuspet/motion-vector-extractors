use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::process::Command;

use crate::benchmark_extractors::run_benchmark_extractors;

// ── Data types shared across modules ────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkResult {
    pub method: String,
    pub streams: i32,
    pub time_per_frame: f64,
    pub fps: f64,
    pub cpu_ms_per_frame: f64,
    pub memory: f64,
    pub mem_per_stream: f64,
    pub frames: i32,
}

pub const NUMERIC_FIELDS: &[&str] = &[
    "time_per_frame",
    "fps",
    "cpu_ms_per_frame",
    "memory",
    "mem_per_stream",
    "frames",
];

// ── Stream run generation ───────────────────────────────────────────────────

pub fn generate_stream_runs(max_streams: i32) -> Vec<i32> {
    let mut base: Vec<i32> = [1, 3, 5].iter().copied().filter(|&x| x <= max_streams).collect();
    if max_streams > 5 {
        let mut s = 10;
        while s <= max_streams {
            base.push(s);
            s += 5;
        }
    }
    base
}

// ── Output parsing ──────────────────────────────────────────────────────────

pub fn parse_output(output_text: &str, stream_count: i32) -> Vec<BenchmarkResult> {
    let mut in_table = false;
    let mut results = Vec::new();

    for line in output_text.lines() {
        let line = line.trim();

        if line.contains("Method") && line.contains("ms/frame(strm)") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        if line.is_empty()
            || line.starts_with('\u{2014}')
            || line.starts_with("---")
            || line.starts_with("===")
        {
            continue;
        }
        if line.starts_with("ms/") || line.starts_with("CPU") || line.starts_with("Total") {
            continue;
        }

        let parts: Vec<&str> = line.split('|').map(|p| p.trim()).collect();
        if parts.len() < 9 {
            continue;
        }

        let parse_f = |s: &str| -> Option<f64> {
            s.replace("ms", "").trim().parse::<f64>().ok()
        };

        let method = parts[0].to_string();
        let time_per_frame = match parse_f(parts[1]) { Some(v) => v, None => continue };
        let fps = match parts[2].parse::<f64>() { Ok(v) => v, Err(_) => continue };
        let cpu_ms_per_frame = match parse_f(parts[3]) { Some(v) => v, None => continue };
        let mem_total_kb = match parts[6].parse::<f64>() { Ok(v) => v, Err(_) => continue };
        let mem_per_stream_kb = match parts[7].parse::<f64>() { Ok(v) => v, Err(_) => continue };
        let frames = match parts[8].parse::<i32>() { Ok(v) => v, Err(_) => continue };

        results.push(BenchmarkResult {
            method,
            streams: stream_count,
            time_per_frame,
            fps,
            cpu_ms_per_frame,
            memory: mem_total_kb,
            mem_per_stream: mem_per_stream_kb,
            frames,
        });
    }
    results
}

// ── Run benchmark subprocess ────────────────────────────────────────────────

pub fn run_benchmark(
    input_file: &str,
    streams: i32,
    project_absolute_path: &str,
    results_absolute_path: &str,
    is_single_threaded: i32,
    is_verbose: i32,
    write_to_csv: i32,
) -> Vec<BenchmarkResult> {
    println!("Running benchmark with {} streams...", streams);

    run_benchmark_extractors(
        input_file,
        streams,
        results_absolute_path,
        project_absolute_path,
        is_single_threaded != 0,
        is_verbose != 0,
        write_to_csv != 0,
    )
    .unwrap_or_default()
}

// ── Averaged runs ───────────────────────────────────────────────────────────

pub fn run_benchmark_averaged(
    input_file: &str,
    streams: i32,
    project_absolute_path: &str,
    results_absolute_path: &str,
    is_single_threaded: i32,
    is_verbose: i32,
    write_to_csv: i32,
    n_runs: usize,
) -> Vec<BenchmarkResult> {
    let mut all_runs: Vec<Vec<BenchmarkResult>> = Vec::new();

    for run_idx in 1..=n_runs {
        println!("  Run {}/{} for {} streams...", run_idx, n_runs, streams);
        let results = run_benchmark(
            input_file,
            streams,
            project_absolute_path,
            results_absolute_path,
            is_single_threaded,
            is_verbose,
            write_to_csv,
        );
        if !results.is_empty() {
            all_runs.push(results);
        }
    }

    if all_runs.is_empty() {
        return Vec::new();
    }

    let mut groups: HashMap<(String, i32), Vec<&BenchmarkResult>> = HashMap::new();
    for run in &all_runs {
        for r in run {
            groups
                .entry((r.method.clone(), r.streams))
                .or_default()
                .push(r);
        }
    }

    let mut averaged = Vec::new();
    for ((method, streams_val), entries) in &groups {
        let n = entries.len() as f64;
        averaged.push(BenchmarkResult {
            method: method.clone(),
            streams: *streams_val,
            time_per_frame: entries.iter().map(|e| e.time_per_frame).sum::<f64>() / n,
            fps: entries.iter().map(|e| e.fps).sum::<f64>() / n,
            cpu_ms_per_frame: entries.iter().map(|e| e.cpu_ms_per_frame).sum::<f64>() / n,
            memory: entries.iter().map(|e| e.memory).sum::<f64>() / n,
            mem_per_stream: entries.iter().map(|e| e.mem_per_stream).sum::<f64>() / n,
            frames: (entries.iter().map(|e| e.frames as f64).sum::<f64>() / n) as i32,
        });
    }
    averaged
}

// ── CSV writing ─────────────────────────────────────────────────────────────

pub fn write_csv(results: &[BenchmarkResult], path: &str) -> Result<(), String> {
    let mut buf = String::new();
    writeln!(
        buf,
        "method,streams,time_per_frame,fps,cpu_ms_per_frame,memory,mem_per_stream,frames"
    )
    .unwrap();
    for r in results {
        writeln!(
            buf,
            "{},{},{},{},{},{},{},{}",
            r.method, r.streams, r.time_per_frame, r.fps, r.cpu_ms_per_frame, r.memory,
            r.mem_per_stream, r.frames
        )
        .unwrap();
    }
    fs::write(path, buf).map_err(|e| format!("Failed to write CSV: {}", e))
}

// ── Main benchmark orchestrator ─────────────────────────────────────────────

pub fn benchmark(
    input: &str,
    streams: i32,
    project_absolute_path: &str,
    results_absolute_path: &str,
    slides_config: &str,
    plots_folder: &str,
    is_single_threaded: i32,
    is_verbose: i32,
    write_to_csv: i32,
    video_type: &str,
    n_runs: usize,
) {
    let stream_steps = generate_stream_runs(streams);
    println!("Stream ranges to test: {:?}", stream_steps);
    println!(
        "Each stream count will be run {} time(s) and averaged.",
        n_runs
    );

    let mut all_results: Vec<BenchmarkResult> = Vec::new();

    for &s in &stream_steps {
        let results = run_benchmark_averaged(
            input,
            s,
            project_absolute_path,
            results_absolute_path,
            is_single_threaded,
            is_verbose,
            write_to_csv,
            n_runs,
        );
        if results.is_empty() {
            println!("Warning: No data returned for streams={}", s);
        }
        all_results.extend(results);
    }

    if all_results.is_empty() {
        println!("Error: all stream runs returned empty results. Check C++ binary and output format.");
        return;
    }

    let csv_path = format!("{}/benchmark_results.csv", plots_folder);
    if let Err(e) = write_csv(&all_results, &csv_path) {
        eprintln!("{}", e);
    } else {
        println!("Saved complete data table: {}", csv_path);
    }

    // ── Rust slide generation (disabled for now) ──
    // slides::produce_slides(
    //     &all_results,
    //     slides_config,
    //     "benchmark_comparison_slides.pptx",
    //     plots_folder,
    //     video_type,
    // );

    // ── Python slide generation via benchmarking/slides.py ──
    println!("Running Python slide generation via benchmarking.slides...");
    let venv_python = format!("{}/../venv-motion-vectors/bin/python3", project_absolute_path);
    let py_code = format!(
        "import pandas as pd; \
         import benchmarking.slides as s; \
         df = pd.read_csv('{csv}'); \
         s.produce_slides(df, '{cfg}', 'benchmark_comparison_slides.pptx', '{plots}', '{vtype}')",
        csv = csv_path,
        cfg = slides_config,
        plots = plots_folder,
        vtype = video_type,
    );

    let status = Command::new(&venv_python)
        .args(["-c", &py_code])
        .current_dir(project_absolute_path)
        .status();

    match status {
        Ok(s) if s.success() => println!("Python slide generation complete."),
        Ok(s) => eprintln!("Python slide generation exited with status: {}", s),
        Err(e) => eprintln!("Failed to run python slide generation: {}", e),
    }
}
