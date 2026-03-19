#!/usr/bin/env python3

import os
import sys
import subprocess
from datetime import datetime
from pathlib import Path

import benchmarking.benchmark as benchmarking
import utils.mv_compare as mv_compare
import utils.vtune_hotspots_plot as vtune


class BenchmarkRunner:
    def __init__(self, video_file, video_type, build_type, streams=1, n_runs=3):
        self.video_file = video_file
        self.build_type = build_type
        self.streams = streams
        self.n_runs = n_runs
        self.current_dir = Path.cwd()
        self.results_base = self.current_dir / "results"
        self.results_base.mkdir(exist_ok=True)

        self.results_type = self.results_base / video_type
        self.results_type.mkdir(exist_ok=True)
        
        run_timestamp = datetime.now().strftime("%Y%m%d_%H%M")
        self.results_dir = self.results_type / run_timestamp
        self.results_dir.mkdir(exist_ok=True)

        self.ffmpeg_prefix = self.current_dir / "ffmpeg" / "FFmpeg-8.0"
        self.pkg_config_path = self.ffmpeg_prefix / "lib" / "pkgconfig"

        self.benchmarking_dir = self.current_dir / "benchmarking"
        self.benchmarking_dir_executables = self.benchmarking_dir / "executables"
        self.benchmark_exec = self.benchmarking_dir_executables / "benchmark_extractors"

        self.extractor_executables = self.current_dir / "extractors" / "executables"

        self.motion_vectors_comparison_file = (
            self.results_dir / "mv_comparison_result.txt"
        )
        self.slides_config = self.benchmarking_dir / "slides_config.json"
        self.plots_dir = self.results_dir / "plots"

        # Only source setvars if oneAPI is not already initialized
        self.setvars_cmd = (
            '[ -z "$VTUNE_PROFILER_DIR" ] && '
            '. /opt/intel/oneapi/setvars.sh --force 2>/dev/null; true'
        )
        self.vtune_dir = self.results_dir / "vtune_results"
        self.vtune_hotspots_file = self.vtune_dir / "hotspots.csv"
        self.vtune_topdown_file = self.vtune_dir / "topdown.csv"

    def run_command(self, cmd, env=None, cwd=None, capture_output=False, shell=False):
        if not shell:
            cmd = cmd.split()
        try:
            result = subprocess.run(
                cmd,
                cwd=cwd,
                env=env,
                capture_output=capture_output,
                text=True,
                check=True,
                shell=shell,
                executable="/bin/bash" if shell else None,
            )
            if capture_output:
                return result.stdout.strip()
            return True
        except subprocess.CalledProcessError as e:
            print(f"Error executing command: {e}")
            return False if not capture_output else None

    def build(self):
        print("Building all extractors and tools...")

        if self.build_type == "sys":
            if not self.run_command("make build_sys"):
                return
        else:
            if not self.run_command("make build"):
                return
        
        env = os.environ.copy()
        env["PKG_CONFIG_PATH"] = self.pkg_config_path
        
        pkg_config_cmd = (
            "pkg-config --cflags --libs libavformat libavcodec libavutil libswscale"
        )
        pkg_flags = subprocess.check_output(
            pkg_config_cmd, shell=True, text=True, env=env
        ).strip()

        compile_cmd = (
            f"g++ -O2 -o {self.benchmark_exec} benchmark_extractors.cpp {pkg_flags} -lm"
        )

        if not self.run_command(compile_cmd, cwd=self.benchmarking_dir):
            return False

        print("Build complete.")
        return True

    def extract(self):
        if not self.video_file:
            print(
                "Extraction step skipped: set VIDEO_FILE environment variable to input file."
            )
            return

        print("Running 9-method benchmark suite...")

        is_single_threaded = 0
        is_verbose = 1
        write_to_csv = 1
        cmd = f"{self.benchmark_exec} {self.video_file} {self.streams} {self.results_dir} {self.current_dir} {is_single_threaded} {is_verbose} {write_to_csv}"

        if not self.run_command(cmd, cwd=self.benchmarking_dir_executables):
            return

        print("Benchmarks complete.")

    def plot(self):
        if not self.video_file:
            print("Plotting step skipped: set VIDEO_FILE argument.")
            return

        self.plots_dir.mkdir(exist_ok=True)

        print("Running Python benchmark visualization and PPT generation...")

        is_single_threaded = 1
        is_verbose = 0
        write_to_csv = 0
        benchmarking.benchmark(
            self.video_file,
            self.streams,
            str(self.benchmarking_dir_executables),
            str(self.benchmark_exec),
            str(self.current_dir),
            str(self.results_dir),
            str(self.slides_config),
            str(self.plots_dir),
            is_single_threaded,
            is_verbose,
            write_to_csv,
            n_runs=self.n_runs
        )

        print(f"Plotting complete. Plots and PPTX in {self.plots_dir}.")

    def generate_mv_comparison(self):
        method1_csv = self.results_dir / "method1_output_0.csv"
        method4_csv = self.results_dir / "method4_output_0.csv"
        mv_compare.compare(
            method1_csv,
            method4_csv,
            self.motion_vectors_comparison_file,
        )

    def _get_vtune_env(self):
        try:
            result = subprocess.run(
                f". /opt/intel/oneapi/setvars.sh --force 2>/dev/null && env",
                shell=True,
                executable="/bin/bash",
                capture_output=True,
                text=True,
            )
            env = {}
            for line in result.stdout.splitlines():
                if "=" in line:
                    key, _, value = line.partition("=")
                    env[key] = value
            if "VTUNE_PROFILER_DIR" not in env:
                print("Warning: setvars.sh did not set VTUNE_PROFILER_DIR — vtune may not be found.")
            return env if env else None
        except Exception as e:
            print(f"Warning: could not source setvars.sh: {e}")
            return None

    def profiler(self):
        print("Running VTune profiler on extractor4 with motion_vectors_only=1...")

        ffmpeg_lib = self.current_dir / "ffmpeg" / "ffmpeg-8.0-custom" / "lib"

        self.vtune_dir.mkdir(exist_ok=True)

        do_print = 0
        extractor_index = 4
        is_verbose = 1
        is_single_threaded = 1 # was 0

        extractor_exec = self.extractor_executables / f"extractor{extractor_index}"
        output_csv = self.results_dir / f"method{extractor_index}_output_vtune.csv"

        env = self._get_vtune_env() or os.environ.copy()

        existing_ld = env.get("LD_LIBRARY_PATH", "")
        env["LD_LIBRARY_PATH"] = (
            f"{ffmpeg_lib}/libavutil:{ffmpeg_lib}/libavformat:{existing_ld}"
        )

        vtune_collect_cmd = (
            f"vtune -collect hotspots "
            f"-knob sampling-mode=sw "
            f"-result-dir {self.vtune_dir} -- "
            f"{extractor_exec} {self.video_file} {do_print} "
            f"{output_csv} {extractor_index} {is_verbose} {is_single_threaded}"
        )

        if not self.run_command(
            vtune_collect_cmd, env=env, cwd=self.extractor_executables, shell=True
        ):
            return

        vtune_report_hotspots = (
            f"vtune -report hotspots -result-dir {self.vtune_dir} "
            f"-format csv -report-output {self.vtune_hotspots_file}"
        )
        vtune_report_topdown = (
            f"vtune -report top-down -result-dir {self.vtune_dir} "
            f"-format csv -report-output {self.vtune_topdown_file}"
        )

        self.run_command(vtune_report_hotspots, env=env, shell=True)
        self.run_command(vtune_report_topdown, env=env, shell=True)

        vtune.build_tree(str(self.vtune_topdown_file))

        print(f"Profiler run complete. Results in {self.vtune_dir}.")

    def run_all(self):
        if not self.build():
            print("Build failed, aborting.")
            return
        self.extract()
        self.generate_mv_comparison()
        self.plot()
        self.profiler()


