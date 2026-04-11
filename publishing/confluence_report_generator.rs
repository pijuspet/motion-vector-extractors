use crate::confluence::ConfluenceClient;

use minijinja::{context, Environment};
use regex::Regex;
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct FileSpec {
    title: Option<&'static str>,
    filename: &'static str,
    subdir: &'static str,
}

struct GlobSpec {
    _title_prefix: Option<&'static str>,
    pattern: &'static str,
    subdir: &'static str,
}

const DETAILED_REPORT_PLOTS: &[FileSpec] = &[
    FileSpec { title: Some("Fastest Methods"), filename: "fastest_methods.png", subdir: "plots" },
    FileSpec { title: Some("Throughput Scaling"), filename: "scaling_fps.png", subdir: "plots" },
    FileSpec { title: Some("Latency Scaling"), filename: "scaling_timeperframe.png", subdir: "plots" },
    FileSpec { title: Some("CPU Usage Scaling"), filename: "scaling_cpu.png", subdir: "plots" },
    FileSpec { title: Some("Memory Usage Scaling"), filename: "scaling_memory.png", subdir: "plots" },
    FileSpec { title: Some("Speedup Heatmap"), filename: "speedup_heatmap.png", subdir: "plots" },
    FileSpec { title: Some("Speedup CPU"), filename: "speedup_cpu.png", subdir: "plots" },
    FileSpec { title: Some("Speedup Throughput"), filename: "speedup_fps.png", subdir: "plots" },
    FileSpec { title: Some("Speedup Memory"), filename: "speedup_memory.png", subdir: "plots" },
    FileSpec { title: Some("Grouped FPS Comparison (All Streams)"), filename: "grouped_barchart_fps.png", subdir: "plots" },
    FileSpec { title: Some("Grouped Latency Comparison (All Streams)"), filename: "grouped_barchart_timeperframe.png", subdir: "plots" },
    FileSpec { title: Some("Grouped CPU Usage Comparison (All Streams)"), filename: "grouped_barchart_cpu.png", subdir: "plots" },
    FileSpec { title: Some("Grouped Memory Usage Comparison (All Streams)"), filename: "grouped_barchart_memory.png", subdir: "plots" },
];

const DETAILED_REPORT_VTUNE: &[FileSpec] = &[
    FileSpec { title: Some("Profiler Results"), filename: "vtune_hotspots.png", subdir: "vtune_results" },
];

const ADDITIONAL_FILES: &[FileSpec] = &[
    FileSpec { title: None, filename: "mv_comparison_result.txt", subdir: "" },
];

const VTUNE_FILES: &[FileSpec] = &[
    FileSpec { title: None, filename: "vtune_hotspots.png", subdir: "vtune_results" },
    FileSpec { title: None, filename: "call_tree.html", subdir: "vtune_results" },
];

const MAIN_DASHBOARD_PLOTS: &[FileSpec] = &[
    FileSpec { title: None, filename: "detail_table_1streams_highlighted.png", subdir: "plots" },
    FileSpec { title: None, filename: "speedup_heatmap.png", subdir: "plots" },
    FileSpec { title: None, filename: "speedup_cpu.png", subdir: "plots" },
    FileSpec { title: None, filename: "speedup_memory.png", subdir: "plots" },
    FileSpec { title: None, filename: "grouped_barchart_cpu.png", subdir: "plots" },
    FileSpec { title: None, filename: "grouped_barchart_memory.png", subdir: "plots" },
];

const GLOB_PATTERNS: &[GlobSpec] = &[
    GlobSpec { _title_prefix: None, pattern: "detail_table_*streams.png", subdir: "plots" },
];

pub struct ConfluenceReportGenerator {
    client: ConfluenceClient,
    space_key: String,
    main_page_title: String,
    video_type: String,
    templates_dir: PathBuf,
    plots_subdir: &'static str,
    attachment_wait_time: u64,
}

