#!/usr/bin/env python3
"""
Automated Confluence Report Publisher
Runs the benchmark, collects output directory, and publishes results to Confluence.
"""
import os
import argparse
import re

from dotenv import load_dotenv

load_dotenv("../.env")

import confluence_report_generator as conf


def create_report(generator, directory, dashboard_id, commit_url, latest=True):
    if latest:
        print_str = "Latest"
    else:
        print_str = "First"
    print(f"[DEBUG] {print_str} results dir: {directory}")
    if not os.path.isdir(directory):
        print(f"[ERROR] {print_str} results directory does not exist: {directory}")
    else:
        m = re.search(r"(\d{8})_(\d{4})", os.path.basename(directory))
        if m:
            date_str = m.group(1)
            time_str = m.group(2)
            report_title = f"Automated Report: {date_str[:4]}-{date_str[4:6]}-{date_str[6:]} {time_str[:2]}:{time_str[2:4]}:00"
        else:
            report_title = f"Automated Report: {os.path.basename(directory)}"
        print(
            f"[DEBUG] Creating detailed report for {print_str.lower()} run: {report_title}"
        )
        generator.create_detailed_report_page(
            directory, report_title, parent_id=dashboard_id, git_commit_url=commit_url
        )
        print(f"[DEBUG] Finished creating detailed report for {print_str.lower()} run.")
        return report_title


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

    # Get dashboard page id to use as parent
    dashboard_page = generator.get_page_by_title()
    print("[DEBUG] Got dashboard page.")
    if not dashboard_page:
        print(f"[ERROR] Main dashboard page '{main_page_title}' not found.")
        raise Exception(f"Main dashboard page '{main_page_title}' not found.")
    dashboard_id = dashboard_page["id"]
    print(f"[DEBUG] Dashboard ID: {dashboard_id}")

    # Debug and check existence for first run (first argument)
    first_dir = args.first_results_dir.rstrip("/")
    create_report(
        generator, first_dir, dashboard_id, args.git_commit_run1, latest=False
    )

    latest_dir = args.latest_results_dir.rstrip("/")
    report_title = create_report(
        generator, latest_dir, dashboard_id, args.git_commit_run2, latest=True
    )

    # Always update dashboard summary
    print("[DEBUG] Updating dashboard summary...")
    generator.update_main_dashboard_summary(
        first_results_dir=args.first_results_dir,
        latest_results_dir=args.latest_results_dir,
        report_title=report_title,
        git_commit_run1=args.git_commit_run1,
        git_commit_run2=args.git_commit_run2,
    )
    print("Dashboard summary updated.")


if __name__ == "__main__":
    cli()
