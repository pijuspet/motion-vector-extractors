import os
import time
from atlassian import Confluence
import glob
import requests
from bs4 import BeautifulSoup
import re


class ConfluenceReportGenerator:
    def __init__(self, confluence_url, username, api_token, space_key, main_page_title):
        self.confluence = Confluence(
            url=confluence_url, username=username, password=api_token
        )
        self.space_key = space_key
        self.main_page_title = main_page_title

    def get_page_by_title(self):
        return self.confluence.get_page_by_title(self.space_key, self.main_page_title)

    def create_detailed_report_page(
        self, results_dir, report_title, parent_id=None, git_commit_url=None
    ):
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

        self.attach_detailed_report_files(page_id, plots_dir, vtune_dir)

        # Compose the detailed report body referencing attachments
        body = self.generate_detailed_report_body(
            plots_dir, vtune_dir, page_id, git_commit_url=git_commit_url
        )

        self.generate_detailed_report(page_id, report_title, body)

    def generate_detailed_report(
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
        print(
            "[DEBUG] Detailed report body being sent to Confluence:\n"
            + body[:2000]
            + ("..." if len(body) > 2000 else "")
        )
        self.confluence.put(f"/rest/api/content/{page_id}", data=update_data)
        print(f"[INFO] Created detailed report page '{report_title}' (id={page_id})")

    def attach_detailed_report_files(self, page_id, plots_dir, vtune_dir):
        vtune_file_names = ["vtune_hotspots.png", "call_tree.html"]

        for file_name in vtune_file_names:
            file_path = os.path.join(vtune_dir, file_name) if vtune_dir else None
            if file_path and os.path.exists(file_path):
                self.confluence.attach_file(
                    filename=file_path, page_id=page_id, name=file_name
                )

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
            file_path = os.path.join(plots_dir, file_name)
            if os.path.exists(file_path):
                self.confluence.attach_file(
                    filename=file_path, page_id=page_id, name=file_name
                )

        # Detailed Tables per Streams Count
        for img in sorted(
            glob.glob(os.path.join(plots_dir, "detail_table_*streams.png"))
        ):
            self.confluence.attach_file(
                filename=img, page_id=page_id, name=os.path.basename(img)
            )

    def __embed_images__(self, images, directory) -> str:
        body = ""
        for title, fname in images:
            img_path = os.path.join(directory, fname)
            if os.path.exists(img_path):
                body += f"<h3>{title}</h3>"
                body += f'<ac:image ac:thumbnail="true" ac:width="600"><ri:attachment ri:filename="{fname}" /></ac:image>'
        return body

    def __get_mv_cmp_attachment__(self, page_id, file_name):
        try:
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
                    return f'<pre style="background:#f4f4f4; border:1px solid #ccc; padding:10px;">{resp.text.strip()}</pre>'
        except Exception:
            pass
        return "<em>No motion vector comparison result available</em>"

    def __get_calltree_html_interactive__(self, page_id, file_name, add_macro=True):
        try:
            attachments = self.confluence.get_attachments_from_content(
                page_id, filename=file_name
            )
            att = attachments["results"][0] if attachments["size"] > 0 else None
            if att and "download" in att["_links"]:
                url = self.confluence.url + att["_links"]["download"]
                resp = requests.get(
                    url, auth=(self.confluence.username, self.confluence.password)
                )
                if resp.ok:
                    html = resp.text
                    if add_macro == False:
                        return (
                            html.strip()
                            if html.strip()
                            else "<em>No call tree HTML available</em>"
                        )
                    macro = f'<ac:structured-macro ac:name="html"><ac:plain-text-body><![CDATA[{html}]]></ac:plain-text-body></ac:structured-macro>'
                    return (
                        macro
                        if html.strip()
                        else "<em>No call tree HTML available</em>"
                    )
        except Exception:
            pass
        return "<em>No call tree HTML available</em>"

    def __get_calltree_html_non_interactive__(self, page_id, file_name):
        body = ""
        call_tree = self.__get_calltree_html_interactive__(page_id, file_name, add_macro=False)
        if call_tree != "<em>No call tree HTML available</em>":
            try:
                soup = BeautifulSoup(call_tree, "html.parser")
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
                    tree_text = "\n".join(tree_lines[:100])  # Limit to first 100 lines
                    body += f'<pre style="background:#f4f4f4; border:1px solid #ccc; padding:10px; max-height:400px; overflow-y:auto;">{tree_text}</pre>'
                else:
                    body += f'<pre style="background:#f4f4f4; border:1px solid #ccc; padding:10px;">{soup.get_text()[:2000]}</pre>'
            except Exception:
                body += f'<pre style="background:#f4f4f4; border:1px solid #ccc; padding:10px;">{calltree_content[:2000]}...</pre>'
        else:
            body += "<em>No call tree data available</em>"
        return body

    def generate_detailed_report_body(
        self, plots_dir, vtune_dir, page_id=None, git_commit_url=None
    ):
        body = '<div style="text-align: left; width: 100vw; max-width: 100vw; margin: 0; padding: 0;">'

        # 1. Motion Vector Comparison (always first)
        body += "<h3>Motion Vector Comparison (Frames 10-100)</h3>"
        body += self.__get_mv_cmp_attachment__(page_id, "mv_comparison_result.txt")

        print(f"[DEBUG] git_commit_url in detailed report: {git_commit_url}")
        if git_commit_url:
            body += "<h3>Git Commit Url</h3>"
            body += f'<a href="{git_commit_url}" style="font-size:1.1em;color:#1976d2;">{git_commit_url}</a></div>'

        # 2. Profiler Results (VTune Hotspots)
        vtune_images = [("Profiler Results", "vtune_hotspots.png")]
        body += self.__embed_images__(vtune_images, vtune_dir)

        # # 3. VTune Call Tree HTML (interactive and non-interactive, always after profiler)
        body += "<h3>VTune Call Tree (Interactive)</h3>"
        body += self.__get_calltree_html_interactive__(page_id, "call_tree.html")

        body += "<h3>VTune Call Tree (Non-Interactive)</h3>"
        body += self.__get_calltree_html_non_interactive__(page_id, "call_tree.html")

        # region image embedding
        plots_images = [
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
            ("Grouped CPU Usage Comparison (All Streams)", "grouped_barchart_cpu.png"),
            (
                "Grouped Memory Usage Comparison (All Streams)",
                "grouped_barchart_memory.png",
            ),
        ]

        body += self.__embed_images__(plots_images, plots_dir)
        # Detailed Tables per Streams Count
        for img in sorted(
            glob.glob(os.path.join(plots_dir, "detail_table_*streams.png"))
        ):
            streams = os.path.basename(img).split("_")[2].replace("streams.png", "")
            body += f"<h3>Performance Table: Streams={streams}</h3>"
            body += f'<ac:image ac:thumbnail="true" ac:width="600"><ri:attachment ri:filename="{os.path.basename(img)}" /></ac:image>'
        # endregion
        body += "</div>"
        return body

    def update_main_dashboard_summary(
        self,
        first_results_dir,
        latest_results_dir,
        report_title,
        git_commit_run1=None,
        git_commit_run2=None,
    ):
        dashboard_page = self.get_page_by_title()
        if not dashboard_page:
            raise Exception(f"Main dashboard page '{self.main_page_title}' not found.")
        dashboard_id = dashboard_page["id"]
        page_body = f'<div style="text-align: left; width: 100vw; max-width: 100vw; margin: 0; padding: 0;">'
        files_to_attach = []

        def add_if_exists(path, name):
            if path and os.path.isfile(path):
                files_to_attach.append((path, name))

        if first_results_dir:
            plots_first = os.path.join(first_results_dir, "plots")
            vtune_dir_first = os.path.join(first_results_dir, "vtune_results")
            add_if_exists(
                os.path.join(plots_first, "detail_table_5streams_highlighted.png"),
                "detail_table_5streams_highlighted.png",
            )
            add_if_exists(
                os.path.join(plots_first, "grouped_barchart_cpu.png"),
                "grouped_barchart_cpu.png",
            )
            add_if_exists(
                os.path.join(plots_first, "grouped_barchart_memory.png"),
                "grouped_barchart_memory.png",
            )
            add_if_exists(
                os.path.join(vtune_dir_first, "vtune_hotspots.png"),
                "vtune_hotspots.png",
            )
            # Attach mv_comparison_result.txt and call_tree.html from first run with unique names
            mv_cmp_first_path = os.path.join(
                first_results_dir, "mv_comparison_result.txt"
            )
            if os.path.isfile(mv_cmp_first_path):
                files_to_attach.append(
                    (mv_cmp_first_path, "mv_comparison_result_first.txt")
                )
            calltree_first_path = os.path.join(vtune_dir_first, "call_tree.html")
            if os.path.isfile(calltree_first_path):
                files_to_attach.append((calltree_first_path, "call_tree_first.html"))
        if latest_results_dir:
            plots_latest = os.path.join(latest_results_dir, "plots")
            vtune_dir_latest = os.path.join(latest_results_dir, "vtune_results")
            # Attach latest run's files with 'current_' prefix to avoid collision
            detail_latest_path = os.path.join(
                plots_latest, "detail_table_5streams_highlighted.png"
            )
            if os.path.isfile(detail_latest_path):
                files_to_attach.append(
                    (
                        detail_latest_path,
                        "current_detail_table_5streams_highlighted.png",
                    )
                )
            cpu_latest_path = os.path.join(plots_latest, "grouped_barchart_cpu.png")
            if os.path.isfile(cpu_latest_path):
                files_to_attach.append(
                    (cpu_latest_path, "current_grouped_barchart_cpu.png")
                )
            mem_latest_path = os.path.join(plots_latest, "grouped_barchart_memory.png")
            if os.path.isfile(mem_latest_path):
                files_to_attach.append(
                    (mem_latest_path, "current_grouped_barchart_memory.png")
                )
            add_if_exists(
                os.path.join(vtune_dir_latest, "vtune_hotspots.png"),
                "current_vtune_hotspots.png",
            )
            # Attach mv_comparison_result.txt and call_tree.html from latest run with unique names
            mv_cmp_latest_path = os.path.join(
                latest_results_dir, "mv_comparison_result.txt"
            )
            if os.path.isfile(mv_cmp_latest_path):
                files_to_attach.append((mv_cmp_latest_path, "mv_comparison_result.txt"))
            calltree_latest_path = os.path.join(vtune_dir_latest, "call_tree.html")
            if os.path.isfile(calltree_latest_path):
                files_to_attach.append((calltree_latest_path, "call_tree.html"))
            add_if_exists(
                os.path.join(latest_results_dir, "decoded_output.mp4"),
                "decoded_output.mp4",
            )
        for fpath, fname in files_to_attach:
            print(
                f"[DEBUG] Attaching file: {fpath} as {fname} to dashboard {dashboard_id}"
            )
            try:
                self.confluence.attach_file(
                    filename=fpath, page_id=dashboard_id, name=fname
                )
            except Exception as e:
                print(f"[ERROR] Failed to attach {fname} to dashboard: {e}")
        time.sleep(5)

        def get_img_tag(fname):
            return f'<ac:image ac:thumbnail="true" ac:width="450"><ri:attachment ri:filename="{fname}" /></ac:image>'

        page_body += "<h3>Motion Vector Comparison (Frames 10-100)</h3>"
        mv_first = self.__get_mv_cmp_attachment__(
            dashboard_id, "mv_comparison_result_first.txt"
        )
        mv_latest = self.__get_mv_cmp_attachment__(
            dashboard_id, "mv_comparison_result.txt"
        )
        page_body += '<table class="mv-mini-table"><tr><th>First Run</th><th>Latest Run</th></tr>'
        page_body += f"<tr><td>{mv_first}</td><td>{mv_latest}</td></tr></table>"

        # 2. VTune Hotspots
        page_body += "<h3>VTune Hotspots (Top 30)</h3>"
        img_first = get_img_tag("vtune_hotspots.png")
        img_latest = get_img_tag("current_vtune_hotspots.png")
        page_body += '<table class="mv-mini-table"><tr><th>First Run</th><th>Latest Run</th></tr>'
        page_body += f"<tr><td>{img_first}</td><td>{img_latest}</td></tr></table>"

        # 3. GitHub Commit (per run)
        page_body += "<h3>GitHub Commit</h3>"
        page_body += '<table class="mv-mini-table"><tr><th>First Run</th><th>Latest Run</th></tr>'
        page_body += "<tr>"
        if git_commit_run1:
            page_body += f'<td><a href="{git_commit_run1}" target="_blank">{git_commit_run1}</a></td>'
        else:
            page_body += "<td>N/A</td>"
        if git_commit_run2:
            page_body += f'<td><a href="{git_commit_run2}" target="_blank">{git_commit_run2}</a></td>'
        else:
            page_body += "<td>N/A</td>"
        page_body += "</tr></table>"

        # 4. VTune Call Tree (Interactive)
        page_body += "<h3>VTune Call Tree (Interactive)</h3>"
        calltree_first = self.__get_calltree_html_interactive__(
            dashboard_id, "call_tree_first.html"
        )
        calltree_latest = self.__get_calltree_html_interactive__(
            dashboard_id, "call_tree.html"
        )
        page_body += "<h3>VTune Call Tree (Non Interactive)</h3>"
        page_body += '<table class="mv-mini-table"><tr><th>First Run</th><th>Latest Run</th></tr>'
        page_body += (
            f"<tr><td>{calltree_first}</td><td>{calltree_latest}</td></tr></table>"
        )
        calltree_first = self.__get_calltree_html_non_interactive__(
            dashboard_id, "call_tree_first.html"
        )
        calltree_latest = self.__get_calltree_html_non_interactive__(
            dashboard_id, "call_tree.html"
        )
        page_body += '<table class="mv-mini-table"><tr><th>First Run</th><th>Latest Run</th></tr>'
        page_body += (
            f"<tr><td>{calltree_first}</td><td>{calltree_latest}</td></tr></table>"
        )

        # 5. Detailed Table
        page_body += "<h3>Detailed Table</h3>"
        table_first = get_img_tag("detail_table_1streams_highlighted.png")
        table_latest = get_img_tag("current_detail_table_1streams_highlighted.png")
        page_body += '<table class="mv-mini-table"><tr><th>First Run</th><th>Latest Run</th></tr>'
        page_body += f"<tr><td>{table_first}</td><td>{table_latest}</td></tr></table>"

        # 6. Grouped CPU Usage
        page_body += "<h3>Grouped CPU Usage (All Streams)</h3>"
        cpu_first = get_img_tag("grouped_barchart_cpu.png")
        cpu_latest = get_img_tag("current_grouped_barchart_cpu.png")
        page_body += '<table class="mv-mini-table"><tr><th>First Run</th><th>Latest Run</th></tr>'
        page_body += f"<tr><td>{cpu_first}</td><td>{cpu_latest}</td></tr></table>"

        # 7. Grouped Memory Usage
        page_body += "<h3>Grouped Memory Usage (All Streams)</h3>"
        mem_first = get_img_tag("grouped_barchart_memory.png")
        mem_latest = get_img_tag("current_grouped_barchart_memory.png")
        page_body += '<table class="mv-mini-table"><tr><th>First Run</th><th>Latest Run</th></tr>'
        page_body += f"<tr><td>{mem_first}</td><td>{mem_latest}</td></tr></table>"

        # 9. Detailed Reports (link to both runs if available)
        page_body += "<h3>Detailed Reports</h3>"
        page_body += "<ul>"
        if first_results_dir:
            first_dir = os.path.basename(first_results_dir.rstrip("/"))
            m1 = re.search(r"(\d{8})_(\d{6})", first_dir)
            if m1:
                date_str = m1.group(1)
                time_str = m1.group(2)
                first_report_title = f"Automated Report: {date_str[:4]}-{date_str[4:6]}-{date_str[6:]} {time_str[:2]}:{time_str[2:4]}:{time_str[4:]}"
            else:
                first_report_title = f"Automated Report: {first_dir}"
            page_body += f'<li><b>First Run:</b> <ac:link><ri:page ri:content-title="{first_report_title}" /></ac:link></li>'
        page_body += f'<li><b>Latest Run:</b> <ac:link><ri:page ri:content-title="{report_title}" /></ac:link></li>'
        page_body += "</ul>"
        page_body += "</div>"
        try:
            print(
                f"[DEBUG] Updating dashboard page {dashboard_id} with summary body..."
            )
            page_info = self.confluence.get_page_by_id(dashboard_id, expand="version")
            version_number = page_info["version"]["number"] + 1
            update_data = {
                "id": dashboard_id,
                "type": "page",
                "title": self.main_page_title,
                "space": {"key": self.space_key},
                "body": {"storage": {"value": page_body, "representation": "storage"}},
                "version": {"number": version_number},
                "metadata": {
                    "properties": {
                        "content-appearance-draft": {"value": "full-width"},
                        "content-appearance-published": {"value": "full-width"},
                    }
                },
            }
            self.confluence.put(f"/rest/api/content/{dashboard_id}", data=update_data)
            print(f"[DEBUG] Dashboard page update complete.")
        except Exception as e:
            print(f"[ERROR] Failed to update dashboard page: {e}")