impl ConfluenceReportGenerator {
    pub fn new(
        confluence_url: &str,
        username: &str,
        api_token: &str,
        space_key: &str,
        main_page_title: &str,
        video_type: &str,
        project_root: &Path,
    ) -> Self {
        let client = ConfluenceClient::new(confluence_url, username, api_token);
        let templates_dir = project_root.join("publishing").join("templates");

        ConfluenceReportGenerator {
            client,
            space_key: space_key.to_string(),
            main_page_title: main_page_title.to_string(),
            video_type: video_type.replace('_', " "),
            templates_dir,
            plots_subdir: "plots",
            attachment_wait_time: 5,
        }
    }

    fn generate_report_title(&self, directory: &str) -> String {
        let name = Path::new(directory)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let re = Regex::new(r"(\d{8})_(\d{4})").unwrap();
        if let Some(caps) = re.captures(&name) {
            let d = &caps[1];
            let t = &caps[2];
            return format!(
                "Automated Report: {}-{}-{} {}:{}:00",
                &d[..4],
                &d[4..6],
                &d[6..],
                &t[..2],
                &t[2..4]
            );
        }
        format!("Automated Report: {}", name)
    }

    fn collect_files(
        &self,
        results_dir: &str,
        file_specs: &[FileSpec],
        prefix: &str,
    ) -> Vec<(String, String, Option<String>)> {
        let mut files = Vec::new();
        for spec in file_specs {
            let full_dir = if spec.subdir.is_empty() {
                results_dir.to_string()
            } else {
                format!("{}/{}", results_dir, spec.subdir)
            };
            let filepath = format!("{}/{}", full_dir, spec.filename);
            if Path::new(&filepath).is_file() {
                let attachment_name = format!("{}{}", prefix, spec.filename);
                files.push((
                    filepath,
                    attachment_name,
                    spec.title.map(String::from),
                ));
            }
        }
        files
    }

    fn collect_glob_files(
        &self,
        results_dir: &str,
        glob_specs: &[GlobSpec],
    ) -> Vec<(String, String, Option<String>)> {
        let mut files = Vec::new();
        for spec in glob_specs {
            let search_dir = if spec.subdir.is_empty() {
                results_dir.to_string()
            } else {
                format!("{}/{}", results_dir, spec.subdir)
            };
            let pattern = format!("{}/{}", search_dir, spec.pattern);
            if let Ok(paths) = glob::glob(&pattern) {
                for entry in paths.flatten() {
                    let filename = entry
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let filepath = entry.to_string_lossy().to_string();
                    files.push((filepath, filename, None));
                }
            }
        }
        files
    }

    fn get_or_create_video_type_page(&self) -> Result<String, String> {
        let dashboard_page = self
            .client
            .get_page_by_title(&self.space_key, &self.main_page_title)
            .ok_or(format!(
                "Main dashboard page '{}' not found.",
                self.main_page_title
            ))?;

        let dashboard_id = dashboard_page["id"]
            .as_str()
            .ok_or("Dashboard page has no id")?;

        println!("[DEBUG] Got dashboard page.");

        let children = self.client.get_child_pages(dashboard_id);
        let video_type_page = children
            .iter()
            .find(|c| c["title"].as_str() == Some(&self.video_type));

        if let Some(page) = video_type_page {
            return page["id"]
                .as_str()
                .map(String::from)
                .ok_or("Video type page has no id".to_string());
        }

        let new_page = self
            .client
            .create_page(
                &self.space_key,
                &self.video_type,
                "<p>Uploading attachments...</p>",
                dashboard_id,
            )
            .ok_or("Failed to create video type page")?;

        new_page["id"]
            .as_str()
            .map(String::from)
            .ok_or("New page has no id".to_string())
    }

