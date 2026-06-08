use std::process::Command;

use crate::full_benchmark::BenchmarkRunner;

impl BenchmarkRunner {
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
    pub fn find_vtune() -> Option<std::path::PathBuf> {
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
}
