use chrono::Local;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::benchmark::benchmark;
use crate::benchmark_extractors::run_benchmark_extractors;
use crate::mv_diff;

pub struct BenchmarkRunner {
    pub video_file: String,
    pub build_type: String,
    pub video_type: String,
    pub streams: i32,
    pub n_runs: usize,
    pub keyframes_only: bool,
    pub thread_count: i32,
    pub write_csv: bool,
    pub profiler_extractor: u32,
    pub current_dir: PathBuf,
    pub results_dir: PathBuf,
    pub pkg_config_path: PathBuf,
    pub extractor_executables: PathBuf,
    pub motion_vectors_comparison_file: PathBuf,
    pub slides_config: PathBuf,
    pub plots_dir: PathBuf,
    pub venv_dir: PathBuf,
    pub vtune_dir: PathBuf,
    pub vtune_topdown_file: PathBuf,
}

impl BenchmarkRunner {
    pub fn new(
        video_file: &str,
        video_type: &str,
        build_type: &str,
        streams: i32,
        n_runs: usize,
        thread_count: i32,
        keyframes_only: bool,
        write_csv: bool,
        profiler_extractor: u32,
    ) -> Self {
        let current_dir = env::current_dir().expect("Failed to get current directory");

        let results_base = current_dir.join("results");
        fs::create_dir_all(&results_base).ok();

        let results_type = results_base.join(video_type);
        fs::create_dir_all(&results_type).ok();

        let run_timestamp = Local::now().format("%Y%m%d_%H%M").to_string();

        let video_stem = std::path::Path::new(video_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("video")
            .to_string();

        let mut folder_name = format!("{}_{}_t{}", run_timestamp, video_stem, thread_count);
        if keyframes_only { folder_name.push_str("_kf"); }
        if write_csv      { folder_name.push_str("_csv"); }

        let results_dir = results_type.join(&folder_name);
        fs::create_dir_all(&results_dir).ok();

        let pkg_config_path = current_dir.join("ffmpeg").join("FFmpeg-8.0").join("lib").join("pkgconfig");

        let extractor_executables = current_dir.join("executables");

        let motion_vectors_comparison_file = results_dir.join("mv_comparison_result.txt");
        let slides_config = current_dir.join("scripts").join("slides_config.json");
        let plots_dir = results_dir.join("plots");

        #[cfg(windows)]
        let venv_dir = current_dir.join("venv-motion-vectors");
        #[cfg(not(windows))]
        let venv_dir = current_dir.join("..").join("venv-motion-vectors");

        let vtune_dir = results_dir.join("vtune_results");
        let vtune_topdown_file = vtune_dir.join("topdown.csv");

        BenchmarkRunner {
            video_file: video_file.to_string(),
            build_type: build_type.to_string(),
            video_type: video_type.to_string(),
            streams,
            n_runs,
            keyframes_only,
            thread_count,
            write_csv,
            profiler_extractor,
            current_dir,
            results_dir,
            pkg_config_path,
            extractor_executables,
            motion_vectors_comparison_file,
            slides_config,
            plots_dir,
            venv_dir,
            vtune_dir,
            vtune_topdown_file,
        }
    }

    pub fn run_command(&self, cmd: &str, cwd: Option<&PathBuf>, env_vars: Option<&[(String, String)]>) -> bool {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return false;
        }

        let mut command = Command::new(parts[0]);
        command.args(&parts[1..]);

        if let Some(dir) = cwd {
            command.current_dir(dir);
        }

        if let Some(vars) = env_vars {
            for (k, v) in vars {
                command.env(k, v);
            }
        }

        match command.status() {
            Ok(status) => status.success(),
            Err(e) => {
                eprintln!("Error executing command '{}': {}", cmd, e);
                false
            }
        }
    }

    pub fn run_shell_command(&self, cmd: &str, cwd: Option<&PathBuf>, env_vars: Option<&[(String, String)]>) -> bool {
        let mut command = Command::new("sh");
        command.args(["-c", cmd]);

        if let Some(dir) = cwd {
            command.current_dir(dir);
        }

        if let Some(vars) = env_vars {
            for (k, v) in vars {
                command.env(k, v);
            }
        }

        match command.status() {
            Ok(status) => status.success(),
            Err(e) => {
                eprintln!("Error executing shell command: {}", e);
                false
            }
        }
    }

    pub fn run_shell_capture(&self, cmd: &str, env_vars: Option<&[(String, String)]>) -> Option<String> {
        let mut command = Command::new("sh");
        command.args(["-c", cmd]);

        if let Some(vars) = env_vars {
            for (k, v) in vars {
                command.env(k, v);
            }
        }

        match command.output() {
            Ok(output) if output.status.success() => {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            }
            Ok(output) => {
                eprintln!("Command failed: {}", String::from_utf8_lossy(&output.stderr));
                None
            }
            Err(e) => {
                eprintln!("Error executing command: {}", e);
                None
            }
        }
    }

