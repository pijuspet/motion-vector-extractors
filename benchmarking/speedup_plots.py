import matplotlib.pyplot as plt
import os
import pandas as pd
import seaborn as sns

_METRIC_INFO = {
    "fps":              ("Throughput (FPS)",           "higher raw value = better"),
    "time_per_frame":   ("Latency (ms/frame)",        "lower raw value = better"),
    "cpu_ms_per_frame": ("CPU Efficiency (ms/frame)", "lower raw value = better"),
    "memory":           ("Memory Usage (kB)",          "lower raw value = better"),
}

_HIGHER_IS_BETTER = {k for k, (_, d) in _METRIC_INFO.items() if "higher" in d}
_METRICS = list(_METRIC_INFO)

# Per-(video_type, metric) y-axis ceiling for speedup line plots.
_SPEEDUP_Y_MAX = {
    #  (video_type,  metric)              y_max
    ("h264_cavlc",        "fps"):              4.7,
    ("h264_cavlc",        "time_per_frame"):   4.7,
    ("h264_cavlc",        "cpu_ms_per_frame"): 5.1,
    ("h264_cavlc",        "memory"):           1.7,

    ("h264_cabac",        "fps"):              2.0,
    ("h264_cabac",        "time_per_frame"):   2.0,
    ("h264_cabac",        "cpu_ms_per_frame"): 2.0,
    ("h264_cabac",        "memory"):           2.7,

    ("h264_avi",          "fps"):              3,
    ("h264_avi",          "time_per_frame"):   3,
    ("h264_avi",          "cpu_ms_per_frame"): 3,
    ("h264_avi",          "memory"):           2,

    ("h265",         "fps"):              3.7,
    ("h265",         "time_per_frame"):   3.7,
    ("h265",         "cpu_ms_per_frame"): 3.7,
    ("h265",         "memory"):           1.05,
}

_SPEEDUP_Y_MIN = {
    ("h265",         "memory"):           0.95,
}


def build_color_map(df: pd.DataFrame) -> dict:
    all_methods = list(dict.fromkeys(df["method"]))
    palette = sns.color_palette(n_colors=len(all_methods))
    return dict(zip(all_methods, palette))


def detect_baseline_method(df: pd.DataFrame) -> str:
    # agg = (
    #     df.groupby("method")
    #     .agg(
    #         avg_time=("time_per_frame", "mean"),
    #         avg_cpu=("cpu_ms_per_frame", "mean"),
    #         avg_mem=("memory", "mean"),
    #     )
    #     .sort_values(["avg_time", "avg_cpu", "avg_mem"], ascending=False)
    # )
    # baseline = agg.index[0]
    # print(f"[speedup] Auto-detected baseline (slowest method): '{baseline}'")
    
    # using original ffmpeg mv only fot consistent graphs
    return "Original FFmpeg MV only"


def compute_speedup_df(df: pd.DataFrame, baseline_method: str) -> pd.DataFrame:
    available_metrics = [m for m in _METRICS if m in df.columns]

    rows = []
    for streams_val in sorted(df["streams"].unique()):
        df_s = df[df["streams"] == streams_val]
        baseline_row = df_s[df_s["method"] == baseline_method]
        if baseline_row.empty:
            print(
                f"[speedup] WARNING: baseline '{baseline_method}' not found at streams={streams_val}, skipping"
            )
            continue

        for metric in available_metrics:
            base_val = baseline_row[metric].values[0]
            if base_val == 0:
                continue

            for _, row in df_s.iterrows():
                if row["method"] == baseline_method:
                    continue

                method_val = row[metric]
                if method_val == 0:
                    speedup = float("nan")
                elif metric in _HIGHER_IS_BETTER:
                    speedup = method_val / base_val
                else:
                    speedup = base_val / method_val

                rows.append(
                    {
                        "method": row["method"],
                        "streams": streams_val,
                        "metric": metric,
                        "speedup": speedup,
                    }
                )

    return pd.DataFrame(rows)


def _label(metric: str) -> str:
    return _METRIC_INFO[metric][0] if metric in _METRIC_INFO else metric


def _direction(metric: str) -> str:
    return _METRIC_INFO[metric][1] if metric in _METRIC_INFO else ""


def _y_max(metric: str, video_type: str) -> float:
    return _SPEEDUP_Y_MAX.get((video_type, metric), 4)


def _y_min(metric: str, video_type: str) -> float:
    return _SPEEDUP_Y_MIN.get((video_type, metric), 0.5)


def plot_speedup_line(
    speedup_df: pd.DataFrame,
    metric: str,
    baseline_method: str,
    color_map: dict,
    filename: str,
    plots_folder: str,
    video_type: str = "",
):
    sub = speedup_df[speedup_df["metric"] == metric].copy()
    if sub.empty:
        print(f"[speedup] No data for metric '{metric}', skipping plot.")
        return

    fig, ax = plt.subplots(figsize=(14, 8))

    methods_ordered = [
        m for m in dict.fromkeys(speedup_df["method"]) if m in sub["method"].values
    ]

    for method in methods_ordered:
        m_data = sub[sub["method"] == method].sort_values("streams")
        if m_data.empty:
            continue
        ax.plot(
            m_data["streams"],
            m_data["speedup"],
            marker="o",
            markersize=8,
            linewidth=2.5,
            label=method,
            color=color_map.get(method),
        )
        for _, row in m_data.iterrows():
            pct = (row["speedup"] - 1.0) * 100
            sign = "+" if pct >= 0 else ""
            ax.annotate(
                f"{row['speedup']:.2f}× ({sign}{pct:.0f}%)",
                (row["streams"], row["speedup"]),
                textcoords="offset points",
                xytext=(0, 12),
                ha="center",
                fontsize=8,
            )

    ax.axhline(
        y=1.0,
        color="red",
        linestyle="--",
        linewidth=2,
        label=f"Baseline ({baseline_method})",
    )

    label = _label(metric)
    direction = _direction(metric)
    ax.set_title(
        f"Speedup: {label}\n(× improvement over baseline — above 1.0 = better; raw: {direction})",
        fontsize=17,
        loc="left",
    )
    ax.set_xlabel("Streams", fontsize=14)
    ax.set_ylabel("Speedup (×)", fontsize=14)

    ax.set_ylim(_y_min(metric, video_type), _y_max(metric, video_type))

    stream_vals = sorted(sub["streams"].unique())
    ax.set_xticks(stream_vals)
    ax.set_xticklabels([str(s) for s in stream_vals], fontsize=12)
    ax.tick_params(axis="y", labelsize=12)

    ax.legend(title="Method", loc="best", fontsize=11, title_fontsize=12)
    ax.grid(axis="y", alpha=0.3)

    fig.tight_layout()
    save_path = os.path.join(plots_folder, filename)
    fig.savefig(save_path, dpi=150)
    plt.close(fig)
    print(f"[speedup] Saved: {save_path}")


