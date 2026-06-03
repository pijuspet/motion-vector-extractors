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

    #[cfg(unix)]
    pub fn get_vtune_env(&self) -> Option<Vec<(String, String)>> {
        let result = Command::new("sh")
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

    #[cfg(windows)]
    fn find_vtune() -> Option<std::path::PathBuf> {
        // Check PATH first (user may have run setvars.bat)
        if Command::new("vtune").arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status().map(|s| s.success()).unwrap_or(false)
        {
            return Some(std::path::PathBuf::from("vtune"));
        }

        // Check VTUNE_PROFILER_DIR env var (set by setvars.bat / oneAPI installer)
        if let Ok(dir) = std::env::var("VTUNE_PROFILER_DIR") {
            let candidate = std::path::Path::new(&dir).join("bin64").join("vtune.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        // Common install paths for Intel oneAPI on Windows
        let roots = [
            r"C:\Program Files (x86)\Intel\oneAPI",
            r"C:\Program Files\Intel\oneAPI",
        ];
        for root in &roots {
            let candidate = std::path::Path::new(root)
                .join("vtune").join("latest").join("bin64").join("vtune.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        None
    }

    #[cfg(windows)]
    pub fn profiler(&self) {
        let vtune = match Self::find_vtune() {
            Some(v) => v,
            None => {
                eprintln!(
                    "vtune not found. Either:\n  \
                     1. Run setvars.bat from the Intel oneAPI install dir to add it to PATH, or\n  \
                     2. Install Intel VTune from https://www.intel.com/vtune"
                );
                return;
            }
        };

        println!("Running VTune profiler on extractor4 (Windows)...");

        std::fs::create_dir_all(&self.vtune_dir).ok();

        let extractor_exec = self.extractor_executables.join("cust").join("extractor4.exe");
        let output_csv    = self.results_dir.join("method4_output_vtune.csv");

        let status = Command::new(&vtune)
            .args([
                "-collect", "hotspots",
                "-knob", "sampling-mode=sw",
                "-result-dir", &self.vtune_dir.to_string_lossy(),
                "--",
                &extractor_exec.to_string_lossy(),
                &self.video_file,
                "1",
                &output_csv.to_string_lossy(),
                "1", "1",
            ])
            .stdin(std::process::Stdio::null())
            .status();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => { eprintln!("vtune exited with status: {}", s); return; }
            Err(e) => { eprintln!("Failed to run vtune: {}", e); return; }
        }

        // Generate hotspots and top-down reports
        let vtune_hotspots_file = self.vtune_dir.join("hotspots.csv");
        for (report, output) in [
            ("hotspots",  vtune_hotspots_file.as_path()),
            ("top-down",  self.vtune_topdown_file.as_path()),
        ] {
            Command::new(&vtune)
                .args([
                    "-report", report,
                    "-result-dir", &self.vtune_dir.to_string_lossy(),
                    "-format", "csv",
                    "-report-output", &output.to_string_lossy(),
                ])
                .stdin(std::process::Stdio::null())
                .status()
                .ok();
        }

        if let Err(e) = crate::vtune_hotspots_plot::build_tree(&self.vtune_topdown_file.to_string_lossy()) {
            eprintln!("VTune tree build error: {}", e);
        }

        println!("Profiler run complete. Results in {}.", self.vtune_dir.display());
    }

    #[cfg(not(any(unix, windows)))]
    pub fn profiler(&self) {
        println!("VTune profiler step skipped: not supported on this platform.");
    }

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
            use inferno::flamegraph::{self, Options};
            use std::io::BufReader;
            let mut opts = Options::default();
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

        match crate::flamegraph::write_html(&svg_str, &output_html.to_string_lossy(), "extractor4 Flamegraph", 1.0) {
            Ok(()) => println!("Flamegraph saved to: {}", output_html.display()),
            Err(e) => eprintln!("Failed to write flamegraph HTML: {}", e),
        }
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
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