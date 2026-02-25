from pathlib import Path
from typing import List
import pandas as pd
import sys

DTYPE_MAP = {
    "frame": "int32", "source": "int16",
    "w": "int16", "h": "int16",
    "src_x": "int16", "src_y": "int16",
    "dst_x": "int16", "dst_y": "int16",
    "motion_x": "int16", "motion_y": "int16",
    "motion_scale": "int16",
}

def compare_frames(
    first_method_df: pd.DataFrame,
    second_method_df: pd.DataFrame,
) -> List[str]:
    KEY_COLUMNS = ["frame", "src_x", "src_y", "dst_x", "dst_y", "source"]
    columns_to_compare = [col for col in first_method_df.columns if col not in KEY_COLUMNS]

    merged = first_method_df.merge(
        second_method_df,
        on=KEY_COLUMNS,
        how="outer",
        suffixes=("_first", "_second"),
        indicator=True,
    )

    key_tuple = list(zip(
        merged["frame"], merged["src_x"], merged["src_y"],
        merged["dst_x"], merged["dst_y"], merged["source"]
    ))
    merged["_key_tuple"] = key_tuple

    key_str = (
        "Frame " + merged["frame"].astype(str)
        + " src=(" + merged["src_x"].astype(str) + "," + merged["src_y"].astype(str) + ")"
        + " dst=(" + merged["dst_x"].astype(str) + "," + merged["dst_y"].astype(str) + ")"
        + " source=" + merged["source"].astype(str)
    )
    merged["_key_str"] = key_str

    diff_tuples: list = []

    # Missing rows
    for side, label in [("left_only", "second"), ("right_only", "first")]:
        mask = merged["_merge"] == side
        subset = merged[mask]
        for kt, ks in zip(subset["_key_tuple"], subset["_key_str"]):
            diff_tuples.append((kt, f"{ks}: missing in {label} file"))

    # Value differences
    both = merged[merged["_merge"] == "both"]
    for col in columns_to_compare:
        col_first, col_second = f"{col}_first", f"{col}_second"
        if col_first not in both.columns:
            continue

        both_null = both[col_first].isnull() & both[col_second].isnull()
        differs = (both[col_first] != both[col_second]) & ~both_null
        differing = both[differs]

        for kt, ks, v1, v2 in zip(
            differing["_key_tuple"], differing["_key_str"],
            differing[col_first], differing[col_second]
        ):
            diff_tuples.append((kt, f"{ks}: '{col}' differs (first={v1}, second={v2})"))

    diff_tuples.sort(key=lambda t: t[0])
    return [t[1] for t in diff_tuples]


def write_results(
    differences: List[str], output_path: Path,
) -> None:
    with open(output_path, "w") as output_file:
        if differences:
            output_file.write("\n".join(differences) + "\n")
        else:
            output_file.write(
                f"No differences found in frames in all frames.\n"
            )


def compare(
    first_file_path, second_file_path, output_file_path
):
    try:
        first_method_dataframe = pd.read_csv(first_file_path, dtype=DTYPE_MAP)
        second_method_dataframe = pd.read_csv(second_file_path, dtype=DTYPE_MAP)

        frame_differences: List[str] = compare_frames(
            first_method_dataframe, second_method_dataframe
        )
        write_results(frame_differences, output_file_path)
        print(f"Comparison complete. Results written to {output_file_path}")

    except FileNotFoundError as error:
        print(f"Error: Could not find file - {error}")
        sys.exit(1)
    except pd.errors.ParserError as error:
        print(f"Error: Could not parse CSV file - {error}")
        sys.exit(1)
    except KeyError as error:
        print(f"Error: Required column not found in CSV - {error}")
        sys.exit(1)
    except Exception as error:
        print(f"Unexpected error: {error}")
        sys.exit(1)