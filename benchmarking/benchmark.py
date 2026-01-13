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
):
    print(f"Running benchmark with {streams} streams...")
    result = subprocess.run(
        [
            exe,
            input_file,
            str(streams),
            results_absolute_path,
            project_absolute_path,
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
        if "Method" in line and "Time/Frame" in line:
            in_table = True
            continue
        if in_table:
            if line.startswith("—") or line == "" or line.startswith("---"):
                continue
            parts = [x.strip() for x in line.split("|")]
            if len(parts) < 8:
                continue
            try:
                method = parts[0]
                time_per_frame = float(parts[1].replace("ms", "").strip())
                fps = float(parts[2])
                cpu = float(parts[3].replace("%", ""))
                mem = float(parts[4])
                mvs = int(parts[5])
                frames = int(parts[6])
                high_profile = parts[7]
                results.append(
                    {
                        "method": method,
                        "streams": stream_count,
                        "time_per_frame": time_per_frame,
                        "fps": fps,
                        "cpu": cpu,
                        "memory": mem,
                        "mvs": mvs,
                        "frames": frames,
                        "high_profile": high_profile,
                    }
                )
            except Exception:
                pass
    return pd.DataFrame(results)


def run_all(
    input_path,
    max_streams,
    exe,
    project_absolute_path,
    results_absolute_path,
    slides_config,
    plots_folder,
):
    stream_steps = generate_stream_runs(max_streams)
    print(f"Stream ranges to test: {stream_steps}")

    all_results = []
    for s in stream_steps:
        df, _ = run_benchmark(
            input_path,
            s,
            project_absolute_path,
            results_absolute_path,
            exe=exe,
        )
        if df.empty:
            print(f"Warning: No data returned for streams={s}")
        all_results.append(df)

    full_df = pd.concat(all_results, ignore_index=True)

    exclude_methods = ["LIVE555 Parser", "Custom H.264 Parser"]
    full_df = full_df[~full_df["method"].isin(exclude_methods)].copy()

    csv_path = os.path.join(plots_folder, "benchmark_results.csv")
    full_df.to_csv(csv_path, index=False)
    print(f"Saved complete data table: {csv_path}")

    df_hp = full_df[full_df["high_profile"] == "1"].copy()
    if df_hp.empty:
        print("No high profile algorithms found in results!")
        return full_df

    sld.produce_slides(
        df_hp,
        slides_config,
        "benchmark_comparison_slides_high_profile.pptx",
        plots_folder,
    )


def benchmark(
    input,
    streams,
    executable_absolute_path,
    project_absolute_path,
    results_absolute_path,
    slides_config_path,
    plots_folder,
    exe,
):
    exe_fullpath = os.path.join(executable_absolute_path, exe)

    run_all(
        input,
        streams,
        exe_fullpath,
        project_absolute_path,
        results_absolute_path,
        slides_config_path,
        plots_folder,
    )