def usage():
    print()
    print(f"Usage: {sys.argv[0]} <input_video_or_rtsp_url> [streams]")
    print("  Set the input (video filename or RTSP URL) as the first argument.")
    print("  The number of 'streams' for benchmarking is optional (default = 1).")
    print("  You will then be prompted to pick which step(s) to run.")
    print("    1 = Build")
    print("    2 = Extract (run benchmark)")
    print("    3 = Generate Plots and PowerPoint")
    print("    4 = Generate MV comparison")
    print("    5 = Profiler (VTune on FFmpeg hacked)")
    print("    0 = Run ALL steps")
    print()


if __name__ == "__main__":
    if len(sys.argv) < 2:
        usage()
        sys.exit(1)

    video_file = sys.argv[1]
    streams = int(sys.argv[2]) if len(sys.argv) > 2 else 1

    video_type = sys.argv[3]
    build_type = sys.argv[4]
    n_runs = int(sys.argv[5])

    if streams < 1:
        print("Error: streams argument must be a positive integer")
        sys.exit(1)

    runner = BenchmarkRunner(video_file, video_type, build_type, streams, n_runs)

    print()
    print("Select steps to run (enter one or more numbers separated by space):")
    print("  1: Build")
    print("  2: Extract (run benchmark)")
    print("  3: Generate Plots and PowerPoint")
    print("  4: Generate MV comparison")
    print("  5: Profiler (VTune on FFmpeg hacked)")
    print("  0: Run ALL steps")
    print()

    if len(sys.argv) > 6:
        choices = sys.argv[6:]
    else:
        choices = input("Choice(s): ").strip().split()

    step_map = {
        "1": runner.build,
        "2": runner.extract,
        "3": runner.plot,
        "4": runner.generate_mv_comparison,
        "5": runner.profiler,
        "0": runner.run_all,
    }

    for step in choices:
        if step in step_map:
            step_map[step]()
            if step == "0":
                break
        else:
            print(f"Invalid step: {step}")
