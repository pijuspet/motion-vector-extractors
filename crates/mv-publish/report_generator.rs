use crate::confluence_report_generator::ConfluenceReportGenerator;

use std::path::Path;

pub fn publish_to_confluence(
    first_dir: &str,
    second_dir: &str,
    first_git_commit: &str,
    second_git_commit: &str,
    video_type: &str,
    project_root: &Path,
) -> Result<(), String> {
    let confluence_url = std::env::var("CONFLUENCE_URL")
        .map_err(|_| "CONFLUENCE_URL not set".to_string())?;
    let space_key =
        std::env::var("SPACE_KEY").map_err(|_| "SPACE_KEY not set".to_string())?;
    let main_page_title = std::env::var("MAIN_PAGE_TITLE")
        .map_err(|_| "MAIN_PAGE_TITLE not set".to_string())?;
    let username = std::env::var("CONFLUENCE_USER")
        .map_err(|_| "CONFLUENCE_USER not set".to_string())?;
    let api_token = std::env::var("CONFLUENCE_TOKEN")
        .map_err(|_| "CONFLUENCE_TOKEN not set".to_string())?;

    let generator = ConfluenceReportGenerator::new(
        &confluence_url,
        &username,
        &api_token,
        &space_key,
        &main_page_title,
        video_type,
        project_root,
    );
    println!("[DEBUG] ConfluenceReportGenerator initialized.");

    let old_dir = first_dir.trim_end_matches('/');
    println!("[DEBUG] First results dir: {}", old_dir);
    generator.create_detailed_report_page(old_dir, Some(first_git_commit))?;
    println!("[DEBUG] Finished creating detailed report for first run.");

    let latest_dir = second_dir.trim_end_matches('/');
    println!("[DEBUG] Latest results dir: {}", latest_dir);
    generator.create_detailed_report_page(latest_dir, Some(second_git_commit))?;
    println!("[DEBUG] Finished creating detailed report for latest run.");

    println!("[DEBUG] Updating dashboard summary...");
    generator.update_main_dashboard_summary(
        &[first_dir, second_dir],
        &[Some(first_git_commit), Some(second_git_commit)],
        &["First run", "Latest run"],
    )?;
    println!("Dashboard summary updated.");

    Ok(())
}
