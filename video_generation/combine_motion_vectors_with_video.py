import cv2
import numpy as np
import sys
from tqdm import tqdm

import motion_vector as mv

def create_combined_video(input_video_filename: str,
                          motion_dfs: list,
                          results_path: str,
                          video_segment_index: int = None,
                          max_frames: int = 660) -> str:
    cap = cv2.VideoCapture(input_video_filename)
    if not cap.isOpened():
        raise IOError(f"Cannot open video file {input_video_filename}")

    try:
        # Video parameters
        frame_width = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
        frame_height = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
        fps = int(cap.get(cv2.CAP_PROP_FPS))

        # Number of segments: one per motion-df + one for the video itself (unless no motion_dfs)
        if len(motion_dfs) > 0:
            num_segments = len(motion_dfs) + 1
        else:
            num_segments = 1

        combined_width = frame_width * num_segments

        # Default video segment index: append the video after the motion segments
        if video_segment_index is None:
            video_segment_index = len(motion_dfs)

        # Determine max number of frames across csv and video
        num_frames_csv = max((df['frame'].max() if not df.empty else 0) for df in motion_dfs) if motion_dfs else 0
        num_frames_video = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
        num_frames = max(num_frames_csv, num_frames_video)

        # Create VideoWriter for combined output
        fourcc = cv2.VideoWriter_fourcc(*'mp4v')
        output_path = results_path + '/combined_motion_vectors_with_video.mp4'
        out = cv2.VideoWriter(output_path, fourcc, fps, (combined_width, frame_height))

        # Rewind to first frame (in case cap was used earlier)
        cap.set(cv2.CAP_PROP_POS_FRAMES, 0)

        for frame_num in tqdm(range(1, num_frames + 1), desc="Rendering video frames"):
            if frame_num > max_frames:
                break

            # Create combined black frame
            combined_frame = np.zeros((frame_height, combined_width, 3), dtype=np.uint8)

            # Read frame from input video
            if frame_num <= num_frames_video:
                ret, video_frame = cap.read()
                if not ret:
                    video_frame = np.zeros((frame_height, frame_width, 3), dtype=np.uint8)
                else:
                    video_frame = cv2.resize(video_frame, (frame_width, frame_height))
            else:
                video_frame = np.zeros((frame_height, frame_width, 3), dtype=np.uint8)

            for i in range(num_segments):
                segment_x_offset = i * frame_width

                if i == video_segment_index:
                    # Place the video frame
                    combined_frame[:, segment_x_offset:segment_x_offset + frame_width] = video_frame
                else:
                    # Draw motion vectors for the corresponding method_id
                    method_idx = i if i < video_segment_index else i - 1

                    # Defensive: if method_idx is out of range, just place a blank segment
                    if method_idx < 0 or method_idx >= len(motion_dfs):
                        segment_img = np.zeros((frame_height, frame_width, 3), dtype=np.uint8)
                    else:
                        df = motion_dfs[method_idx]
                        segment_img = np.zeros((frame_height, frame_width, 3), dtype=np.uint8)
                        frame_data = df[df['frame'] == frame_num]

                        frame_data = mv.reduce_motion_vectors(frame_data, max_vectors=15000)
                        mv.draw_motion_vectors(segment_img, frame_data)

                    combined_frame[:, segment_x_offset:segment_x_offset + frame_width] = segment_img

                # Draw vertical dividing line between segments
                if i > 0:
                    cv2.line(combined_frame, (segment_x_offset, 0), (segment_x_offset, frame_height), (128, 128, 128), 1)

            out.write(combined_frame)

        out.release()
        return output_path

    finally:
        cap.release()


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python combine_motion_vectors_with_video.py [video_file] [csv_file] [results_path] [video_segment_index] [max_frames]")
        sys.exit(1)

    input_video_filename = sys.argv[1]
    csv_file = sys.argv[2]
    results_path = sys.argv[3]

    # Load CSV file into DataFrame
    all_mvs = mv.load_motion_vectors(csv_file)

    # Find unique method_ids in the CSV
    method_ids = sorted(all_mvs['method_id'].unique())

    # Only keep method_ids for extractor0 and extractor7 that actually exist in the data
    method_ids = [mid for mid in method_ids if mid in [0, 7]]
    motion_dfs = [all_mvs[all_mvs['method_id'] == mid] for mid in method_ids]

    # Get video segment index (optional)
    if len(sys.argv) > 4:
        video_segment_index = int(sys.argv[4])
    else:
        video_segment_index = None

    # Get max_frames from command line if provided
    if len(sys.argv) > 5:
        max_frames = int(sys.argv[5])
    else:
        max_frames = 660

    output_path = create_combined_video(input_video_filename, motion_dfs, results_path, video_segment_index, max_frames)
    print(f"Combined video saved as {output_path}")