    fn generate_detailed_report_body(
        &self,
        plots_dir: &str,
        page_id: &str,
        git_commit_url: Option<&str>,
    ) -> Result<String, String> {
        let mv_comparison = self
            .client
            .get_attachment_content(page_id, "mv_comparison_result.txt");

        let vtune_images: Vec<serde_json::Value> = DETAILED_REPORT_VTUNE
            .iter()
            .map(|spec| {
                serde_json::json!({
                    "title": spec.title.unwrap_or(""),
                    "filename": spec.filename,
                })
            })
            .collect();

        let calltree = self
            .client
            .get_attachment_content(page_id, "call_tree.html");

        let plots_images: Vec<serde_json::Value> = DETAILED_REPORT_PLOTS
            .iter()
            .map(|spec| {
                serde_json::json!({
                    "title": spec.title.unwrap_or(""),
                    "filename": spec.filename,
                })
            })
            .collect();

        // Detail tables
        let re = Regex::new(r"detail_table_(\d+)streams\.png").unwrap();
        let pattern = format!("{}/detail_table_*streams.png", plots_dir);
        let mut detail_tables: Vec<(i32, serde_json::Value)> = Vec::new();

        if let Ok(paths) = glob::glob(&pattern) {
            for entry in paths.flatten() {
                let filename = entry
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Some(caps) = re.captures(&filename) {
                    let streams: i32 = caps[1].parse().unwrap_or(0);
                    detail_tables.push((
                        streams,
                        serde_json::json!({
                            "streams": streams.to_string(),
                            "filename": filename,
                        }),
                    ));
                }
            }
        }
        detail_tables.sort_by_key(|(s, _)| *s);
        let detail_tables: Vec<serde_json::Value> =
            detail_tables.into_iter().map(|(_, v)| v).collect();

        let template_path = self
            .templates_dir
            .join("detailed_report_template.html.jinja");
        let template_source = std::fs::read_to_string(&template_path)
            .map_err(|e| format!("Failed to read template: {}", e))?;

        let mut env = Environment::new();
        env.add_template("report", &template_source)
            .map_err(|e| format!("Template parse error: {}", e))?;

        let tmpl = env
            .get_template("report")
            .map_err(|e| format!("Template error: {}", e))?;

        let rendered = tmpl
            .render(context! {
                mv_comparison => mv_comparison,
                git_commit_url => git_commit_url,
                vtune_images => vtune_images,
                calltree => calltree,
                plots_images => plots_images,
                detail_tables => detail_tables,
                macro_id => Uuid::new_v4().to_string(),
            })
            .map_err(|e| format!("Template render error: {}", e))?;

        Ok(rendered)
    }

    fn generate_main_dashboard_body(
        &self,
        dashboard_id: &str,
        results_dirs: &[&str],
        git_commits: &[Option<&str>],
        run_titles: &[&str],
    ) -> Result<String, String> {
        let template_path = self
            .templates_dir
            .join("main_dashboard_template.html.jinja");
        let template_source = std::fs::read_to_string(&template_path)
            .map_err(|e| format!("Failed to read template: {}", e))?;

        let mut env = Environment::new();
        env.add_template("dashboard", &template_source)
            .map_err(|e| format!("Template parse error: {}", e))?;

        let tmpl = env
            .get_template("dashboard")
            .map_err(|e| format!("Template error: {}", e))?;

        let mut runs = Vec::new();
        for (idx, ((results_dir, git_commit), title)) in results_dirs
            .iter()
            .zip(git_commits.iter())
            .zip(run_titles.iter())
            .enumerate()
        {
            let prefix = format!("run{}_", idx);

            let mv_comparison = self.client.get_attachment_content(
                dashboard_id,
                &format!("{}mv_comparison_result.txt", prefix),
            );

            let calltree = self.client.get_attachment_content(
                dashboard_id,
                &format!("{}call_tree.html", prefix),
            );

            let report_title = if !results_dir.is_empty() {
                let basename = Path::new(results_dir.trim_end_matches('/'))
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                Some(self.generate_report_title(&basename))
            } else {
                None
            };

            runs.push(serde_json::json!({
                "title": title,
                "mv_comparison": mv_comparison,
                "vtune_hotspots": format!("{}vtune_hotspots.png", prefix),
                "git_commit": git_commit,
                "calltree": calltree,
                "detail_table": format!("{}detail_table_1streams_highlighted.png", prefix),
                "cpu_chart": format!("{}grouped_barchart_cpu.png", prefix),
                "speedup_cpu": format!("{}speedup_cpu.png", prefix),
                "speedup_memory": format!("{}speedup_memory.png", prefix),
                "speedup_heatmap": format!("{}speedup_heatmap.png", prefix),
                "memory_chart": format!("{}grouped_barchart_memory.png", prefix),
                "report_title": report_title,
                "macro_id": Uuid::new_v4().to_string(),
            }));
        }

        let rendered = tmpl
            .render(context! { runs => runs })
            .map_err(|e| format!("Template render error: {}", e))?;

        Ok(rendered)
    }