    pub fn build(&self) -> bool {
        println!("Building all extractors and tools...");

        #[cfg(windows)]
        let mf = "-f makefile.windows ";
        #[cfg(not(windows))]
        let mf = "";

        let target = if self.build_type == "sys" { "build_sys" } else { "build" };
        let make_cmd = format!("make {}{}", mf, target);
        let compile_cmd = format!("make {}build_tools", mf);

        if !self.run_command(&make_cmd, Some(&self.current_dir), None) {
            return false;
        }
        if !self.run_command(&compile_cmd, Some(&self.current_dir), None) {
            return false;
        }

        println!("Build complete.");
        true
    }

    pub fn extract(&self) {
        if self.video_file.is_empty() {
            println!("Extraction step skipped: set VIDEO_FILE environment variable to input file.");
            return;
        }

        println!("Running 9-method benchmark suite...");

        let results = match run_benchmark_extractors(
            &self.video_file,
            self.streams,
            &self.results_dir.to_string_lossy(),
            &self.current_dir.to_string_lossy(),
            true,
            self.write_csv,
            self.keyframes_only,
            self.thread_count,
        ) {
            Some(results) => results,
            None => return,
        };

        println!("Benchmarks complete.");

        self.report_speedup(&results);
    }

    /// Compare "Original FFmpeg MV only" (extractor0) against "Custom FFmpeg"
    /// (extractor5) and print whether the expected speedup showed up.
    ///
    /// Report-only by design: never changes exit code.
    pub fn report_speedup(&self, results: &[crate::benchmark::BenchmarkResult]) {
        const SPEEDUP_MIN_RATIO: f64 = 1.2;
        const ORIG_METHOD: &str = "Original FFmpeg MV only";
        const CUST_METHOD: &str = "Custom FFmpeg";

        let original: Vec<&crate::benchmark::BenchmarkResult> =
            results.iter().filter(|r| r.method == ORIG_METHOD).collect();
        let custom: Vec<&crate::benchmark::BenchmarkResult> =
            results.iter().filter(|r| r.method == CUST_METHOD).collect();

        println!();
        println!("============================================================");
        println!("  SPEEDUP CHECK  ({ORIG_METHOD}  vs  {CUST_METHOD})");
        println!("============================================================");

        if original.is_empty() || custom.is_empty() {
            println!(
                "  Skipped: need both methods (got {} original rows, {} custom rows).",
                original.len(), custom.len()
            );
            println!("============================================================");
            return;
        }

        println!("  {:>7}  {:>10}  {:>10}  {:>10}  {:>10}  {:>8}",
            "Streams", "Orig FPS", "Orig ms", "Cust FPS", "Cust ms", "Speedup");
        println!("  {}", "-".repeat(63));

        let mut csv_rows: Vec<(i32, f64, f64, f64, f64, f64)> = Vec::new();
        let mut stream_counts: Vec<i32> = original.iter().map(|r| r.streams).collect();
        stream_counts.sort();
        stream_counts.dedup();

        for &streams in &stream_counts {
            let orig = original.iter().find(|r| r.streams == streams);
            let cust = custom.iter().find(|r| r.streams == streams);
            if let (Some(o), Some(c)) = (orig, cust) {
                let speedup = if o.fps > 0.0 { c.fps / o.fps } else { 0.0 };
                println!("  {:>7}  {:>10.1}  {:>10.2}  {:>10.1}  {:>10.2}  {:>7.2}×",
                    streams, o.fps, o.time_per_frame, c.fps, c.time_per_frame, speedup);
                csv_rows.push((streams, o.fps, o.time_per_frame, c.fps, c.time_per_frame, speedup));
            }
        }

        if csv_rows.is_empty() {
            println!("  No matching stream counts between the two methods.");
            println!("============================================================");
            return;
        }

        let n = csv_rows.len() as f64;
        let ratio    = csv_rows.iter().map(|r| r.5).sum::<f64>() / n;
        let orig_fps = csv_rows.iter().map(|r| r.1).sum::<f64>() / n;
        let orig_ms  = csv_rows.iter().map(|r| r.2).sum::<f64>() / n;
        let cust_fps = csv_rows.iter().map(|r| r.3).sum::<f64>() / n;
        let cust_ms  = csv_rows.iter().map(|r| r.4).sum::<f64>() / n;

        println!("  {}", "-".repeat(63));
        println!("  Mean speedup  : {:>8.2}×  (threshold >= {:.2}×)", ratio, SPEEDUP_MIN_RATIO);

        let verdict = if ratio >= SPEEDUP_MIN_RATIO {
            "SPEEDUP CONFIRMED"
        } else if ratio >= 1.0 {
            "WARNING: custom faster, but below the speedup threshold"
        } else {
            "REGRESSION: custom is SLOWER than original"
        };
        println!("  Verdict       : {}", verdict);
        println!("============================================================");

        self.publish_github_report(verdict, ratio, SPEEDUP_MIN_RATIO,
            orig_fps, orig_ms, original.len(), cust_fps, cust_ms, custom.len());
    }

