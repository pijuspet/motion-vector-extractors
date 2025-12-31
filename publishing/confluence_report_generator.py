import os
import time
from atlassian import Confluence
import glob
import requests
from bs4 import BeautifulSoup
import re
from jinja2 import Template


class ConfluenceReportGenerator:
    def __init__(self, confluence_url, username, api_token, space_key, main_page_title):
        self.confluence = Confluence(
            url=confluence_url, username=username, password=api_token
        )
        self.space_key = space_key
        self.main_page_title = main_page_title

    def __get_page_by_title__(self):
        return self.confluence.get_page_by_title(self.space_key, self.main_page_title)

    def __embed_images__(self, images, directory):
        image_list = []
        for title, fname in images:
            img_path = os.path.join(directory, fname)
            if os.path.exists(img_path):
                image_list.append({"title": title, "filename": fname})
        return image_list

    def __get_mv_cmp_attachment__(self, page_id, file_name):
        attachments = self.confluence.get_attachments_from_content(
            page_id, filename=file_name
        )
        att = attachments["results"][0] if attachments["size"] > 0 else None
        if att and "download" in att["_links"]:
            url = self.confluence.url + att["_links"]["download"]
            resp = requests.get(
                url, auth=(self.confluence.username, self.confluence.password)
            )
            if resp.ok and resp.text.strip():
                return resp.text.strip()
        return None

    def __get_calltree_html_interactive__(self, page_id, file_name, add_macro=True):
        attachments = self.confluence.get_attachments_from_content(
            page_id, filename=file_name
        )
        att = attachments["results"][0] if attachments["size"] > 0 else None
        if att and "download" in att["_links"]:
            url = self.confluence.url + att["_links"]["download"]
            resp = requests.get(
                url, auth=(self.confluence.username, self.confluence.password)
            )
            if not resp.ok or not resp.text.strip():
                return None

            html = resp.text.strip()
            return {"html": html, "add_macro": add_macro}
        return None

    def __get_calltree_html_non_interactive__(self, page_id, file_name):
        call_tree_data = self.__get_calltree_html_interactive__(
            page_id, file_name, add_macro=False
        )

        if not call_tree_data:
            return None

        try:
            soup = BeautifulSoup(call_tree_data["html"], "html.parser")
            tree_container = soup.find("ul", class_="tree-root")

            if tree_container:

                def extract_tree_text(ul_element, indent=0):
                    result = []
                    for li in ul_element.find_all(
                        "li", class_="tree-node", recursive=False
                    ):
                        name_span = li.find("span", class_="name")
                        cpu_total = li.find("span", class_="cpu-total")
                        cpu_self = li.find("span", class_="cpu-self")
                        if name_span:
                            line = "  " * indent + name_span.get_text(strip=True)
                            if cpu_total:
                                line += f" {cpu_total.get_text(strip=True)}"
                            if cpu_self:
                                line += f" {cpu_self.get_text(strip=True)}"
                            result.append(line)
                        child_ul = li.find("ul", class_="children")
                        if child_ul:
                            result.extend(extract_tree_text(child_ul, indent + 1))
                    return result

                tree_lines = extract_tree_text(tree_container)
                return "\n".join(tree_lines[:100])  # Limit to first 100 lines
            else:
                return soup.get_text()[:2000]
        except Exception:
            return call_tree_data["html"][:2000]

    def __add_files__(self, results_dir, prefix):
        file_list = []

        # Helper function
        def add_if_exists(path, name):
            directory = os.path.join(path, name)
            if path and os.path.isfile(directory):
                file_list.append((directory, prefix + name))

        plots_dir = os.path.join(results_dir, "plots")
        vtune_dir = os.path.join(results_dir, "vtune_results")

        plots_files = [
            "detail_table_1streams_highlighted.png",
            "grouped_barchart_cpu.png",
            "grouped_barchart_memory.png",
        ]

        vtune_files = [
            "vtune_hotspots.png",
            "call_tree.html",
        ]

        for file in plots_files:
            add_if_exists(plots_dir, file)
        for file in vtune_files:
            add_if_exists(vtune_dir, file)

        add_if_exists(
            results_dir,
            "mv_comparison_result.txt",
        )
        return file_list

    def __generate_detailed_report__(
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

    def __attach_detailed_report_files__(self, page_id, plots_dir, vtune_dir):
        file_list = []

        # Helper function
        def add_if_exists(path, name):
            directory = os.path.join(path, name)
            if path and os.path.isfile(directory):
                file_list.append((directory, name))

        vtune_file_names = ["vtune_hotspots.png", "call_tree.html"]

        plots_file_names = [
            "fastest_high_profile_methods.png",
            "scaling_fps.png",
            "scaling_timeperframe.png",
            "scaling_cpu.png",
            "scaling_memory.png",
            "grouped_barchart_fps.png",
            "grouped_barchart_timeperframe.png",
            "grouped_barchart_cpu.png",
            "grouped_barchart_memory.png",
        ]

        for file_name in plots_file_names:
            add_if_exists(plots_dir, file_name)

        for file_name in vtune_file_names:
            add_if_exists(vtune_dir, file_name)

        # Detailed Tables per Streams Count
        for img in glob.glob(os.path.join(plots_dir, "detail_table_*streams.png")):
            file_list.append((img, os.path.basename(img)))

        for fpath, fname in file_list:
            self.confluence.attach_file(filename=fpath, page_id=page_id, name=fname)

    def __generate_detailed_report_body__(
        self, plots_dir, vtune_dir, page_id=None, git_commit_url=None
    ):
        mv_comparison = self.__get_mv_cmp_attachment__(
            page_id, "mv_comparison_result.txt"
        )

        vtune_img = [("Profiler Results", "vtune_hotspots.png")]
        vtune_images = self.__embed_images__(vtune_img, vtune_dir)

        calltree_interactive = self.__get_calltree_html_interactive__(
            page_id, "call_tree.html"
        )
        calltree_non_interactive = self.__get_calltree_html_non_interactive__(
            page_id, "call_tree.html"
        )

        plots_img = [
            ("Fastest Methods", "fastest_high_profile_methods.png"),
            ("Throughput Scaling", "scaling_fps.png"),
            ("Latency Scaling", "scaling_timeperframe.png"),
            ("CPU Usage Scaling", "scaling_cpu.png"),
            ("Memory Usage Scaling", "scaling_memory.png"),
            ("Grouped FPS Comparison (All Streams)", "grouped_barchart_fps.png"),
            (
                "Grouped Latency Comparison (All Streams)",
                "grouped_barchart_timeperframe.png",
            ),
            (
                "Grouped CPU Usage Comparison (All Streams)",
                "grouped_barchart_cpu.png",
            ),
            (
                "Grouped Memory Usage Comparison (All Streams)",
                "grouped_barchart_memory.png",
            ),
        ]
        plots_images = self.__embed_images__(plots_img, plots_dir)

        detail_tables = []
        for img in sorted(
            glob.glob(os.path.join(plots_dir, "detail_table_*streams.png"))
        ):
            streams = os.path.basename(img).split("_")[2].replace("streams.png", "")
            detail_tables.append(
                {"streams": streams, "filename": os.path.basename(img)}
            )

        with open("templates/detailed_report_template.html.jinja", "r") as f:
            template = Template(f.read())

        return template.render(
            mv_comparison=mv_comparison,
            git_commit_url=git_commit_url,
            vtune_images=vtune_images,
            calltree_interactive=calltree_interactive,
            calltree_non_interactive=calltree_non_interactive,
            plots_images=plots_images,
            detail_tables=detail_tables,
        )

    def __get_main_dashboard_body__(
        self,
        dashboard_id,
        results_dirs,
        git_commits=None,
        run_titles=None,
    ):
        with open("templates/main_dashboard_template.html.jinja", "r") as f:
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
                "mv_comparison": self.__get_mv_cmp_attachment__(
                    dashboard_id, f"{prefix}mv_comparison_result.txt"
                ),
                "vtune_hotspots": f"{prefix}vtune_hotspots.png",
                "git_commit": git_commit,
                "calltree_interactive": self.__get_calltree_html_interactive__(
                    dashboard_id, f"{prefix}call_tree.html"
                ),
                "calltree_non_interactive": self.__get_calltree_html_non_interactive__(
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
        # Check if a child page with the same title exists under the dashboard

        # region page deletion
        if parent_id:  # ??  why this deletion
            children = self.confluence.get_child_pages(parent_id)
            for child in children:
                if child["title"] == report_title:
                    print(
                        f"[INFO] Deleting existing detailed report page '{report_title}' under dashboard."
                    )
                    self.confluence.remove_page(child["id"])
                    break
        # Otherwise, create the page as a child of the dashboard if parent_id is given
        # endregion

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
        plots_dir = os.path.join(results_dir, "plots")
        vtune_dir = os.path.join(results_dir, "vtune_results")

        # Attach mv_comparison_result.txt and call_tree.html if present before other files
        mv_cmp_path = os.path.join(results_dir, "mv_comparison_result.txt")
        if os.path.isfile(mv_cmp_path):
            self.confluence.attach_file(
                filename=mv_cmp_path, page_id=page_id, name="mv_comparison_result.txt"
            )

        self.__attach_detailed_report_files__(page_id, plots_dir, vtune_dir)

        # Compose the detailed report body referencing attachments
        body = self.__generate_detailed_report_body__(
            plots_dir, vtune_dir, page_id, git_commit_url=git_commit_url
        )

        print(
            "[DEBUG] Detailed report body being sent to Confluence:\n"
            + body[:2000]
            + ("..." if len(body) > 2000 else "")
        )
        self.__generate_detailed_report__(page_id, report_title, body)
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

        files_to_attach = []

        for idx, results_dir in enumerate(results_dirs):
            files_to_attach.extend(self.__add_files__(results_dir, prefix=f"run{idx}_"))

        for fpath, fname in files_to_attach:
            print(
                f"[DEBUG] Attaching file: {fpath} as {fname} to dashboard {dashboard_id}"
            )
            self.confluence.attach_file(
                filename=fpath, page_id=dashboard_id, name=fname
            )
        time.sleep(5)

        body = self.__get_main_dashboard_body__(
            dashboard_id, results_dirs, git_commits, run_titles
        )

        print(f"[DEBUG] Updating dashboard page {dashboard_id} with summary body...")
        self.__generate_detailed_report__(dashboard_id, self.main_page_title, body)
        print(f"[DEBUG] Dashboard page update complete.")

    def generate_report_title(self, directory):
        m = re.search(r"(\d{8})_(\d{4})", os.path.basename(directory))
        if m:
            date_str = m.group(1)
            time_str = m.group(2)
            return f"Automated Report: {date_str[:4]}-{date_str[4:6]}-{date_str[6:]} {time_str[:2]}:{time_str[2:4]}:00"
        return f"Automated Report: {os.path.basename(directory)}"