    pub fn create_detailed_report_page(
        &self,
        results_dir: &str,
        git_commit_url: Option<&str>,
    ) -> Result<(), String> {
        let report_title = self.generate_report_title(results_dir);
        println!("[DEBUG] git_commit_url in detailed report: {:?}", git_commit_url);

        let video_type_id = self.get_or_create_video_type_page()?;

        let children = self.client.get_child_pages(&video_type_id);
        if children
            .iter()
            .any(|c| c["title"].as_str() == Some(&report_title))
        {
            println!("[INFO] Report '{}' already exists. Skipping.", report_title);
            return Ok(());
        }

        let new_report = self
            .client
            .create_page(
                &self.space_key,
                &report_title,
                "<p>Uploading attachments...</p>",
                &video_type_id,
            )
            .ok_or("Failed to create report page")?;

        let page_id = new_report["id"]
            .as_str()
            .ok_or("New report page has no id")?;

        let plots_dir = format!("{}/{}", results_dir, self.plots_subdir);

        // Collect all files to attach
        let mut all_files = Vec::new();
        all_files.extend(self.collect_files(results_dir, DETAILED_REPORT_PLOTS, ""));
        all_files.extend(self.collect_files(results_dir, DETAILED_REPORT_VTUNE, ""));
        all_files.extend(self.collect_files(results_dir, ADDITIONAL_FILES, ""));
        all_files.extend(self.collect_files(results_dir, VTUNE_FILES, ""));
        all_files.extend(self.collect_glob_files(results_dir, GLOB_PATTERNS));

        for (filepath, attachment_name, _) in &all_files {
            if let Err(e) = self.client.attach_file(page_id, filepath, attachment_name) {
                eprintln!("[WARN] Failed to attach {}: {}", attachment_name, e);
            }
        }

        let body = self.generate_detailed_report_body(
            &plots_dir,
            page_id,
            git_commit_url,
        )?;

        println!("[DEBUG] Detailed report body being sent to Confluence");
        self.client
            .update_page(&self.space_key, page_id, &report_title, &body)?;
        println!(
            "[INFO] Created detailed report page '{}' (id={})",
            report_title, page_id
        );

        Ok(())
    }

    pub fn update_main_dashboard_summary(
        &self,
        results_dirs: &[&str],
        git_commits: &[Option<&str>],
        run_titles: &[&str],
    ) -> Result<(), String> {
        let video_type_id = self.get_or_create_video_type_page()?;

        let mut all_files = Vec::new();
        for (idx, results_dir) in results_dirs.iter().enumerate() {
            let prefix = format!("run{}_", idx);
            all_files.extend(self.collect_files(results_dir, MAIN_DASHBOARD_PLOTS, &prefix));
            all_files.extend(self.collect_files(results_dir, DETAILED_REPORT_VTUNE, &prefix));
            all_files.extend(self.collect_files(results_dir, ADDITIONAL_FILES, &prefix));
            all_files.extend(self.collect_files(results_dir, VTUNE_FILES, &prefix));
        }

        for (fpath, fname, _) in &all_files {
            println!(
                "[DEBUG] Attaching file: {} as {} to dashboard {}",
                fpath, fname, video_type_id
            );
            if let Err(e) = self.client.attach_file(&video_type_id, fpath, fname) {
                eprintln!("[WARN] Failed to attach {}: {}", fname, e);
            }
        }

        self.client.wait(self.attachment_wait_time);

        let body = self.generate_main_dashboard_body(
            &video_type_id,
            results_dirs,
            git_commits,
            run_titles,
        )?;

        println!(
            "[DEBUG] Updating dashboard page {} with summary body...",
            video_type_id
        );
        self.client
            .update_page(&self.space_key, &video_type_id, &self.video_type, &body)?;
        println!("[DEBUG] Dashboard page update complete.");

        Ok(())
    }
}