def _render_heatmap(pivot: pd.DataFrame, title: str, filename: str, plots_folder: str):
    col_order = [c for c in _METRICS if c in pivot.columns]
    pivot = pivot[col_order]
    pivot.columns = [_label(c) for c in pivot.columns]

    annot_labels = pivot.copy().astype(str)
    for r in range(pivot.shape[0]):
        for c in range(pivot.shape[1]):
            val = pivot.iloc[r, c]
            pct = (val - 1.0) * 100
            sign = "+" if pct >= 0 else ""
            annot_labels.iloc[r, c] = f"{val:.2f}×\n({sign}{pct:.0f}%)"

    fig, ax = plt.subplots(figsize=(14, max(6, len(pivot) * 0.8 + 2)))
    sns.heatmap(
        pivot,
        annot=annot_labels,
        fmt="",
        cmap="RdYlGn",
        center=1.0,
        linewidths=0.5,
        ax=ax,
        cbar_kws={"label": "Speedup (×)"},
        annot_kws={"fontsize": 12},
    )
    ax.set_title(
        f"{title}\nGreen > 1.0 = better · Red < 1.0 = worse", fontsize=16, loc="left"
    )
    ax.set_ylabel("")
    ax.tick_params(axis="x", rotation=20, labelsize=12)
    ax.tick_params(axis="y", rotation=0, labelsize=12)
    fig.tight_layout()
    save_path = os.path.join(plots_folder, filename)
    fig.savefig(save_path, dpi=150)
    plt.close(fig)
    print(f"[speedup] Saved heatmap: {save_path}")


def plot_speedup_heatmap(speedup_df, baseline_method, filename, plots_folder):
    pivot = speedup_df.pivot_table(
        index="method", columns="metric", values="speedup", aggfunc="mean"
    )
    _render_heatmap(
        pivot,
        f"Average Speedup vs Baseline ({baseline_method})",
        filename,
        plots_folder,
    )


def plot_speedup_heatmap_per_stream(
    speedup_df, baseline_method, streams_val, filename, plots_folder
):
    sub = speedup_df[speedup_df["streams"] == streams_val]
    if sub.empty:
        print(f"[speedup] No data for streams={streams_val}, skipping heatmap.")
        return
    pivot = sub.pivot_table(
        index="method", columns="metric", values="speedup", aggfunc="first"
    )
    _render_heatmap(
        pivot,
        f"Speedup at {streams_val} Streams vs Baseline ({baseline_method})",
        filename,
        plots_folder,
    )


def add_speedup_slides(
    slides: list, df_hp: pd.DataFrame, plots_folder: str, video_type: str, config: dict
):
    if not config:
        return

    baseline_method = detect_baseline_method(df_hp)
    speedup_df = compute_speedup_df(df_hp, baseline_method)

    if speedup_df.empty:
        print("[speedup] Speedup DataFrame is empty — nothing to plot.")
        return

    heatmap_cfg = config.get("heatmap")
    plot_speedup_heatmap(
        speedup_df,
        baseline_method,
        heatmap_cfg["filename"],
        plots_folder,
    )
    slides.append(
        {
            "title": heatmap_cfg["title"].format(baseline=baseline_method),
            "subtitle": heatmap_cfg["subtitle"].format(baseline=baseline_method),
            "filename": heatmap_cfg["filename"],
        }
    )

    per_stream_heatmap_cfg = config.get("heatmap_per_stream")
    for streams_val in sorted(speedup_df["streams"].unique()):
        filename = per_stream_heatmap_cfg["filename"].format(streams=streams_val)
        plot_speedup_heatmap_per_stream(
            speedup_df,
            baseline_method,
            streams_val,
            filename,
            plots_folder,
        )
        slides.append(
            {
                "title": per_stream_heatmap_cfg["title"].format(
                    baseline=baseline_method, streams=streams_val
                ),
                "subtitle": per_stream_heatmap_cfg["subtitle"].format(
                    baseline=baseline_method, streams=streams_val
                ),
                "filename": filename,
            }
        )

    for per_metric_cfg in config.get("per_metric", []):
        metric = per_metric_cfg["metric"]
        filename = per_metric_cfg["filename"]

        plot_speedup_line(
            speedup_df,
            metric,
            baseline_method,
            build_color_map(df_hp),
            filename,
            plots_folder,
            video_type,
        )
        slides.append(
            {
                "title": per_metric_cfg["title"].format(baseline=baseline_method),
                "subtitle": per_metric_cfg["subtitle"].format(baseline=baseline_method),
                "filename": filename,
            }
        )