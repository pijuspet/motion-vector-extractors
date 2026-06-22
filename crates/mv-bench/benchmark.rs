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

// ── Run benchmark subprocess ────────────────────────────────────────────────

pub fn run_benchmark(
    input_file: &str,
    streams: i32,
    project_absolute_path: &str,
    results_absolute_path: &str,
    is_verbose: i32,
    write_to_csv: i32,
    keyframes_only: bool,
    thread_count: i32,
) -> Vec<BenchmarkResult> {
    println!("Running benchmark with {} streams...", streams);

    run_benchmark_extractors(
        input_file,
        streams,
        results_absolute_path,
        project_absolute_path,
        is_verbose != 0,
        write_to_csv != 0,
        keyframes_only,
        thread_count,
    )
    .unwrap_or_default()
}

// ── Averaged runs ───────────────────────────────────────────────────────────

pub fn run_benchmark_averaged(
    input_file: &str,
    streams: i32,
    project_absolute_path: &str,
    results_absolute_path: &str,
    is_verbose: i32,
    write_to_csv: i32,
    n_runs: usize,
    keyframes_only: bool,
    thread_count: i32,
) -> Vec<BenchmarkResult> {
    let mut all_runs: Vec<Vec<BenchmarkResult>> = Vec::new();

    for run_idx in 1..=n_runs {
        println!("  Run {}/{} for {} streams...", run_idx, n_runs, streams);
        let results = run_benchmark(
            input_file,
            streams,
            project_absolute_path,
            results_absolute_path,
            is_verbose,
            write_to_csv,
            keyframes_only,
            thread_count,
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

    // Preserve the canonical method order from the first run instead of
    // iterating the HashMap (which has random order).
    let method_order: Vec<String> = {
        let mut seen = Vec::new();
        for r in &all_runs[0] {
            if !seen.contains(&r.method) {
                seen.push(r.method.clone());
            }
        }
        seen
    };

    let mut averaged = Vec::new();
    for method in &method_order {
        let key = (method.clone(), streams);
        if let Some(entries) = groups.get(&key) {
            let n = entries.len() as f64;
            averaged.push(BenchmarkResult {
                method: method.clone(),
                streams,
                time_per_frame: entries.iter().map(|e| e.time_per_frame).sum::<f64>() / n,
                fps: entries.iter().map(|e| e.fps).sum::<f64>() / n,
                cpu_ms_per_frame: entries.iter().map(|e| e.cpu_ms_per_frame).sum::<f64>() / n,
                memory: entries.iter().map(|e| e.memory).sum::<f64>() / n,
                mem_per_stream: entries.iter().map(|e| e.mem_per_stream).sum::<f64>() / n,
                frames: (entries.iter().map(|e| e.frames as f64).sum::<f64>() / n) as i32,
            });
        }
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
    is_verbose: i32,
    write_to_csv: i32,
    video_type: &str,
    n_runs: usize,
    keyframes_only: bool,
    thread_count: i32,
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
            is_verbose,
            write_to_csv,
            n_runs,
            keyframes_only,
            thread_count,
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

    // ── Python slide generation via scripts/slides.py ──
    println!("Running Python slide generation...");

    let threads_str = if thread_count == 0 { "auto".to_string() } else { thread_count.to_string() };
    let run_info = format!(
        "Keyframes only: {} | Threads: {}",
        if keyframes_only { "yes" } else { "no" },
        threads_str,
    );

    #[cfg(windows)]
    let venv_python = format!("{}/venv-motion-vectors/bin/python.exe", project_absolute_path);
    #[cfg(not(windows))]
    let venv_python = format!("{}/../venv-motion-vectors/bin/python3", project_absolute_path);
    let scripts_dir = format!("{}/scripts", project_absolute_path);
    // Normalise to forward slashes: Windows paths embedded in a Python string
    // literal cause SyntaxError because \U, \t, \D etc. are unicode escapes.
    let scripts_dir   = scripts_dir.replace('\\', "/");
    let csv_path_fwd  = csv_path.replace('\\', "/");
    let cfg_fwd       = slides_config.replace('\\', "/");
    let plots_fwd     = plots_folder.replace('\\', "/");
    let py_code = format!(
        "import sys; sys.path.insert(0, '{scripts}'); \
         import pandas as pd; \
         import slides as s; \
         df = pd.read_csv('{csv}'); \
         s.produce_slides(df, '{cfg}', 'benchmark_comparison_slides.pptx', '{plots}', '{vtype}', '{rinfo}')",
        scripts = scripts_dir,
        csv = csv_path_fwd,
        cfg = cfg_fwd,
        plots = plots_fwd,
        vtype = video_type,
        rinfo = run_info,
    );

    // On Windows the Python child inherits the Windows system PATH, not
    // MSYS2's ~/.bashrc PATH. Prepend the wkhtmltopdf bin dir explicitly so
    // imgkit can find wkhtmltoimage.exe regardless of system PATH setup.
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new(&venv_python);
        c.args(["-c", &py_code]).current_dir(project_absolute_path);
        let sys_path = std::env::var("PATH").unwrap_or_default();
        let wk_bin = "C:\\Program Files\\wkhtmltopdf\\bin";
        if std::path::Path::new(wk_bin).exists() {
            c.env("PATH", format!("{};{}", wk_bin, sys_path));
        }
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new(&venv_python);
        c.args(["-c", &py_code]).current_dir(project_absolute_path);
        c
    };
    let status = cmd.status();

    match status {
        Ok(s) if s.success() => println!("Python slide generation complete."),
        Ok(s) => eprintln!("Python slide generation exited with status: {}", s),
        Err(e) => eprintln!("Failed to run python slide generation: {}", e),
    }
}
