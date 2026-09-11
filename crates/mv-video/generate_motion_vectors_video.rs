mod motion_vector_display;

use opencv::core::{Mat, Scalar, Size, CV_8UC3};
use opencv::imgproc;
use opencv::prelude::MatExprTraitConst;
use opencv::prelude::VideoWriterTraitConst;
use opencv::videoio::{
    VideoCapture, VideoCaptureTraitConst, VideoWriter, VideoWriterTrait, CAP_PROP_FPS,
    CAP_PROP_FRAME_COUNT,
};
use std::collections::BTreeSet;
use std::env;
use std::path::Path;
use std::process;

use crate::motion_vector_display::draw_motion_vectors;

use mv_types::motion_vector::{
    group_by_frame, load_motion_vectors, reduce_motion_vectors, MotionVector,
};

fn create_motion_vector_video(
    index: &std::collections::HashMap<i32, Vec<MotionVector>>,
    output_path: &str,
    width: i32,
    height: i32,
    fps: f64,
    max_vectors: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Collect sorted unique frame numbers
    let frames: Vec<i32> = {
        let set: BTreeSet<i32> = index.keys().copied().collect();
        set.into_iter().collect()
    };

    println!("Creating video with {} frames...", frames.len());

    let fourcc = VideoWriter::fourcc('m', 'p', '4', 'v')?;
    let mut writer = VideoWriter::new(output_path, fourcc, fps, Size::new(width, height), true)?;

    if !writer.is_opened()? {
        return Err(format!("Cannot open video writer for {}", output_path).into());
    }

    for (i, &frame_num) in frames.iter().enumerate() {
        let empty: Vec<MotionVector> = Vec::new();
        let mut frame_data = index.get(&frame_num).unwrap_or(&empty).clone();

        if frame_data.len() > max_vectors {
            frame_data = reduce_motion_vectors(&frame_data, max_vectors);
        }

        let mut img = Mat::zeros_size(Size::new(width, height), CV_8UC3)?.to_mat()?;
        draw_motion_vectors(&mut img, &frame_data)?;

        // Draw frame info text
        imgproc::put_text(
            &mut img,
            &format!("Frame: {}", frame_num),
            opencv::core::Point::new(50, 50),
            imgproc::FONT_HERSHEY_SIMPLEX,
            1.5,
            Scalar::new(255.0, 255.0, 255.0, 0.0),
            3,
            imgproc::LINE_8,
            false,
        )?;
        imgproc::put_text(
            &mut img,
            &format!("Vectors: {}", frame_data.len()),
            opencv::core::Point::new(50, 100),
            imgproc::FONT_HERSHEY_SIMPLEX,
            1.0,
            Scalar::new(255.0, 255.0, 255.0, 0.0),
            2,
            imgproc::LINE_8,
            false,
        )?;

        writer.write(&img)?;

        // Progress output every 10%
        if frames.len() > 10 && (i + 1) % (frames.len() / 10) == 0 {
            println!("  {}/{} frames rendered", i + 1, frames.len());
        }
    }

    writer.release()?;
    println!("Saved optimized motion vector video: {}", output_path);

    Ok(())
}

/// Output frame rate that makes the motion-vector video last exactly as long as
/// the source clip.
///
/// The CSV holds one entry per *decoded* picture, and temporal decimation
/// (MV_SKIP_FRAME / MV_SKIP_EVERY_NTH in the custom FFmpeg) means that can be
/// a fraction of the source's pictures - decoding every 3rd leaves a third of
/// them. Rendering those at the source frame rate would play the clip 3x fast.
/// Scaling the rate by the same fraction restores real-time playback, so a
/// decimated overlay stays comparable with the source and with other runs.
///
/// Returns None when the source cannot be probed, leaving the caller's default.
fn playback_matched_fps(source_video: &str, mv_frames: usize) -> Option<f64> {
    let cap = VideoCapture::from_file(source_video, 0).ok()?;
    if !cap.is_opened().ok()? {
        return None;
    }
    let src_fps = cap.get(CAP_PROP_FPS).ok()?;
    let src_frames = cap.get(CAP_PROP_FRAME_COUNT).ok()?;
    if !(src_fps > 0.0) || src_frames < 1.0 || mv_frames == 0 {
        return None;
    }
    let fps = src_fps * (mv_frames as f64 / src_frames);
    println!(
        "Source: {:.3} fps, {} frames. MV data: {} frames ({:.2}x decimation).",
        src_fps, src_frames as i64, mv_frames, src_frames / mv_frames as f64
    );
    println!("Output rate {:.3} fps so playback matches the source duration.", fps);
    Some(fps)
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!(
            "Usage: generate_motion_vectors_video [csv_file] [output_dir] [source_video]"
        );
        println!("  source_video is optional; given, the output frame rate is scaled so the");
        println!("  video lasts as long as the source even when pictures were skipped.");
        process::exit(1);
    }

    let csv_file = &args[1];
    let output_dir = &args[2];
    let source_video = args.get(3);

    if !Path::new(csv_file).is_file() {
        eprintln!("Error: File '{}' not found.", csv_file);
        process::exit(1);
    }

    println!("Loading motion vector data...");
    let vectors = match load_motion_vectors(csv_file) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error loading CSV: {}", e);
            process::exit(1);
        }
    };

    let output_path = format!("{}/motion_vectors_video.mp4", output_dir);

    println!("Loaded {} motion vectors.", vectors.len());

    // Print first few unique frames
    let unique_frames: BTreeSet<i32> = vectors.iter().map(|v| v.frame).collect();
    let frame_preview: Vec<String> = unique_frames
        .iter()
        .take(10)
        .map(|f| f.to_string())
        .collect();
    let suffix = if unique_frames.len() > 10 { "..." } else { "" };
    println!("Frames in data: {}{}", frame_preview.join(", "), suffix);

    let fps = source_video
        .and_then(|v| playback_matched_fps(v, unique_frames.len()))
        .unwrap_or_else(|| {
            println!("No source video given; using the default 24 fps.");
            24.0
        });

    println!("Creating motion vector video...");
    let index = group_by_frame(vectors);
    if let Err(e) = create_motion_vector_video(&index, &output_path, 1920, 1080, fps, 15000) {
        eprintln!("Error creating video: {}", e);
        process::exit(1);
    }
    println!("Visualization complete!");
}
