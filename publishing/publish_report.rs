mod confluence;
mod confluence_report_generator;
mod report_generator;

use chrono::Local;
use rust_benchmarking::runner::BenchmarkRunner;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;


struct BenchmarkPublisher {
    project_root: PathBuf,
    results_path: PathBuf,
    repo_path: PathBuf,
    first_results_dir: PathBuf,
    first_git_commit: String,
    streams: i32,
    video: PathBuf
}

impl BenchmarkPublisher {
    fn new() -> Self {
        let project_root = env::current_dir().expect("Failed to get current directory");
        let results_path = project_root.join("results");
        let repo_path = project_root.join("ffmpeg");
        let first_results_dir = project_root.join("published").join("initial_results");
        let first_git_commit = "https://github.com/ablouise/ffmpeg-8.0-ourversion/commit/6faaff56c675b77dc783afc89a1dfb113c07bcf9".to_string();
        let streams = 15;
        let video = project_root.join("videos").join("vid_h264.mp4"); // need to add type
        // let video = self.project_root.join("videos").join("big_bunny.mp4");

        BenchmarkPublisher {
            project_root,
            results_path,
            repo_path,
            first_results_dir,
            first_git_commit,
            streams,
            video
        }
    }

    fn get_last_dir(&self, path: &Path) -> Option<PathBuf> {
        let mut dirs: Vec<PathBuf> = fs::read_dir(path)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();

        dirs.sort();
        dirs.last().cloned()
    }

    fn run_command(&self, cmd: &str, cwd: Option<&Path>) -> bool {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return false;
        }

        let mut command = Command::new(parts[0]);
        command.args(&parts[1..]);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }

        match command.status() {
            Ok(status) => status.success(),
            Err(e) => {
                eprintln!("Error executing command '{}': {}", cmd, e);
                false
            }
        }
    }

    fn run_command_capture(&self, cmd: &str) -> Option<String> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        let output = Command::new(parts[0])
            .args(&parts[1..])
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    fn run_shell_command(&self, cmd: &str) -> bool {
        match Command::new("/bin/bash")
            .args(["-c", cmd])
            .status()
        {
            Ok(status) => status.success(),
            Err(e) => {
                eprintln!("Error: {}", e);
                false
            }
        }
    }

    fn run_benchmark(&self) -> Option<PathBuf> {
        println!("DEBUG: Starting benchmark...");

        let benchmark_runner = BenchmarkRunner::new(
            self.video.to_str()?,
            "h264_cabac",
            "cust",
            self.streams,
            3
        );

        benchmark_runner.run_all();

        println!("DEBUG: Benchmark script finished.");
        self.get_last_dir(&self.results_path)
    }

    fn publish_git(&self) -> Option<String> {
        println!("Committing and pushing all changes to git in {}...", self.repo_path.display());

        self.run_command(
            &format!("git -C {} add .", self.repo_path.display()),
            None,
        );

        let commit_msg = format!(
            "Automated benchmark and report update {}",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        );

        // Don't track failure for commit (might be nothing to commit)
        self.run_shell_command(&format!(
            "git -C {} commit -m \"{}\"",
            self.repo_path.display(),
            commit_msg
        ));

        self.run_command(
            &format!("git -C {} push origin", self.repo_path.display()),
            None,
        );

        let commit_hash = self.run_command_capture(&format!(
            "git -C {} rev-parse HEAD",
            self.repo_path.display()
        ))?;

        let remote_url = self.run_command_capture(&format!(
            "git -C {} config --get remote.origin.url",
            self.repo_path.display()
        ));

        match remote_url {
            Some(url) => Some(format!("{}/commit/{}", url, commit_hash)),
            None => Some(commit_hash),
        }
    }

    fn publish_confluence(
        &self,
        first_dir: &str,
        latest_dir: &str,
        video_type: &str,
        git_commit_run1: &str,
        git_commit_run2: &str,
        use_predefined_git_commits: bool,
    ) {
        let (commit1, commit2) = if use_predefined_git_commits {
            let c2 = self.publish_git().unwrap_or_default();
            (self.first_git_commit.clone(), c2)
        } else {
            (git_commit_run1.to_string(), git_commit_run2.to_string())
        };

        println!("Publishing report to Confluence...");
        println!("  First results directory: {}", first_dir);
        println!("  Latest results directory: {}", latest_dir);
        println!("  Git commit run 1: {}", commit1);
        println!("  Git commit run 2: {}", commit2);

        if first_dir.is_empty()
            || latest_dir.is_empty()
            || commit1.is_empty()
            || commit2.is_empty()
        {
            eprintln!("Error: You must provide FIRST and LATEST results directories and both git commit URLs.");
            return;
        }

        if !Path::new(first_dir).is_dir() {
            eprintln!("Error: First results directory '{}' does not exist.", first_dir);
            return;
        }

        if !Path::new(latest_dir).is_dir() {
            eprintln!("Error: Latest results directory '{}' does not exist.", latest_dir);
            return;
        }

        if let Err(e) = report_generator::publish_to_confluence(
            first_dir,
            latest_dir,
            &commit1,
            &commit2,
            video_type,
            &self.project_root,
        ) {
            eprintln!("Error publishing to Confluence: {}", e);
            return;
        }

        // Copy results to published directory
        let published_dir = latest_dir.replace("/results/", "/published/");
        let published_path = Path::new(&published_dir);

        if published_path.exists() {
            println!("Published directory already exists: {}", published_dir);
        } else {
            if let Some(parent) = published_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            println!("Copying files to a published directory");
            self.run_command(
                &format!("cp -r {} {}", latest_dir, published_dir),
                None,
            );
            println!("Published results copied to: {}", published_dir);
        }
    }

    fn run_all(&self) {
        println!("Running full benchmark and publishing results...");
        let latest_results_dir = match self.run_benchmark() {
            Some(dir) => dir,
            None => {
                eprintln!("Benchmark failed, aborting.");
                return;
            }
        };
        let latest_dir_str = latest_results_dir.to_string_lossy().to_string();
        println!("Latest results directory: {}", latest_dir_str);

        let latest_git_commit = self.publish_git().unwrap_or_default();

        if let Err(e) = report_generator::publish_to_confluence(
            &self.first_results_dir.to_string_lossy(),
            &latest_dir_str,
            &self.first_git_commit,
            &latest_git_commit,
            "h264_cabac",
            &self.project_root,
        ) {
            eprintln!("Error publishing to Confluence: {}", e);
        }
    }
}

