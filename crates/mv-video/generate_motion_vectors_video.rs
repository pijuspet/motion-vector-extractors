mod motion_vector_display;

use opencv::core::{Mat, Scalar, Size, CV_8UC3};
use opencv::imgproc;
use opencv::prelude::MatExprTraitConst;
use opencv::prelude::VideoWriterTraitConst;
use opencv::videoio::{VideoWriter, VideoWriterTrait};
use std::collections::BTreeSet;
use std::env;
use std::path::Path;
use std::process;

use crate::motion_vector_display::draw_motion_vectors;

use mv_types::motion_vector::{
    get_frame_vectors, load_motion_vectors, reduce_motion_vectors, MotionVector,
};

fn create_motion_vector_video(
    vectors: &[MotionVector],
    output_path: &str,
    width: i32,
    height: i32,
    fps: f64,
    max_vectors: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Collect sorted unique frame numbers
    let frames: Vec<i32> = {
        let set: BTreeSet<i32> = vectors.iter().map(|v| v.frame).collect();
        set.into_iter().collect()
    };

    println!("Creating video with {} frames...", frames.len());

    let fourcc = VideoWriter::fourcc('m', 'p', '4', 'v')?;
    let mut writer = VideoWriter::new(output_path, fourcc, fps, Size::new(width, height), true)?;

    if !writer.is_opened()? {
        return Err(format!("Cannot open video writer for {}", output_path).into());
    }

    for (i, &frame_num) in frames.iter().enumerate() {
        let mut frame_data = get_frame_vectors(vectors, frame_num);

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

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("Usage: generate_motion_vectors_video [csv_file] [output_dir]");
        process::exit(1);
    }

    let csv_file = &args[1];
    let output_dir = &args[2];

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

    println!("Creating motion vector video...");
    if let Err(e) = create_motion_vector_video(&vectors, &output_path, 1920, 1080, 24.0, 15000) {
        eprintln!("Error creating video: {}", e);
        process::exit(1);
    }
    println!("Visualization complete!");
}
