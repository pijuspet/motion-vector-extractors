import subprocess
import os
import sys
from pathlib import Path
from typing import Optional
from datetime import datetime

import publishing.publish_to_confluence as ptc
from benchmarking.full_benchmark import BenchmarkRunner


class BenchmarkPublisher:
    def __init__(self):
        self.project_root = Path.cwd()
        self.results_path = self.project_root / "results"
        self.repo_path = self.project_root / "ffmpeg"
        
        self.first_results_dir = self.project_root / "published" / "initial_results"
        self.first_git_commit = "https://github.com/ablouise/ffmpeg-8.0-ourversion/commit/6faaff56c675b77dc783afc89a1dfb113c07bcf9"

        # self.video = self.project_root / "videos" / "bigbunny.mp4"
        self.video = self.project_root / "videos" / "vid_h264.mp4"
        self.streams = 15

    def __get_last_dir__(self, path):
        items = os.listdir(path)
        dirs = []
        for d in items:
            if os.path.isdir(path / d):
                dirs.append(path / d)
        dirs = sorted(dirs)
        return dirs[-1]

    def run_command(self, cmd, env=None, cwd=None, capture_output=False, shell=False, track_failure=True):
        if not shell:
            cmd = cmd.split()
        try:
            result = subprocess.run(
                cmd,
                cwd=cwd,
                env=env,
                capture_output=capture_output,
                text=True,
                check=track_failure,
                shell=shell,
            )
            if capture_output:
                return result.stdout.strip()
            return True
        except subprocess.CalledProcessError as e:
            print(f"Error executing command: {e}")
            return False if not capture_output else None

    def run_benchmark(self) -> str:
        print("DEBUG: Starting benchmark...")
        benchmarker = BenchmarkRunner(self.video, self.streams)
        benchmarker.run_all()

        print("DEBUG: Benchmark script finished.")
        return self.__get_last_dir__(self.results_path)

    def publish_git(self) -> str:
        print(f"Committing and pushing all changes to git in {self.repo_path}...")

        self.run_command(f"git -C {self.repo_path} add .")
        commit_msg = f"Automated benchmark and report update {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}"

        self.run_command(f"git -C {self.repo_path} commit -m \"{commit_msg}\"", shell=True, track_failure=False)
        self.run_command(f"git -C {self.repo_path} push origin")

        commit_hash = self.run_command(
            f"git -C {self.repo_path} rev-parse HEAD", capture_output=True
        )

        remote_url = self.run_command(
            f"git -C {self.repo_path} config --get remote.origin.url",
            capture_output=True,
        )

        if not remote_url:
            return commit_hash

        return f"{remote_url}/commit/{commit_hash}"

    def publish_confluence(
        self,
        first_dir: str,
        latest_dir: str,
        git_commit_run1: str,
        git_commit_run2: str,
    ):
        print("Publishing report to Confluence...")
        print(f"  First results directory: {first_dir}")
        print(f"  Latest results directory: {latest_dir}")
        print(f"  Git commit run 1: {git_commit_run1}")
        print(f"  Git commit run 2: {git_commit_run2}")

        if not all([first_dir, latest_dir, git_commit_run1, git_commit_run2]):
            print(
                "Error: You must provide FIRST and LATEST results directories and both git commit URLs."
            )
            print(
                "Usage: publish_confluence <first_results_dir> <latest_results_dir> <git_commit_run1> <git_commit_run2>"
            )
            return

        if not Path(first_dir).is_dir():
            print(f"Error: First results directory '{first_dir}' does not exist.")
            return

        if not Path(latest_dir).is_dir():
            print(f"Error: Latest results directory '{latest_dir}' does not exist.")
            return

        ptc.publish_to_confluence(
            first_dir, latest_dir, git_commit_run1, git_commit_run2, self.project_root
        )

        published_dir = latest_dir.replace("/results/", "/published/")
        published_path = Path(published_dir)

        if published_path.exists():
            print(f"Published directory already exists: {published_dir}")
        else:
            published_path.parent.mkdir(parents=True, exist_ok=True)
            print("Copying files to a published directory")
            self.run_command(f"cp -r {latest_dir} {published_dir}")
            print(f"Published results copied to: {published_dir}")

    def run_all(self):
        print("Running full benchmark and publishing results...")
        latest_results_dir = self.run_benchmark()
        print(f"Latest results directory: {latest_results_dir}")

        latest_git_commit = self.publish_git()

        self.publish_confluence(
            self.first_results_dir,
            latest_results_dir,
            self.first_git_commit,
            latest_git_commit,
        )

    def show_menu(self) -> list:
        print()
        print(
            "Select publish step to run (enter one or more numbers separated by space):"
        )
        print("  1: Run Full Benchmark")
        print("  2: Commit to Git")
        print("  3: Publish to Confluence")
        print("  0: Run ALL (benchmark, git, confluence)")
        print()

        choices = input("Choice(s): ").strip().split()
        return choices

    def run_interactive(self, choices: Optional[list] = None):
        if choices is None:
            choices = self.show_menu()

        for step in choices:
            if step == "1":
                self.run_benchmark()

            elif step == "2":
                self.publish_git()

            elif step == "3":
                if len(sys.argv) >= 6:
                    self.publish_confluence(
                        sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]
                    )
                else:
                    first_results_dir = input(
                        "Enter the path to the FIRST results directory: "
                    )
                    latest_results_dir = input(
                        "Enter the path to the LATEST results directory: "
                    )
                    git_commit_run1 = input("Enter the Git commit URL for RUN 1: ")
                    git_commit_run2 = input("Enter the Git commit URL for RUN 2: ")

                    self.publish_confluence(
                        first_results_dir,
                        latest_results_dir,
                        git_commit_run1,
                        git_commit_run2,
                    )

            elif step == "0":
                self.run_all()
                break

            else:
                print(f"Invalid step: {step}")


if __name__ == "__main__":
    publisher = BenchmarkPublisher()

    if len(sys.argv) > 1:
        choices = sys.argv[1].split()
        publisher.run_interactive(choices)
    else:
        publisher.run_interactive()
