import os

from dotenv import load_dotenv

load_dotenv(".env")

import publishing.confluence_report_generator as conf


def create_report(generator, directory, commit_url, latest=True):
    if not os.path.isdir(directory):
        print(f"[ERROR] results directory does not exist: {directory}")
        return

    if latest:
        print_str = "Latest"
    else:
        print_str = "First"
    print(f"[DEBUG] {print_str} results dir: {directory}")

    generator.create_detailed_report_page(
        directory, git_commit_url=commit_url
    )
    print(f"[DEBUG] Finished creating detailed report for {print_str.lower()} run.")


def publish_to_confluence(first_dir, second_dir, first_git_commit, second_git_commit, project_root):
    confluence_url = os.environ.get("CONFLUENCE_URL")
    space_key = os.environ.get("SPACE_KEY")
    main_page_title = os.environ.get("MAIN_PAGE_TITLE")
    username = os.environ.get("CONFLUENCE_USER")
    api_token = os.environ.get("CONFLUENCE_TOKEN")

    generator = conf.ConfluenceReportGenerator(
        confluence_url, username, api_token, space_key, main_page_title, project_root
    )
    print("[DEBUG] ConfluenceReportGenerator initialized.")

    # Debug and check existence for first run (first argument)
    old_dir = first_dir.rstrip("/")
    create_report(generator, old_dir, first_git_commit, latest=False)

    latest_dir = second_dir.rstrip("/")
    create_report(generator, latest_dir, second_git_commit, latest=True)

    # Always update dashboard summary
    print("[DEBUG] Updating dashboard summary...")
    generator.update_main_dashboard_summary(
        results_dirs=[first_dir, latest_dir],
        git_commits=[first_git_commit, second_git_commit],
        run_titles=["First run", "Latest run"],
    )
    print("Dashboard summary updated.")