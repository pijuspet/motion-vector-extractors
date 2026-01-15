import os
import time
from atlassian import Confluence
import glob
import requests
import re
from pathlib import Path
from jinja2 import Template
import uuid


class ConfluenceReportGenerator:
    def __init__(
        self,
        confluence_url: str,
        username: str,
        api_token: str,
        space_key: str,
        main_page_title: str,
        project_root: Path,
    ) -> None:
        self.confluence = Confluence(
            url=confluence_url, username=username, password=api_token
        )

        self.call_tree_line_limit = 100
        self.html_preview_limit = 2000
        self.attachment_wait_time = 5

        self.space_key = space_key
        self.main_page_title = main_page_title
        self.project_root = project_root

        self.templates = self.project_root / "publishing" / "templates"

        self.detailed_report_template = (
            self.templates / "detailed_report_template.html.jinja"
        )
        self.main_dashboard_template = (
            self.templates / "main_dashboard_template.html.jinja"
        )
        self.plots_subdir = "plots"
        self.vtune_subdir = "vtune_results"

        self.detailed_report_plots = [
            ("Fastest Methods", "fastest_methods.png", self.plots_subdir),
            ("Throughput Scaling", "scaling_fps.png", self.plots_subdir),
            ("Latency Scaling", "scaling_timeperframe.png", self.plots_subdir),
            ("CPU Usage Scaling", "scaling_cpu.png", self.plots_subdir),
            ("Memory Usage Scaling", "scaling_memory.png", self.plots_subdir),
            (
                "Grouped FPS Comparison (All Streams)",
                "grouped_barchart_fps.png",
                self.plots_subdir,
            ),
            (
                "Grouped Latency Comparison (All Streams)",
                "grouped_barchart_timeperframe.png",
                self.plots_subdir,
            ),
            (
                "Grouped CPU Usage Comparison (All Streams)",
                "grouped_barchart_cpu.png",
                self.plots_subdir,
            ),
            (
                "Grouped Memory Usage Comparison (All Streams)",
                "grouped_barchart_memory.png",
                self.plots_subdir,
            ),
        ]

        self.detailed_report_vtune = [
            ("Profiler Results", "vtune_hotspots.png", self.vtune_subdir),
        ]

        self.main_dashboard_plots = [
            (None, "detail_table_1streams_highlighted.png", self.plots_subdir),
            (None, "grouped_barchart_cpu.png", self.plots_subdir),
            (None, "grouped_barchart_memory.png", self.plots_subdir),
        ]

        self.additional_files = [
            (None, "mv_comparison_result.txt", ""),
        ]

        self.vtune_files = [
            (None, "vtune_hotspots.png", self.vtune_subdir),
            (None, "call_tree.html", self.vtune_subdir),
        ]

        self.glob_patterns = [(None, "detail_table_*streams.png", self.plots_subdir)]

    def __get_page_by_title__(self):
        return self.confluence.get_page_by_title(self.space_key, self.main_page_title)

    def __collect_files__(self, results_dir, file_specs, prefix=""):
        file_list = []

        for title, filename, subdir in file_specs:
            full_dir = os.path.join(results_dir, subdir) if subdir else results_dir
            filepath = os.path.join(full_dir, filename)

            if os.path.isfile(filepath):
                attachment_name = prefix + filename
                file_list.append((filepath, attachment_name, title))

        return file_list

    def __collect_glob_files__(self, results_dir, glob_specs):
        file_list = []

        for title_prefix, pattern, subdir in glob_specs:
            search_dir = os.path.join(results_dir, subdir) if subdir else results_dir

            for filepath in glob.glob(os.path.join(search_dir, pattern)):
                filename = os.path.basename(filepath)
                title = None
                if title_prefix:
                    title = f"{title_prefix}: {filename}"

                file_list.append((filepath, filename, title))

        return file_list

    def __embed_images__(self, images):
        image_list = []
        for title, fname, _ in images:
            image_list.append({"title": title, "filename": fname})
        return image_list

    def __get_attachment_content__(self, page_id, file_name):
        attachments = self.confluence.get_attachments_from_content(
            page_id, filename=file_name
        )
        if attachments["size"] == 0:
            return None

        att = attachments["results"][0]
        if "download" not in att["_links"]:
            return None

        url = self.confluence.url + att["_links"]["download"]
        resp = requests.get(
            url, auth=(self.confluence.username, self.confluence.password)
        )

        if resp.ok and resp.text.strip():
            return resp.text.strip()

        return None

    def __update_page__(
        self,
        page_id,
        report_title,
        body,
    ):
        page_info = self.confluence.get_page_by_id(page_id, expand="version")
        version_number = page_info["version"]["number"] + 1
        update_data = {
            "id": page_id,
            "type": "page",
            "title": report_title,
            "space": {"key": self.space_key},
            "body": {"storage": {"value": body, "representation": "storage"}},
            "version": {"number": version_number},
            "metadata": {
                "properties": {
                    "content-appearance-draft": {"value": "full-width"},
                    "content-appearance-published": {"value": "full-width"},
                }
            },
        }

        self.confluence.put(f"/rest/api/content/{page_id}", data=update_data)

    def __generate_detailed_report_body__(
        self, plots_dir, page_id=None, git_commit_url=None
    ):
        mv_comparison = self.__get_attachment_content__(
            page_id, "mv_comparison_result.txt"
        )

        vtune_images = self.__embed_images__(self.detailed_report_vtune)

        calltree = self.__get_attachment_content__(page_id, "call_tree.html")

        plots_images = self.__embed_images__(self.detailed_report_plots)

        detail_tables = []
        for img in sorted(
            glob.glob(os.path.join(plots_dir, "detail_table_*streams.png")),
            key=lambda x: int(re.search(r"detail_table_(\d+)streams", x).group(1)),
        ):
            streams = os.path.basename(img).split("_")[2].replace("streams.png", "")
            detail_tables.append(
                {"streams": streams, "filename": os.path.basename(img)}
            )

        with open(self.detailed_report_template, "r") as f:
            template = Template(f.read())

        return template.render(
            mv_comparison=mv_comparison,
            git_commit_url=git_commit_url,
            vtune_images=vtune_images,
            calltree=calltree,
            plots_images=plots_images,
            detail_tables=detail_tables,
            macro_id=str(uuid.uuid4()),
        )

    def __get_main_dashboard_body__(
        self,
        dashboard_id,
        results_dirs,
        git_commits=None,
        run_titles=None,
    ):
        with open(self.main_dashboard_template, "r") as f:
            template = Template(f.read())

        if git_commits is None:
            git_commits = [None] * len(results_dirs)

        if run_titles is None:
            run_titles = [f"Run {i+1}" for i in range(len(results_dirs))]

        runs = []

        for idx, (results_dir, git_commit, title) in enumerate(
            zip(results_dirs, git_commits, run_titles)
        ):
            prefix = f"run{idx}_"

            run_data = {
                "title": title,
                "mv_comparison": self.__get_attachment_content__(
                    dashboard_id, f"{prefix}mv_comparison_result.txt"
                ),
                "vtune_hotspots": f"{prefix}vtune_hotspots.png",
                "git_commit": git_commit,
                "calltree": self.__get_attachment_content__(
                    dashboard_id, f"{prefix}call_tree.html"
                ),
                "detail_table": f"{prefix}detail_table_1streams_highlighted.png",
                "cpu_chart": f"{prefix}grouped_barchart_cpu.png",
                "memory_chart": f"{prefix}grouped_barchart_memory.png",
                "report_title": (
                    self.generate_report_title(
                        os.path.basename(results_dir.rstrip("/"))
                    )
                    if results_dir
                    else None
                ),
                "macro_id": str(uuid.uuid4()),
            }

            runs.append(run_data)

        context = {"runs": runs}

        return template.render(context)

    def create_detailed_report_page(
        self, results_dir, report_title, git_commit_url=None
    ):
        dashboard_page = self.__get_page_by_title__()
        print("[DEBUG] Got dashboard page.")
        if not dashboard_page:
            print(f"[ERROR] Main dashboard page '{self.main_page_title}' not found.")
            raise Exception(f"Main dashboard page '{self.main_page_title}' not found.")
        parent_id = dashboard_page["id"]

        print(f"[DEBUG] git_commit_url in detailed report: {git_commit_url}")

        if parent_id:
            children = self.confluence.get_child_pages(parent_id)
            report_exists = any(child["title"] == report_title for child in children)

            if report_exists:
                print(f"[INFO] Report '{report_title}' already exists.")
                return

        # region create page
        create_kwargs = dict(
            space=self.space_key,
            title=report_title,
            body="<p>Uploading attachments...</p>",
            representation="storage",
        )
        if parent_id:
            create_kwargs["parent_id"] = parent_id
        new_page = self.confluence.create_page(**create_kwargs)
        # endregion

        page_id = new_page["id"]
        plots_dir = os.path.join(results_dir, self.plots_subdir)

        all_files = []
        all_files.extend(
            self.__collect_files__(results_dir, self.detailed_report_plots)
        )
        all_files.extend(
            self.__collect_files__(results_dir, self.detailed_report_vtune)
        )
        all_files.extend(self.__collect_files__(results_dir, self.additional_files))
        all_files.extend(self.__collect_files__(results_dir, self.vtune_files))
        all_files.extend(self.__collect_glob_files__(results_dir, self.glob_patterns))

        for filepath, attachment_name, _ in all_files:
            self.confluence.attach_file(
                filename=filepath, page_id=page_id, name=attachment_name
            )

        body = self.__generate_detailed_report_body__(
            plots_dir, page_id, git_commit_url=git_commit_url
        )

        print("[DEBUG] Detailed report body being sent to Confluence")
        self.__update_page__(page_id, report_title, body)
        print(f"[INFO] Created detailed report page '{report_title}' (id={page_id})")

    def update_main_dashboard_summary(
        self,
        results_dirs,
        git_commits=None,
        run_titles=None,
    ):
        dashboard_page = self.__get_page_by_title__()
        if not dashboard_page:
            raise Exception(f"Main dashboard page '{self.main_page_title}' not found.")
        dashboard_id = dashboard_page["id"]

        all_files = []

        for idx, results_dir in enumerate(results_dirs):
            prefix = f"run{idx}_"
            all_files.extend(
                self.__collect_files__(results_dir, self.main_dashboard_plots, prefix)
            )
            all_files.extend(
                self.__collect_files__(results_dir, self.detailed_report_vtune, prefix)
            )
            all_files.extend(
                self.__collect_files__(results_dir, self.additional_files, prefix)
            )
            all_files.extend(
                self.__collect_files__(results_dir, self.vtune_files, prefix)
            )

        for fpath, fname, _ in all_files:
            print(
                f"[DEBUG] Attaching file: {fpath} as {fname} to dashboard {dashboard_id}"
            )
            self.confluence.attach_file(
                filename=fpath, page_id=dashboard_id, name=fname
            )
        time.sleep(self.attachment_wait_time)

        body = self.__get_main_dashboard_body__(
            dashboard_id, results_dirs, git_commits, run_titles
        )

        print(f"[DEBUG] Updating dashboard page {dashboard_id} with summary body...")
        self.__update_page__(dashboard_id, self.main_page_title, body)
        print(f"[DEBUG] Dashboard page update complete.")

    def generate_report_title(self, directory):
        name = os.path.basename(directory.rstrip(os.sep))
        match = re.search(r"(\d{8})_(\d{4})", name)
        if match:
            d, t = match.groups()
            return f"Automated Report: {d[:4]}-{d[4:6]}-{d[6:]} {t[:2]}:{t[2:4]}:00"
        return f"Automated Report: {name}"
