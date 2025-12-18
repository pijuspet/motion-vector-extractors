import sys
import numpy as np 
import cv2 
import os
from tqdm import tqdm

import motion_vector as mv

def create_optimized_motion_vector_video(df, output_path,
                                       frame_width=1920, frame_height=1080, 
                                       fps=24, max_vectors_per_frame=15000):
    """Optimized video creation with progress tracking."""
    
    frames = sorted(df['frame'].unique())
    print(f"Creating video with {len(frames)} frames...")
    
    fourcc = cv2.VideoWriter_fourcc(*'mp4v')
    out = cv2.VideoWriter(output_path, fourcc, fps, (frame_width, frame_height))
    
    for i, frame_num in enumerate(tqdm(frames, desc="Rendering video frames")):
        frame_data = df[df['frame'] == frame_num]
        
        # Reduce vectors per frame for performance
        frame_data = mv.reduce_motion_vectors(frame_data, max_vectors_per_frame)
        
        # Create frame image
        img = np.zeros((frame_height, frame_width, 3), dtype=np.uint8)
        
        mv.draw_motion_vectors(img, frame_data)
       
        # Add frame info
        cv2.putText(img, f'Frame: {frame_num}', (50, 50), 
                    cv2.FONT_HERSHEY_SIMPLEX, 1.5, (255, 255, 255), 3)
        cv2.putText(img, f'Vectors: {len(frame_data)}', (50, 100), 
                    cv2.FONT_HERSHEY_SIMPLEX, 1.0, (255, 255, 255), 2)
        
        out.write(img)
    
    out.release()
    print(f"Saved optimized motion vector video: {output_path}")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python generate_motion_vectors_video.py [csv_file] [results_path]")
        sys.exit(1)

    csv_file = sys.argv[1]
    results_path = sys.argv[2]
    
    if not os.path.isfile(csv_file):
        print(f"Error: File '{csv_file}' not found.")
        exit(1)
    
    print("Loading motion vector data...")
    df = mv.load_motion_vectors(csv_file)
    
    print(f"Loaded {len(df):,} motion vectors.")
    print(f"Frames in data: {sorted(df['frame'].unique())[:10]}{'...' if len(df['frame'].unique())>10 else ''}")
    
    # Get first frame for visualization
    frame_to_visualize = df['frame'].min()
    
    print("Creating motion vector video...")
    create_optimized_motion_vector_video(df, results_path + '/motion_vectors_video_optimized.mp4')
    
    print("Visualization complete!")