mod motion_vector;

pub use shared::get_max_frame;
use motion_vector::{
    draw_motion_vectors, get_frame_vectors, load_motion_vectors,
    reduce_motion_vectors, MotionVector,
};
use opencv::core::{Mat, Rect, Scalar, Size, CV_8UC3};
use opencv::imgproc;
use opencv::prelude::MatExprTraitConst;
use opencv::prelude::MatTraitConst;
use opencv::prelude::VideoWriterTraitConst;
use opencv::videoio::{
    VideoCapture, VideoCaptureTrait, VideoCaptureTraitConst, VideoWriter, VideoWriterTrait,
    CAP_PROP_FPS, CAP_PROP_FRAME_COUNT, CAP_PROP_FRAME_HEIGHT, CAP_PROP_FRAME_WIDTH,
    CAP_PROP_POS_FRAMES,
};
use std::env;
use std::process;

/// Read current video frame, resized to target dimensions.
/// Returns a black frame if past the end of the video or if the read fails.
fn read_video_frame(
    video_capture: &mut VideoCapture,
    frame_number: i32,
    total_video_frames: i32,
    frame_width: i32,
    frame_height: i32,
) -> Result<Mat, Box<dyn std::error::Error>> {
    if frame_number > total_video_frames {
        return Ok(Mat::zeros_size(Size::new(frame_width, frame_height), CV_8UC3)?.to_mat()?);
    }

    let mut frame = Mat::default();
    let success = video_capture.read(&mut frame)?;
    if !success || frame.empty() {
        return Ok(Mat::zeros_size(Size::new(frame_width, frame_height), CV_8UC3)?.to_mat()?);
    }

    let mut resized = Mat::default();
    imgproc::resize(
        &frame,
        &mut resized,
        Size::new(frame_width, frame_height),
        0.0,
        0.0,
        imgproc::INTER_LINEAR,
    )?;
    Ok(resized)
}

/// Render a single motion vector segment image for the given dataframe and frame number.
fn render_motion_segment(
    motion_dataframes: &[Vec<MotionVector>],
    motion_df_index: i32,
    frame_number: i32,
    frame_width: i32,
    frame_height: i32,
) -> Result<Mat, Box<dyn std::error::Error>> {
    let mut segment_image =
        Mat::zeros_size(Size::new(frame_width, frame_height), CV_8UC3)?.to_mat()?;

    if motion_df_index >= 0 && (motion_df_index as usize) < motion_dataframes.len() {
        let mut frame_motion_data = get_frame_vectors(
            &motion_dataframes[motion_df_index as usize],
            frame_number,
        );
        frame_motion_data = reduce_motion_vectors(&frame_motion_data, 15000);
        draw_motion_vectors(&mut segment_image, &frame_motion_data)?;
    }

    Ok(segment_image)
}

/// Compose all segments (video + motion vectors) into a single wide frame,
/// with vertical dividing lines between segments.
fn compose_combined_frame(
    video_frame: &Mat,
    motion_dataframes: &[Vec<MotionVector>],
    frame_number: i32,
    num_segments: i32,
    video_seg_idx: i32,
    frame_width: i32,
    frame_height: i32,
) -> Result<Mat, Box<dyn std::error::Error>> {
    let combined_width = frame_width * num_segments;
    // Initialize combined frame canvas
    let mut combined_frame =
        Mat::zeros_size(Size::new(combined_width, frame_height), CV_8UC3)?.to_mat()?;

    for segment_index in 0..num_segments {
        let segment_x_offset = segment_index * frame_width;
        let roi = Rect::new(segment_x_offset, 0, frame_width, frame_height);

        let segment_image = if segment_index == video_seg_idx {
            // Place original video frame
            video_frame.clone()
        } else {
            // Draw motion vectors for corresponding method
            let motion_df_index = if segment_index < video_seg_idx {
                segment_index
            } else {
                segment_index - 1
            };
            render_motion_segment(
                motion_dataframes,
                motion_df_index,
                frame_number,
                frame_width,
                frame_height,
            )?
        };

        let mut roi_mat = Mat::roi_mut(&mut combined_frame, roi)?;
        segment_image.copy_to(&mut roi_mat)?;

        // Draw vertical dividing line between segments
        if segment_index > 0 {
            imgproc::line(
                &mut combined_frame,
                opencv::core::Point::new(segment_x_offset, 0),
                opencv::core::Point::new(segment_x_offset, frame_height),
                Scalar::new(128.0, 128.0, 128.0, 0.0),
                1,
                imgproc::LINE_8,
                0,
            )?;
        }
    }

    Ok(combined_frame)
}