    /// Mirror the speedup verdict into GitHub Actions' reporting surfaces so it
    /// shows up in the run's UI, not just buried in the step log:
    ///   - `$GITHUB_STEP_SUMMARY`: a markdown table rendered on the run's
    ///     summary page (the "report").
    ///   - a `::notice::` annotation (gated on `GITHUB_ACTIONS=true`) that
    ///     surfaces the headline at the top of the run.
    /// Both are no-ops locally, where these env vars are unset.
    #[allow(clippy::too_many_arguments)]
    fn publish_github_report(
        &self,
        verdict: &str,
        ratio: f64,
        threshold: f64,
        orig_fps: f64,
        orig_ms: f64,
        orig_n: usize,
        cust_fps: f64,
        cust_ms: f64,
        cust_n: usize,
    ) {
        let vtype = &self.video_type;
        let video = &self.video_file;
        let streams = self.streams;
        let runs = self.n_runs;

        if let Ok(summary_path) = env::var("GITHUB_STEP_SUMMARY") {
            let md = format!(
                "## Speedup check — {vtype}\n\n\
                 **{verdict}** — custom is **{ratio:.2}×** vs original (threshold ≥ {threshold:.2}×)\n\n\
                 | Build | Mean throughput | Mean ms/frame | Methods |\n\
                 | --- | ---: | ---: | ---: |\n\
                 | Original FFmpeg | {orig_fps:.1} FPS | {orig_ms:.2} | {orig_n} |\n\
                 | Custom FFmpeg | {cust_fps:.1} FPS | {cust_ms:.2} | {cust_n} |\n\n\
                 <sub>streams: {streams} · runs: {runs} · video: {video}</sub>\n\n"
            );
            use std::io::Write as _;
            match fs::OpenOptions::new().create(true).append(true).open(&summary_path) {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(md.as_bytes()) {
                        eprintln!("Could not write GITHUB_STEP_SUMMARY: {}", e);
                    }
                }
                Err(e) => eprintln!("Could not open GITHUB_STEP_SUMMARY: {}", e),
            }
        }

