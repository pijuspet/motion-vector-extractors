import imgkit
import matplotlib.pyplot as plt
import os
import pandas as pd
import seaborn as sns

def highlight_table(df):
    def find_col(possibles):
        for p in possibles:
            for c in df.columns:
                if c.strip().lower().replace(" ", "").replace("_", "") == p:
                    return c
        return None

    col_time = find_col(
        ["time/frame(ms)", "timeperframe(ms)", "time/frame", "timeperframe"]
    )
    col_cpu = find_col(["cpu(%)", "cpu"])
    col_mem = find_col(["memΔkb", "memdelta", "mem", "memory"])
    col_fps = find_col(["fps"])

    styles = pd.DataFrame("", index=df.index, columns=df.columns)
    if col_time:
        min_time = df[col_time].min()
        styles.loc[df[col_time] == min_time, col_time] = (
            "background-color: #c6efce; color: black"
        )
    if col_cpu:
        min_cpu = df[col_cpu].min()
        styles.loc[df[col_cpu] == min_cpu, col_cpu] = (
            "background-color: #c6efce; color: black"
        )
    if col_mem and col_mem in df.columns:
        min_mem = df[col_mem].min()
        styles.loc[df[col_mem] == min_mem, col_mem] = (
            "background-color: #c6efce; color: black"
        )
    if col_fps:
        max_fps = df[col_fps].max()
        styles.loc[df[col_fps] == max_fps, col_fps] = (
            "background-color: #c6efce; color: black"
        )

    return df.style.apply(lambda _: styles, axis=None)


def save_highlighted_table_as_png(df, filename):
    styled = highlight_table(df)
    html_file = filename.replace(".png", ".html")
    styled.to_html(html_file)
    imgkit.from_file(html_file, filename)
    print(f"Saved highlighted table as {filename}")

def pretty_table(df, filename, plots_folder, col_width=2.8, row_height=0.8):
    n_rows, n_cols = df.shape
    fig, ax = plt.subplots(figsize=(col_width * n_cols, row_height * (n_rows + 1)))
    ax.axis("off")
    tbl = ax.table(
        cellText=df.values.tolist(),
        colLabels=list(df.columns),
        loc="center",
        cellLoc="center",
        colLoc="center",
        edges="open",
    )
    tbl.auto_set_font_size(False)
    tbl.set_fontsize(12)  # More compact font

    # header styling (lighter background, bold, slightly larger font)
    for j in range(n_cols):
        tbl[(0, j)].set_text_props(weight="bold", fontsize=15)
        tbl[(0, j)].set_facecolor("#f5f5f5")

    # Data row styling: alternating row color, center text
    for i in range(1, n_rows + 1):
        for j in range(n_cols):
            cell = tbl[(i, j)]
            cell.set_facecolor("#fafafa" if i % 2 == 0 else "white")
            cell.set_text_props(color="black", weight="normal", fontsize=12)
            cell.PAD = 0.12

    tbl.auto_set_column_width(col=list(range(n_cols)))
    fig.tight_layout(pad=0.5)

    outpath = os.path.join(plots_folder, filename)
    fig.savefig(outpath, dpi=200)
    plt.close(fig)
    print(f"Saved pretty table image: {outpath}")
    return filename

def plot_grouped_bar(
    df, metric, title, ylabel, filename, plots_folder, palette="tab20"
):
    plt.figure(figsize=(16, 9))
    sns.barplot(
        data=df, x="streams", y=metric, hue="method", palette=palette, edgecolor="black"
    )
    plt.title(title, fontsize=20, loc="left")
    plt.xlabel("Streams", fontsize=14)
    plt.ylabel(ylabel, fontsize=14)
    plt.legend(title="Method", loc="best", fontsize=12)
    plt.tight_layout()
    save_path = os.path.join(plots_folder, filename)
    plt.savefig(save_path)
    plt.close()
    print(f"Saved grouped bar chart: {save_path}")


def plot_metric(df, metric, title, ylabel, filename, plots_folder, palette="viridis"):
    plt.figure(figsize=(16, 9))
    sns.barplot(
        data=df, x="method", y=metric, hue="method", palette=palette, legend=False
    )
    plt.title(title, fontsize=20, loc="left")
    plt.xlabel("Method", fontsize=14)
    plt.ylabel(ylabel, fontsize=14)
    plt.xticks(rotation=30, ha="right", fontsize=12)
    plt.yticks(fontsize=12)
    plt.tight_layout()
    save_path = os.path.join(plots_folder, filename)
    plt.savefig(save_path)
    plt.close()
    print(f"Saved plot: {save_path}")


def plot_scaling(df, metric, title, ylabel, filename, plots_folder, legend_loc="best"):
    plt.figure(figsize=(16, 9))
    sns.lineplot(data=df, x="streams", y=metric, hue="method", marker="o")
    plt.title(title, fontsize=20, loc="left")
    plt.xlabel("Streams", fontsize=14)
    plt.ylabel(ylabel, fontsize=14)
    plt.legend(title="Method", loc=legend_loc, fontsize=12)
    plt.tight_layout()
    save_path = os.path.join(plots_folder, filename)
    plt.savefig(save_path)
    plt.close()
    print(f"Saved plot: {save_path}")