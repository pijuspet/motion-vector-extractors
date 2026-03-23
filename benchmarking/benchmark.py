import subprocess
import pandas as pd
import os

import benchmarking.slides as sld


def generate_stream_runs(max_streams):
    base = [x for x in [1, 3, 5] if x <= max_streams]
    if max_streams > 5:
        base += list(range(10, max_streams + 1, 5))
    return base


def run_benchmark(
    input_file,
    streams,
    project_absolute_path,
    results_absolute_path,
    exe,
    is_single_threaded,
    is_verbose,
    write_to_csv,
):
    print(f"Running benchmark with {streams} streams...")
    result = subprocess.run(
        [
            exe,
            input_file,
            str(streams),
            results_absolute_path,
            project_absolute_path,
            str(is_single_threaded),
            str(is_verbose),
            str(write_to_csv),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        encoding="utf-8",
    )
    if result.returncode != 0:
        print(f"Error running benchmark: {result.stderr}")
        return pd.DataFrame(), result.stdout
    return parse_output(result.stdout, streams), result.stdout


def parse_output(output_text, stream_count):
    in_table = False
    results = []
    for line in output_text.split("\n"):
        line = line.strip()
        if "Method" in line and "ms/frame(strm)" in line:
            in_table = True
            continue

        if not in_table:
            continue
        # Skip separator lines and blanks
        if (
            not line
            or line.startswith("—")
            or line.startswith("---")
            or line.startswith("===")
        ):
            continue

        if line.startswith("ms/") or line.startswith("CPU") or line.startswith("Total"):
            continue

        parts = [p.strip() for p in line.split("|")]

        # Format: 9 columns
        # [0] Method
        # [1] ms/frame(strm)   e.g. "10.23 ms"
        # [2] Total FPS        e.g. "97.8"
        # [3] CPU ms/frame     e.g. "7.321 ms"
        # [4] CPU %/stream     e.g. "85%"
        # [5] CPU % total      e.g. "87%"
        # [6] Mem Total KB
        # [7] Mem/Strm KB
        # [8] Frames
        if len(parts) < 9:
            continue
        try:
            method = parts[0]
            time_per_frame = float(parts[1].replace("ms", "").strip())
            fps = float(parts[2].strip())
            cpu_ms_per_frame = float(parts[3].replace("ms", "").strip())
            mem_total_kb = float(parts[6].strip())
            mem_per_stream_kb = float(parts[7].strip())
            frames = int(parts[8].strip())

            results.append(
                {
                    "method": method,
                    "streams": stream_count,
                    "time_per_frame": time_per_frame,
                    "fps": fps,
                    "cpu_ms_per_frame": cpu_ms_per_frame,
                    "memory": mem_total_kb,
                    "mem_per_stream": mem_per_stream_kb,
                    "frames": frames,
                }
            )
        except (ValueError, IndexError):
            pass

    return pd.DataFrame(results)


def run_benchmark_averaged(
    input_file,
    streams,
    project_absolute_path,
    results_absolute_path,
    exe,
    is_single_threaded,
    is_verbose,
    write_to_csv,
    n_runs,
):
    numeric_cols = [
        "time_per_frame",
        "fps",
        "cpu_ms_per_frame",
        "memory",
        "mem_per_stream",
        "frames",
    ]

    run_dfs = []
    for run_idx in range(1, n_runs + 1):
        print(f"  Run {run_idx}/{n_runs} for {streams} streams...")
        df, _ = run_benchmark(
            input_file,
            streams,
            project_absolute_path,
            results_absolute_path,
            exe,
            is_single_threaded,
            is_verbose,
            write_to_csv,
        )
        if not df.empty:
            run_dfs.append(df)

    if not run_dfs:
        return pd.DataFrame()

    combined = pd.concat(run_dfs, ignore_index=True)
    averaged = combined.groupby(["method", "streams"], as_index=False)[
        numeric_cols
    ].mean()
    return averaged


def benchmark(
    input,
    streams,
    executable_absolute_path,
    exe,
    project_absolute_path,
    results_absolute_path,
    slides_config,
    plots_folder,
    is_single_threaded,
    is_verbose,
    write_to_csv,
    video_type,
    n_runs=1,
):
    exe_fullpath = os.path.join(executable_absolute_path, exe)

    stream_steps = generate_stream_runs(streams)
    print(f"Stream ranges to test: {stream_steps}")
    print(f"Each stream count will be run {n_runs} time(s) and averaged.")

    all_results = []
    for s in stream_steps:
        df = run_benchmark_averaged(
            input,
            s,
            project_absolute_path,
            results_absolute_path,
            exe_fullpath,
            is_single_threaded,
            is_verbose,
            write_to_csv,
            n_runs,
        )
        if df.empty:
            print(f"Warning: No data returned for streams={s}")
        all_results.append(df)

    # Drop any empty DataFrames before concat to avoid dtype warnings
    non_empty = [df for df in all_results if not df.empty]
    if not non_empty:
        print(
            "Error: all stream runs returned empty results. Check C++ binary and output format."
        )
        return

    full_df = pd.concat(non_empty, ignore_index=True)

    csv_path = os.path.join(plots_folder, "benchmark_results.csv")
    full_df.to_csv(csv_path, index=False)
    print(f"Saved complete data table: {csv_path}")

    sld.produce_slides(
        full_df,
        slides_config,
        "benchmark_comparison_slides.pptx",
        plots_folder,
        video_type,
    )
