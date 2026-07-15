mod confluence;
mod confluence_report_generator;
mod report_generator;

use chrono::{Local, NaiveDateTime, TimeZone};
use mv_bench::full_benchmark::BenchmarkRunner;
use regex::Regex;
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
        // Matches the makefile's INITIAL_RUN_DATA convention
        // (published/$(VIDEO_TYPE)/initial_results_$(VIDEO_TYPE)); the old
        // published/initial_results path doesn't exist on disk.
        let first_results_dir = project_root
            .join("published")
            .join("h264_cabac")
            .join("initial_results_h264_cabac");
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

    fn run_command(&self, args: &[&str], cwd: Option<&Path>) -> bool {
        let (cmd, rest) = match args.split_first() {
            Some(v) => v,
            None => return false,
        };

        let mut command = Command::new(cmd);
        command.args(rest);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }

        match command.status() {
            Ok(status) => status.success(),
            Err(e) => {
                eprintln!("Error executing command '{:?}': {}", args, e);
                false
            }
        }
    }

    fn run_command_capture(&self, args: &[&str], cwd: Option<&Path>) -> Option<String> {
        let (cmd, rest) = args.split_first()?;

        let mut command = Command::new(cmd);
        command.args(rest);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        let output = command.output().ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }


    fn run_benchmark(&self) -> Option<PathBuf> {
        println!("DEBUG: Starting benchmark...");

        let benchmark_runner = BenchmarkRunner::new(
            self.video.to_str()?,
            "h264_cabac",
            "cust",
            self.streams,
            3,
            0,
            false,
            false,
            4,
        );

        benchmark_runner.run_all();

        println!("DEBUG: Benchmark script finished.");
        self.get_last_dir(&self.results_path)
    }

    fn publish_git(&self) -> Option<String> {
        println!("Committing and pushing all changes to git in {}...", self.repo_path.display());

        let repo = &self.repo_path;
        self.run_command(&["git", "add", "."], Some(repo));

        let commit_msg = format!(
            "Automated benchmark and report update {}",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        );

        // Don't track failure for commit (might be nothing to commit)
        self.run_command(&["git", "commit", "--no-gpg-sign", "-m", &commit_msg], Some(repo));

        self.run_command(&["git", "push", "origin"], Some(repo));

        let commit_hash = self.run_command_capture(&["git", "rev-parse", "HEAD"], Some(repo))?;

        let remote_url = self.run_command_capture(&["git", "config", "--get", "remote.origin.url"], Some(repo));

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
            self.run_command(&["cp", "-r", latest_dir, &published_dir], None);
            println!("Published results copied to: {}", published_dir);
        }
    }

    /// Commit history of the *nested* FFmpeg source repo (ffmpeg/FFmpeg-8.0-custom/FFmpeg),
    /// which is what actually tracks the decoder changes behind these benchmark runs.
    /// Returns (commit_unix_timestamp, full_hash) sorted oldest to newest.
    fn get_ffmpeg_commit_history(&self) -> Vec<(i64, String)> {
        let nested_repo = self.repo_path.join("FFmpeg-8.0-custom").join("FFmpeg");
        let log_output = self
            .run_command_capture(&["git", "log", "--pretty=format:%ct %H"], Some(&nested_repo))
            .unwrap_or_default();

        let mut commits: Vec<(i64, String)> = log_output
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, ' ');
                let ts: i64 = parts.next()?.parse().ok()?;
                let hash = parts.next()?.to_string();
                Some((ts, hash))
            })
            .collect();
        commits.sort_by_key(|(ts, _)| *ts);
        commits
    }

    fn get_ffmpeg_remote_url(&self) -> Option<String> {
        let nested_repo = self.repo_path.join("FFmpeg-8.0-custom").join("FFmpeg");
        self.run_command_capture(
            &["git", "config", "--get", "remote.origin.url"],
            Some(&nested_repo),
        )
    }

    /// Parses the leading `YYYYMMDD_HHMM` prefix every results/bulk/* folder name
    /// starts with (with or without a trailing _<video>_t<threads> suffix) into a
    /// unix timestamp, interpreting it in the local timezone (matching how these
    /// folder names get generated by chrono::Local::now() elsewhere in this crate).
    fn parse_folder_timestamp(name: &str) -> Option<i64> {
        let re = Regex::new(r"^(\d{8})_(\d{4})").ok()?;
        let caps = re.captures(name)?;
        let date_str = format!("{} {}", &caps[1], &caps[2]);
        let naive = NaiveDateTime::parse_from_str(&date_str, "%Y%m%d %H%M").ok()?;
        let local = Local.from_local_datetime(&naive).single()?;
        Some(local.timestamp())
    }

    /// The commit that was HEAD as of `folder_ts`: the newest commit whose own
    /// timestamp is <= folder_ts. Falls back to the earliest known commit if the
    /// folder predates all recorded history (shouldn't happen for this repo, but
    /// better than silently producing no link).
    fn commit_url_for_timestamp(
        commits: &[(i64, String)],
        remote: &str,
        folder_ts: i64,
    ) -> Option<String> {
        let chosen = commits
            .iter()
            .rev()
            .find(|(ts, _)| *ts <= folder_ts)
            .or_else(|| commits.first())?;
        Some(format!(
            "{}/commit/{}",
            remote.trim_end_matches(".git"),
            chosen.1
        ))
    }

    /// Creates one detailed Confluence report page per results/bulk/* run,
    /// each tagged with the FFmpeg commit that was HEAD at that run's timestamp.
    /// create_detailed_report_page() is idempotent (skips a title that already
    /// exists), so this is safe to re-run. Does NOT touch the main dashboard
    /// summary page.
    fn publish_all_bulk_runs(&self) {
        let bulk_dir = self.results_path.join("bulk");
        let mut dirs: Vec<PathBuf> = match fs::read_dir(&bulk_dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect(),
            Err(e) => {
                eprintln!("Cannot read {}: {}", bulk_dir.display(), e);
                return;
            }
        };
        dirs.sort();
        println!("Found {} runs under {}", dirs.len(), bulk_dir.display());

        let commits = self.get_ffmpeg_commit_history();
        if commits.is_empty() {
            eprintln!(
                "No commit history found in {}/FFmpeg-8.0-custom/FFmpeg — aborting.",
                self.repo_path.display()
            );
            return;
        }
        let remote = match self.get_ffmpeg_remote_url() {
            Some(r) => r,
            None => {
                eprintln!("Could not determine FFmpeg repo remote URL — aborting.");
                return;
            }
        };

        let confluence_url = match env::var("CONFLUENCE_URL") {
            Ok(v) => v,
            Err(_) => { eprintln!("CONFLUENCE_URL not set"); return; }
        };
        let space_key = match env::var("SPACE_KEY") {
            Ok(v) => v,
            Err(_) => { eprintln!("SPACE_KEY not set"); return; }
        };
        let main_page_title = match env::var("MAIN_PAGE_TITLE") {
            Ok(v) => v,
            Err(_) => { eprintln!("MAIN_PAGE_TITLE not set"); return; }
        };
        let username = match env::var("CONFLUENCE_USER") {
            Ok(v) => v,
            Err(_) => { eprintln!("CONFLUENCE_USER not set"); return; }
        };
        let api_token = match env::var("CONFLUENCE_TOKEN") {
            Ok(v) => v,
            Err(_) => { eprintln!("CONFLUENCE_TOKEN not set"); return; }
        };

        let generator = confluence_report_generator::ConfluenceReportGenerator::new(
            &confluence_url,
            &username,
            &api_token,
            &space_key,
            &main_page_title,
            "h264_cabac",
            &self.project_root,
        );

        let mut published = 0;
        let mut skipped = 0;
        let mut failed = 0;

        for dir in &dirs {
            let name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
            let folder_ts = match Self::parse_folder_timestamp(&name) {
                Some(ts) => ts,
                None => {
                    eprintln!("[SKIP] {}: couldn't parse a YYYYMMDD_HHMM timestamp", name);
                    skipped += 1;
                    continue;
                }
            };
            let commit_url = Self::commit_url_for_timestamp(&commits, &remote, folder_ts);
            println!("{} -> {}", name, commit_url.as_deref().unwrap_or("<no commit found>"));

            match generator.create_detailed_report_page(&dir.to_string_lossy(), commit_url.as_deref()) {
                Ok(()) => published += 1,
                Err(e) => {
                    eprintln!("[FAIL] {}: {}", name, e);
                    failed += 1;
                }
            }
        }

        println!(
            "\nDone. {} pages created/confirmed, {} skipped (bad name), {} failed.",
            published, skipped, failed
        );
    }

    /// Rebuilds the main dashboard's First-run/Latest-run comparison using the
    /// most recent results/bulk/* run as "latest", tagged with the FFmpeg
    /// commit that was HEAD at its timestamp. "First run" stays the fixed
    /// published/h264_cabac/initial_results_h264_cabac baseline.
    fn publish_dashboard_latest_bulk(&self) {
        let bulk_dir = self.results_path.join("bulk");
        let latest_dir = match self.get_last_dir(&bulk_dir) {
            Some(d) => d,
            None => {
                eprintln!("No runs found under {}", bulk_dir.display());
                return;
            }
        };
        let latest_name = latest_dir.file_name().unwrap_or_default().to_string_lossy().to_string();
        println!("Using latest bulk run: {}", latest_name);

        let commits = self.get_ffmpeg_commit_history();
        if commits.is_empty() {
            eprintln!(
                "No commit history found in {}/FFmpeg-8.0-custom/FFmpeg — aborting.",
                self.repo_path.display()
            );
            return;
        }
        let remote = match self.get_ffmpeg_remote_url() {
            Some(r) => r,
            None => {
                eprintln!("Could not determine FFmpeg repo remote URL — aborting.");
                return;
            }
        };
        let latest_commit = match Self::parse_folder_timestamp(&latest_name)
            .and_then(|ts| Self::commit_url_for_timestamp(&commits, &remote, ts))
        {
            Some(c) => c,
            None => {
                eprintln!("Could not resolve a commit for {} — aborting.", latest_name);
                return;
            }
        };
        println!("Latest run -> {}", latest_commit);

        if !self.first_results_dir.is_dir() {
            eprintln!(
                "[WARN] First-run baseline dir {} does not exist; the dashboard's \
                 'First run' side will show no attachments.",
                self.first_results_dir.display()
            );
        }

        let confluence_url = match env::var("CONFLUENCE_URL") {
            Ok(v) => v,
            Err(_) => { eprintln!("CONFLUENCE_URL not set"); return; }
        };
        let space_key = match env::var("SPACE_KEY") {
            Ok(v) => v,
            Err(_) => { eprintln!("SPACE_KEY not set"); return; }
        };
        let main_page_title = match env::var("MAIN_PAGE_TITLE") {
            Ok(v) => v,
            Err(_) => { eprintln!("MAIN_PAGE_TITLE not set"); return; }
        };
        let username = match env::var("CONFLUENCE_USER") {
            Ok(v) => v,
            Err(_) => { eprintln!("CONFLUENCE_USER not set"); return; }
        };
        let api_token = match env::var("CONFLUENCE_TOKEN") {
            Ok(v) => v,
            Err(_) => { eprintln!("CONFLUENCE_TOKEN not set"); return; }
        };

        let generator = confluence_report_generator::ConfluenceReportGenerator::new(
            &confluence_url,
            &username,
            &api_token,
            &space_key,
            &main_page_title,
            "h264_cabac",
            &self.project_root,
        );

        let first_dir_str = self.first_results_dir.to_string_lossy().to_string();
        let latest_dir_str = latest_dir.to_string_lossy().to_string();

        match generator.update_main_dashboard_summary(
            &[&first_dir_str, &latest_dir_str],
            &[Some(self.first_git_commit.as_str()), Some(latest_commit.as_str())],
            &["First run", "Latest run"],
        ) {
            Ok(()) => println!("Dashboard summary updated."),
            Err(e) => eprintln!("Error updating dashboard: {}", e),
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
    println!("  4: Publish all results/bulk runs individually (dated to matching FFmpeg commit)");
    println!("  5: Update main dashboard comparison using the latest results/bulk run");
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
            "4" => {
                publisher.publish_all_bulk_runs();
            }
            "5" => {
                publisher.publish_dashboard_latest_bulk();
            }
            "0" => {
                publisher.run_all();
                break;
            }
            _ => println!("Invalid step: {}", step),
        }
    }
}