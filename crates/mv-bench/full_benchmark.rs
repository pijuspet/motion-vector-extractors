use chrono::Local;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::benchmark::benchmark;
use crate::benchmark_extractors::run_benchmark_extractors;

pub struct BenchmarkRunner {
    pub video_file: String,
    pub build_type: String,
    pub video_type: String,
    pub streams: i32,
    pub n_runs: usize,
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
    ) -> Self {
        let current_dir = env::current_dir().expect("Failed to get current directory");

        let results_base = current_dir.join("results");
        fs::create_dir_all(&results_base).ok();

        let results_type = results_base.join(video_type);
        fs::create_dir_all(&results_type).ok();

        let run_timestamp = Local::now().format("%Y%m%d_%H%M").to_string();
        let results_dir = results_type.join(&run_timestamp);
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

        let is_single_threaded = false; // true
        let is_verbose = true;
        let write_to_csv = true;

        let results = match run_benchmark_extractors(
            &self.video_file,
            self.streams,
            &self.results_dir.to_string_lossy(),
            &self.current_dir.to_string_lossy(),
            is_single_threaded,
            is_verbose,
            write_to_csv,
        ) {
            Some(results) => results,
            None => return,
        };

        println!("Benchmarks complete.");

        self.report_speedup(&results);
    }

    /// Compare the custom-FFmpeg methods against the original-FFmpeg methods
    /// and print whether the expected speedup actually showed up. This runs
    /// right after the extract pass (`make benchmark STEPS=2`) so the
    /// custom-vs-original verdict is visible without needing the plot step.
    ///
    /// Report-only by design: it never changes the exit code, so a missing or
    /// shrunk speedup is surfaced loudly but does not fail the build/CI.
    ///
    /// "Speedup confirmed" requires the mean custom throughput to beat the mean
    /// original throughput by at least `SPEEDUP_MIN_RATIO`. The small margin
    /// keeps the tiny CI smoke run (few streams, NRUNS=1) from flipping the
    /// verdict on timing noise, while the historical gap (~2x) clears it easily.
    pub fn report_speedup(&self, results: &[crate::benchmark::BenchmarkResult]) {
        const SPEEDUP_MIN_RATIO: f64 = 1.2;

        let original: Vec<&crate::benchmark::BenchmarkResult> =
            results.iter().filter(|r| r.method.starts_with("Original")).collect();
        let custom: Vec<&crate::benchmark::BenchmarkResult> =
            results.iter().filter(|r| r.method.starts_with("Custom")).collect();

        println!();
        println!("============================================================");
        println!("  SPEEDUP CHECK  (custom FFmpeg vs original FFmpeg)");
        println!("============================================================");

        if original.is_empty() || custom.is_empty() {
            println!(
                "  Skipped: need both groups (got {} original, {} custom).",
                original.len(),
                custom.len()
            );
            println!("============================================================");
            return;
        }

        let mean_fps = |group: &[&crate::benchmark::BenchmarkResult]| -> f64 {
            group.iter().map(|r| r.fps).sum::<f64>() / group.len() as f64
        };
        let mean_ms = |group: &[&crate::benchmark::BenchmarkResult]| -> f64 {
            group.iter().map(|r| r.time_per_frame).sum::<f64>() / group.len() as f64
        };

        let orig_fps = mean_fps(&original);
        let cust_fps = mean_fps(&custom);
        let orig_ms = mean_ms(&original);
        let cust_ms = mean_ms(&custom);
        let ratio = if orig_fps > 0.0 { cust_fps / orig_fps } else { 0.0 };
        let on = original.len();
        let cn = custom.len();

        println!(
            "  Original mean : {:>8.1} FPS   ({:>7.2} ms/frame, {} methods)",
            orig_fps, orig_ms, on
        );
        println!(
            "  Custom   mean : {:>8.1} FPS   ({:>7.2} ms/frame, {} methods)",
            cust_fps, cust_ms, cn
        );
        println!(
            "  Speedup       : {:>8.2}x  (threshold >= {:.2}x)",
            ratio, SPEEDUP_MIN_RATIO
        );

        let (verdict, emoji) = if ratio >= SPEEDUP_MIN_RATIO {
            ("SPEEDUP CONFIRMED", "⚡")
        } else if ratio >= 1.0 {
            ("WARNING: custom faster, but below the speedup threshold", "⚠️")
        } else {
            ("REGRESSION: custom is SLOWER than original", "🛑")
        };
        println!("  Verdict       : {}", verdict);
        println!("============================================================");

        self.publish_github_report(emoji, verdict, ratio, SPEEDUP_MIN_RATIO,
            orig_fps, orig_ms, on, cust_fps, cust_ms, cn);
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
        emoji: &str,
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
                "## {emoji} Speedup check — {vtype}\n\n\
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

        let is_single_threaded = 1;
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
            is_single_threaded,
            is_verbose,
            write_to_csv,
            &self.video_type,
            self.n_runs,
        );

        println!("Plotting complete. Plots and PPTX in {}.", self.plots_dir.display());
    }

    pub fn generate_mv_comparison(&self) {
        let method1_csv = self.results_dir.join("method1_output_0.csv");
        let method4_csv = self.results_dir.join("method4_output_0.csv");

        if let Err(e) = mv_types::mv_compare::compare(
            &method1_csv.to_string_lossy(),
            &method4_csv.to_string_lossy(),
            &self.motion_vectors_comparison_file.to_string_lossy(),
        ) {
            eprintln!("MV comparison error: {}", e);
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