        if env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
            // Annotation message can't span lines; keep it to one headline.
            println!(
                "::notice title=Speedup check ({vtype})::{verdict} — {ratio:.2}x \
                 (custom {cust_fps:.0} FPS vs original {orig_fps:.0} FPS)"
            );
        }
    }

    pub fn plot(&self) {
        if self.video_file.is_empty() {
            println!("Plotting step skipped: set VIDEO_FILE argument.");
            return;
        }

        fs::create_dir_all(&self.plots_dir).ok();

        let is_verbose = 0;
        let write_to_csv = 0;

        println!("Running Rust benchmark visualization and PPT generation...");
        benchmark(
            &self.video_file,
            self.streams,
            &self.current_dir.to_string_lossy(),
            &self.results_dir.to_string_lossy(),
            &self.slides_config.to_string_lossy(),
            &self.plots_dir.to_string_lossy(),
            is_verbose,
            write_to_csv,
            &self.video_type,
            self.n_runs,
            self.keyframes_only,
            self.thread_count,
        );

        println!("Plotting complete. Plots and PPTX in {}.", self.plots_dir.display());
    }

    pub fn generate_mv_comparison(&self) {
        // Which two methods this full (both-lists) sanity check compares —
        // makefile variables COMPARE_FIRST/COMPARE_SECOND (default 1/4, the
        // historical pairing: both built from extractor1.rs, one against the
        // regular FFmpeg and one against the custom fork). Logged as
        // "first"/"second" rather than bare method numbers so the labels
        // stay meaningful regardless of which pair is configured.
        let first: u32 = std::env::var("COMPARE_FIRST").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        let second: u32 = std::env::var("COMPARE_SECOND").ok().and_then(|v| v.parse().ok()).unwrap_or(4);

        // method1 and method4 (the default pairing) are both built from
        // extractor1.rs — method1 against the regular (unpatched) FFmpeg,
        // where the mv_l0_only AVOption doesn't exist (silently ignored, so
        // it always emits both lists), and method4 against the custom-patched
        // fork, where it does apply. Under the L0_ONLY default (see the
        // makefile), method4 drops every list-1 row while method1 can't — an
        // unfiltered compare would report every one of those as a spurious
        // diff, not a real decoder mismatch. Skip this pairing while L0_ONLY
        // is active; the list-0 comparison below (method0 vs method9) already
        // filters source==-1 on both sides so it stays valid either way.
        let l0_only = std::env::var("L0_ONLY").map(|v| v != "0").unwrap_or(true);
        if l0_only {
            println!(
                "MV comparison (first=method{first} vs second=method{second}): skipped — \
                 L0_ONLY is set, so first (regular FFmpeg, always both lists) and second \
                 (custom FFmpeg, list-0 only) aren't comparable. Run with L0_ONLY=0 to \
                 include this check."
            );
        } else {
            let first_csv = self.results_dir.join(format!("method{first}_output_0.csv"));
            let second_csv = self.results_dir.join(format!("method{second}_output_0.csv"));

            println!("MV comparison: first=method{first} second=method{second}");
            if let Err(e) = mv_types::mv_compare::compare(
                &first_csv.to_string_lossy(),
                &second_csv.to_string_lossy(),
                &self.motion_vectors_comparison_file.to_string_lossy(),
            ) {
                eprintln!("MV comparison error: {}", e);
            }
        }

        self.generate_mv_comparison_neg1();
    }

    /// Compare method0 (original FFmpeg) against method9 (custom from-scratch
    /// parser), list-0 (`source == -1`, backward-reference) motion vectors
    /// only — the same comparison `make compare_mvs` runs standalone, folded
    /// into the interactive benchmark flow so it doesn't need a second
    /// invocation. See [`mv_bench::mv_diff::compare_list0`] for the display-
    /// order remap this needs and why.
    ///
    /// Report-only: missing CSVs (e.g. this run didn't include methods 0/9)
    /// just skip the step rather than failing the benchmark.
    fn generate_mv_comparison_neg1(&self) {
        let method0_csv = self.results_dir.join("method0_output_0.csv");
        let method9_csv = self.results_dir.join("method9_output_0.csv");
        if !method0_csv.exists() || !method9_csv.exists() {
            println!(
                "MV comparison (list-0): skipped, method0/method9 CSVs not found in {}",
                self.results_dir.display()
            );
            return;
        }

        // Regular (unpatched) FFmpeg-8.0's ffprobe, deliberately not the
        // custom-patched build: the custom decoder's full-decode path has a
        // pre-existing bug that can segfault on B-frame content when
        // motion_vectors_only isn't set, which plain ffprobe never sets.
        // Packet PTS extraction doesn't need any custom-decoder feature, so
        // this sidesteps the bug instead of tripping over it.
        let ffprobe = self
            .current_dir
            .join("ffmpeg")
            .join("FFmpeg-8.0")
            .join("bin")
            .join("ffprobe");
        let ffprobe_libdir = self
            .current_dir
            .join("ffmpeg")
            .join("FFmpeg-8.0")
            .join("lib");
        let pkt_order_path = self.results_dir.join("pkt_order.txt");

        let output = std::process::Command::new(&ffprobe)
            .args([
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "packet=pts_time",
                "-of", "csv=p=0",
            ])
            .arg(&self.video_file)
            .env("LD_LIBRARY_PATH", &ffprobe_libdir)
            .output();
        let output = match output {
            Ok(o) if o.status.success() => o,
            Ok(o) => {
                eprintln!(
                    "MV comparison (list-0): ffprobe failed: {}",
                    String::from_utf8_lossy(&o.stderr)
                );
                return;
            }
            Err(e) => {
                eprintln!("MV comparison (list-0): could not run ffprobe ({}): {}", ffprobe.display(), e);
                return;
            }
        };
        if let Err(e) = fs::write(&pkt_order_path, &output.stdout) {
            eprintln!("MV comparison (list-0): could not write {}: {}", pkt_order_path.display(), e);
            return;
        }

        let output_file = self.results_dir.join("mv_diff_neg1.txt");
        match mv_diff::compare_list0(
            &method0_csv.to_string_lossy(),
            &method9_csv.to_string_lossy(),
            &pkt_order_path.to_string_lossy(),
            &output_file,
        ) {
            Ok(summary) => {
                println!(
                    "MV comparison (list-0): method0={} method9={} differences={} -> {}",
                    summary.method0_rows, summary.method9_rows, summary.diffs, output_file.display()
                );
            }
            Err(e) => eprintln!("MV comparison (list-0): {}", e),
        }
    }

    pub fn run_all(&self) {
        if !self.build() {
            println!("Build failed, aborting.");
            return;
        }
        self.extract();
        self.generate_mv_comparison();
        self.plot();
        self.profiler();
        self.flamegraph();
    }
}