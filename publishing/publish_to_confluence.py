#!/usr/bin/env python3

import os
import argparse

from dotenv import load_dotenv

load_dotenv("../.env")

import confluence_report_generator as conf


def create_report(generator, directory, commit_url, latest=True):
    print("[DEBUG DIRECTORY]", directory)
    if not os.path.isdir(directory):
        print(f"[ERROR] results directory does not exist: {directory}")
        return

    if latest:
        print_str = "Latest"
    else:
        print_str = "First"
    print(f"[DEBUG] {print_str} results dir: {directory}")
    report_title = generator.generate_report_title(directory)

    generator.create_detailed_report_page(
        directory, report_title, git_commit_url=commit_url
    )
    print(f"[DEBUG] Finished creating detailed report for {print_str.lower()} run.")


def cli():
    confluence_url = os.environ.get("CONFLUENCE_URL")
    space_key = os.environ.get("SPACE_KEY")
    main_page_title = os.environ.get("MAIN_PAGE_TITLE")
    username = os.environ.get("CONFLUENCE_USER")
    api_token = os.environ.get("CONFLUENCE_TOKEN")

    print("[DEBUG] Entered cli() function.")

    parser = argparse.ArgumentParser(
        description="Publish benchmark results to Confluence."
    )
    parser.add_argument(
        "first_results_dir", help="Results directory for the first run (left column)"
    )
    parser.add_argument(
        "latest_results_dir", help="Results directory for the latest run (right column)"
    )
    parser.add_argument(
        "git_commit_run1", help="Git commit hash or URL for run 1 (first run)"
    )
    parser.add_argument(
        "git_commit_run2", help="Git commit hash or URL for run 2 (latest run)"
    )
    args = parser.parse_args()

    generator = conf.ConfluenceReportGenerator(
        confluence_url, username, api_token, space_key, main_page_title
    )
    print("[DEBUG] ConfluenceReportGenerator initialized.")

    # Debug and check existence for first run (first argument)
    first_dir = args.first_results_dir.rstrip("/")
    create_report(generator, first_dir, args.git_commit_run1, latest=False)

    latest_dir = args.latest_results_dir.rstrip("/")
    create_report(generator, latest_dir, args.git_commit_run2, latest=True)

    # Always update dashboard summary
    print("[DEBUG] Updating dashboard summary...")
    generator.update_main_dashboard_summary(
        results_dirs=[args.first_results_dir, args.latest_results_dir],
        git_commits=[args.git_commit_run1, args.git_commit_run2],
        run_titles=["First run", "Latest run"],
    )
    print("Dashboard summary updated.")


if __name__ == "__main__":
    cli()
