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
        let mut command = Command::new("/bin/bash");
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
        let mut command = Command::new("/bin/bash");
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

        let make_cmd = if self.build_type == "sys" {
            "make build_sys"
        } else {
            "make build"
        };

        if !self.run_command(make_cmd, Some(&self.current_dir), None) {
            return false;
        }

        let compile_cmd = "make build_tools";

        if !self.run_command(compile_cmd, Some(&self.current_dir), None) {
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

        let is_single_threaded = true; // true
        let is_verbose = true;
        let write_to_csv = true;

        if run_benchmark_extractors(
            &self.video_file,
            self.streams,
            &self.results_dir.to_string_lossy(),
            &self.current_dir.to_string_lossy(),
            is_single_threaded,
            is_verbose,
            write_to_csv,
        )
        .is_none()
        {
            return;
        }

        println!("Benchmarks complete.");
    }

    pub fn plot(&self) {
        if self.video_file.is_empty() {
            println!("Plotting step skipped: set VIDEO_FILE argument.");
            return;
        }

        fs::create_dir_all(&self.plots_dir).ok();

        let is_single_threaded = 1;
        let is_verbose = 0;
        let write_to_csv = 1;

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

    pub fn get_vtune_env(&self) -> Option<Vec<(String, String)>> {
        let result = Command::new("/bin/bash")
            .args(["-c", ". /opt/intel/oneapi/setvars.sh --force 2>/dev/null && env"])
            .output();

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut env_vars: Vec<(String, String)> = Vec::new();
                let mut has_vtune = false;

                for line in stdout.lines() {
                    if let Some((key, value)) = line.split_once('=') {
                        if key == "VTUNE_PROFILER_DIR" {
                            has_vtune = true;
                        }
                        env_vars.push((key.to_string(), value.to_string()));
                    }
                }

                if !has_vtune {
                    println!("Warning: setvars.sh did not set VTUNE_PROFILER_DIR — vtune may not be found.");
                }

                if env_vars.is_empty() {
                    None
                } else {
                    Some(env_vars)
                }
            }
            Err(e) => {
                println!("Warning: could not source setvars.sh: {}", e);
                None
            }
        }
    }

    pub fn profiler(&self) {
        println!("Running VTune profiler on extractor4 with motion_vectors_only=1...");

        let ffmpeg_lib = self
            .current_dir
            .join("ffmpeg")
            .join("ffmpeg-8.0-custom")
            .join("lib");

        fs::create_dir_all(&self.vtune_dir).ok();

        let do_print = 1;
        let is_verbose = 1;
        let is_single_threaded = 1;

        let extractor_exec = self
            .extractor_executables
            .join("extractor4");
        let output_csv = self
            .results_dir
            .join("method4_output_vtune.csv");

        let vtune_env = self.get_vtune_env().unwrap_or_else(|| {
            env::vars().map(|(k, v)| (k, v)).collect()
        });

        // Build LD_LIBRARY_PATH
        let existing_ld = vtune_env
            .iter()
            .find(|(k, _)| k == "LD_LIBRARY_PATH")
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        let ld_path = format!(
            "{}/libavutil:{}/libavformat:{}",
            ffmpeg_lib.display(),
            ffmpeg_lib.display(),
            existing_ld
        );

        let mut env_with_ld = vtune_env.clone();
        if let Some(entry) = env_with_ld.iter_mut().find(|(k, _)| k == "LD_LIBRARY_PATH") {
            entry.1 = ld_path.clone();
        } else {
            env_with_ld.push(("LD_LIBRARY_PATH".to_string(), ld_path));
        }

        let vtune_collect_cmd = format!(
            "vtune -collect hotspots -knob sampling-mode=sw -result-dir {} -- {} {} {} {} {} {}",
            self.vtune_dir.display(),
            extractor_exec.display(),
            self.video_file,
            do_print,
            output_csv.display(),
            is_verbose,
            is_single_threaded
        );

        if !self.run_shell_command(
            &vtune_collect_cmd,
            Some(&self.extractor_executables),
            Some(&env_with_ld),
        ) {
            return;
        }

        let vtune_hotspots_file = self.vtune_dir.join("hotspots.csv");
        let vtune_report_hotspots = format!(
            "vtune -report hotspots -result-dir {} -format csv -report-output {}",
            self.vtune_dir.display(),
            vtune_hotspots_file.display()
        );
        let vtune_report_topdown = format!(
            "vtune -report top-down -result-dir {} -format csv -report-output {}",
            self.vtune_dir.display(),
            self.vtune_topdown_file.display()
        );

        self.run_shell_command(&vtune_report_hotspots, None, Some(&env_with_ld));
        self.run_shell_command(&vtune_report_topdown, None, Some(&env_with_ld));

        if let Err(e) = crate::vtune_hotspots_plot::build_tree(&self.vtune_topdown_file.to_string_lossy()) {
            eprintln!("VTune tree build error: {}", e);
        }

        println!("Profiler run complete. Results in {}.", self.vtune_dir.display());
    }

    pub fn flamegraph(&self) {
        println!("Generating flamegraph for extractor4...");

        let flamegraph_dir = self.results_dir.join("flamegraph");
        fs::create_dir_all(&flamegraph_dir).ok();

        let output_html = flamegraph_dir.join("extractor4_flamegraph.html");

        if let Some(perf_bin) = crate::flamegraph::find_perf() {
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

            if let Err(e) = crate::flamegraph::flamegraph_from_perf(
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