fn usage() {
    println!();
    println!("Select publish step to run (enter one or more numbers separated by space):");
    println!("  1: Run Full Benchmark");
    println!("  2: Commit to Git");
    println!("  3: Publish to Confluence");
    println!("  0: Run ALL (benchmark, git, confluence)");
    println!();
}

fn load_env_file(path: &Path) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: Failed to read {}: {}", path.display(), e);
            return;
        }
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !key.is_empty() {
                env::set_var(key, value);
            }
        }
    }
    println!("Loaded .env from {}", path.display());
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let publisher = BenchmarkPublisher::new();

    // Load .env from project root at startup
    let env_path = publisher.project_root.join(".env");
    load_env_file(&env_path);

    let choices: Vec<String> = if args.len() > 1 {
        args[1].split_whitespace().map(String::from).collect()
    } else {
        usage();
        print!("Choice(s): ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap_or(0);
        input.split_whitespace().map(String::from).collect()
    };

    for step in &choices {
        match step.as_str() {
            "1" => {
                publisher.run_benchmark();
            }
            "2" => {
                publisher.publish_git();
            }
            "3" => {
                // Args: <step> <first_dir> <latest_dir> <video_type> <git1> <git2> [use_predefined]
                if args.len() >= 7 {
                    let use_predefined = args.get(7).map(|s| !s.is_empty()).unwrap_or(false);
                    publisher.publish_confluence(
                        &args[2],
                        &args[3],
                        &args[4],
                        &args[5],
                        &args[6],
                        use_predefined,
                    );
                } else {
                    print!("Enter the path to the FIRST results directory: ");
                    io::stdout().flush().ok();
                    let mut first = String::new();
                    io::stdin().read_line(&mut first).ok();

                    print!("Enter the path to the LATEST results directory: ");
                    io::stdout().flush().ok();
                    let mut latest = String::new();
                    io::stdin().read_line(&mut latest).ok();

                    print!("Enter the Git commit URL for RUN 1: ");
                    io::stdout().flush().ok();
                    let mut git1 = String::new();
                    io::stdin().read_line(&mut git1).ok();

                    print!("Enter the Git commit URL for RUN 2: ");
                    io::stdout().flush().ok();
                    let mut git2 = String::new();
                    io::stdin().read_line(&mut git2).ok();

                    print!("Enter video type you've run experiments on: ");
                    io::stdout().flush().ok();
                    let mut vtype = String::new();
                    io::stdin().read_line(&mut vtype).ok();

                    publisher.publish_confluence(
                        first.trim(),
                        latest.trim(),
                        vtype.trim(),
                        git1.trim(),
                        git2.trim(),
                        false,
                    );
                }
            }
            "0" => {
                publisher.run_all();
                break;
            }
            _ => println!("Invalid step: {}", step),
        }
    }
}
