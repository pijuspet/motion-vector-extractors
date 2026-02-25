import numpy as np
import pandas as pd
import cv2


def load_motion_vectors(csv_file: str) -> pd.DataFrame:
    df = pd.read_csv(csv_file)

    # Verify and convert columns to numeric types
    expected_cols = [
        "frame",
        "source",
        "w",
        "h",
        "src_x",
        "src_y",
        "dst_x",
        "dst_y",
        "flags",
        "motion_x",
        "motion_y",
        "motion_scale",
    ]
    present_cols = [c for c in expected_cols if c in df.columns]

    for col in present_cols:
        df[col] = pd.to_numeric(df[col], errors="coerce")

    # Drop rows with missing src or dst coordinates
    df = df.dropna(subset=["frame", "src_x", "src_y", "dst_x", "dst_y"])

    # Add computed motion columns if not present
    if "motion_x" not in df.columns:
        df["motion_x"] = df["dst_x"] - df["src_x"]
    if "motion_y" not in df.columns:
        df["motion_y"] = df["dst_y"] - df["src_y"]

    return df.reset_index(drop=True)


def reduce_motion_vectors(frame_data: pd.DataFrame, max_vectors: int = 10000):
    # Calculate motion magnitude
    mag = np.hypot(frame_data["motion_x"], frame_data["motion_y"])
    frame_data = frame_data.copy()
    frame_data["magnitude"] = mag

    # Keep only significant motion (magnitude > 2 pixels)
    significant = frame_data[mag > 2]

    # If still too many, sample by magnitude
    if len(significant) > max_vectors:
        # Sort by magnitude and keep the largest motions
        significant = significant.nlargest(max_vectors, "magnitude")

    return significant


def draw_motion_vectors(img: np.ndarray, frame_data: pd.DataFrame):
    src_x = frame_data["src_x"].values.astype(int)
    src_y = frame_data["src_y"].values.astype(int)
    dst_x = frame_data["dst_x"].values.astype(int)
    dst_y = frame_data["dst_y"].values.astype(int)
    motion_x = frame_data["motion_x"].values
    motion_y = frame_data["motion_y"].values

    mag = np.hypot(motion_x, motion_y)

    valid_magnitude_mask = mag >= 2

    colors = np.zeros((len(mag), 3), dtype=np.uint8)
    colors[valid_magnitude_mask] = [255, 255, 255]
    colors[(mag > 10) & (mag <= 20)] = [0, 255, 255]
    colors[mag > 20] = [0, 0, 255]

    valid_indices = np.where(valid_magnitude_mask)[0]
    for idx in valid_indices:
        cv2.arrowedLine(
            img,
            (src_x[idx], src_y[idx]),
            (dst_x[idx], dst_y[idx]),
            tuple(colors[idx].tolist()),
            1,
            tipLength=0.3,
        )
        cv2.circle(img, (src_x[idx], src_y[idx]), 1, (255, 255, 255), -1)

    return img