fn create_combined_video(
    input_video_filename: &str,
    motion_dataframes: &[Vec<MotionVector>],
    output_path: &str,
    video_segment_index: Option<i32>,
    max_frames: i32,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut video_capture = VideoCapture::from_file(input_video_filename, 0)?;
    if !video_capture.is_opened()? {
        return Err(format!("Cannot open video file {}", input_video_filename).into());
    }

    let frame_width = video_capture.get(CAP_PROP_FRAME_WIDTH)? as i32;
    let frame_height = video_capture.get(CAP_PROP_FRAME_HEIGHT)? as i32;
    let fps = video_capture.get(CAP_PROP_FPS)?;

    // Number of segments: one per motion dataframe + one for original video
    let num_segments = if motion_dataframes.is_empty() {
        1
    } else {
        motion_dataframes.len() as i32 + 1
    };
    let combined_width = frame_width * num_segments;

    // Default video segment index: append video after motion segments
    let video_seg_idx = video_segment_index.unwrap_or(motion_dataframes.len() as i32);

    // Determine maximum frames across all data sources
    let max_csv_frames = motion_dataframes
        .iter()
        .map(|df| get_max_frame(df))
        .max()
        .unwrap_or(0);
    let total_video_frames = video_capture.get(CAP_PROP_FRAME_COUNT)? as i32;
    let num_frames = max_csv_frames.max(total_video_frames);

    // Initialize video writer
    let fourcc = VideoWriter::fourcc('m', 'p', '4', 'v')?;
    let mut video_writer = VideoWriter::new(
        output_path,
        fourcc,
        fps,
        Size::new(combined_width, frame_height),
        true,
    )?;

    if !video_writer.is_opened()? {
        return Err(format!("Cannot open video writer for {}", output_path).into());
    }

    // Reset to first frame
    video_capture.set(CAP_PROP_POS_FRAMES, 0.0)?;

    for frame_number in 1..=num_frames {
        if frame_number > max_frames {
            break;
        }
        // Read current video frame
        let video_frame = read_video_frame(
            &mut video_capture,
            frame_number,
            total_video_frames,
            frame_width,
            frame_height,
        )?;

        let combined_frame = compose_combined_frame(
            &video_frame,
            motion_dataframes,
            frame_number,
            num_segments,
            video_seg_idx,
            frame_width,
            frame_height,
        )?;

        video_writer.write(&combined_frame)?;

        // Progress output
        if num_frames > 10 && frame_number % (num_frames / 10) == 0 {
            println!(
                "  {}/{} frames rendered",
                frame_number,
                num_frames.min(max_frames)
            );
        }
    }

    video_writer.release()?;
    drop(video_capture);

    Ok(output_path.to_string())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 5 {
        println!(
            "Usage: combine_motion_vectors_with_video \
             [video_file] [csv_file_orig] [csv_file_cust] [results_path] \
             [video_segment_index] [max_frames]"
        );
        process::exit(1);
    }

    let input_video_filename = &args[1];
    let csv_file_path_orig = &args[2];
    let csv_file_path_cust = &args[3];
    let results_directory = &args[4];

    let original_motion_vectors = match load_motion_vectors(csv_file_path_orig) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error loading original CSV: {}", e);
            process::exit(1);
        }
    };
    let custom_motion_vectors = match load_motion_vectors(csv_file_path_cust) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error loading custom CSV: {}", e);
            process::exit(1);
        }
    };

    let video_position: Option<i32> = if args.len() > 5 {
        args[5].parse().ok()
    } else {
        None
    };

    let max_frames_to_process: i32 = if args.len() > 6 {
        args[6].parse().unwrap_or(660)
    } else {
        660
    };

    let output_path = format!(
        "{}/combined_motion_vectors_with_video.mp4",
        results_directory
    );

    match create_combined_video(
        input_video_filename,
        &[original_motion_vectors, custom_motion_vectors],
        &output_path,
        video_position,
        max_frames_to_process,
    ) {
        Ok(result) => println!("Combined video saved as {}", result),
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}
