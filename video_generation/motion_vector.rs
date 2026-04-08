use opencv::core::{Mat, Point, Scalar};
use opencv::imgproc;
use std::fs::File;
use std::io::{BufRead, BufReader};

#[derive(Debug, Clone, Default)]
pub struct MotionVector {
    pub frame: i32,
    pub source: i32,
    pub w: i32,
    pub h: i32,
    pub src_x: f64,
    pub src_y: f64,
    pub dst_x: f64,
    pub dst_y: f64,
    pub flags: i32,
    pub motion_x: f64,
    pub motion_y: f64,
    pub motion_scale: f64,
}

fn parse_f64(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

/// Load motion vectors from a CSV file.
pub fn load_motion_vectors(
    csv_file: &str,
) -> Result<Vec<MotionVector>, Box<dyn std::error::Error>> {
    let file = File::open(csv_file)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    // Parse header
    let header_line = lines.next().ok_or("Empty CSV file")??;
    let headers: Vec<String> = header_line
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let find_col = |name: &str| -> Option<usize> { headers.iter().position(|h| h == name) };

    let col_frame = find_col("frame").ok_or("Missing 'frame' column")?;
    let col_src_x = find_col("src_x").ok_or("Missing 'src_x' column")?;
    let col_src_y = find_col("src_y").ok_or("Missing 'src_y' column")?;
    let col_dst_x = find_col("dst_x").ok_or("Missing 'dst_x' column")?;
    let col_dst_y = find_col("dst_y").ok_or("Missing 'dst_y' column")?;
    let col_source = find_col("source");
    let col_w = find_col("w");
    let col_h = find_col("h");
    let col_flags = find_col("flags");
    let col_motion_x = find_col("motion_x");
    let col_motion_y = find_col("motion_y");
    let col_motion_scale = find_col("motion_scale");

    let get_field = |fields: &[&str], idx: Option<usize>| -> Option<f64> {
        idx.and_then(|i| fields.get(i)).and_then(|s| parse_f64(s))
    };

    let mut vectors = Vec::new();

    for line_result in lines {
        let line = line_result?;
        if line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split(',').collect();

        let frame_val = match fields.get(col_frame).and_then(|s| parse_f64(s)) {
            Some(v) => v,
            None => continue,
        };
        let src_x_val = match fields.get(col_src_x).and_then(|s| parse_f64(s)) {
            Some(v) => v,
            None => continue,
        };
        let src_y_val = match fields.get(col_src_y).and_then(|s| parse_f64(s)) {
            Some(v) => v,
            None => continue,
        };
        let dst_x_val = match fields.get(col_dst_x).and_then(|s| parse_f64(s)) {
            Some(v) => v,
            None => continue,
        };
        let dst_y_val = match fields.get(col_dst_y).and_then(|s| parse_f64(s)) {
            Some(v) => v,
            None => continue,
        };

        let motion_x_val = get_field(&fields, col_motion_x).unwrap_or(dst_x_val - src_x_val);
        let motion_y_val = get_field(&fields, col_motion_y).unwrap_or(dst_y_val - src_y_val);

        let mv = MotionVector {
            frame: frame_val as i32,
            source: get_field(&fields, col_source).unwrap_or(0.0) as i32,
            w: get_field(&fields, col_w).unwrap_or(0.0) as i32,
            h: get_field(&fields, col_h).unwrap_or(0.0) as i32,
            src_x: src_x_val,
            src_y: src_y_val,
            dst_x: dst_x_val,
            dst_y: dst_y_val,
            flags: get_field(&fields, col_flags).unwrap_or(0.0) as i32,
            motion_x: motion_x_val,
            motion_y: motion_y_val,
            motion_scale: get_field(&fields, col_motion_scale).unwrap_or(0.0),
        };

        vectors.push(mv);
    }

    Ok(vectors)
}

/// Reduce motion vectors: keep only significant (magnitude > 2) vectors,
/// and if there are more than max_vectors, keep the largest ones.
pub fn reduce_motion_vectors(frame_data: &[MotionVector], max_vectors: usize) -> Vec<MotionVector> {
    let mut mag_indices: Vec<(f64, usize)> = frame_data
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let mag = (v.motion_x * v.motion_x + v.motion_y * v.motion_y).sqrt();
            if mag > 2.0 {
                Some((mag, i))
            } else {
                None
            }
        })
        .collect();

    if mag_indices.len() > max_vectors {
        // Sort by magnitude descending, keep top max_vectors
        mag_indices.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        mag_indices.truncate(max_vectors);
    }

    mag_indices
        .iter()
        .map(|(_, idx)| frame_data[*idx].clone())
        .collect()
}

/// Draw motion vectors onto an image with color-coded arrows.
pub fn draw_motion_vectors(
    img: &mut Mat,
    frame_data: &[MotionVector],
) -> Result<(), Box<dyn std::error::Error>> {
    for v in frame_data {
        let mag = (v.motion_x * v.motion_x + v.motion_y * v.motion_y).sqrt();
        if mag < 2.0 {
            continue;
        }

        let color = if mag > 20.0 {
            Scalar::new(0.0, 0.0, 255.0, 0.0) // Red (BGR)
        } else if mag > 10.0 {
            Scalar::new(0.0, 255.0, 255.0, 0.0) // Yellow (BGR)
        } else {
            Scalar::new(255.0, 255.0, 255.0, 0.0) // White (BGR)
        };

        let src = Point::new(v.src_x as i32, v.src_y as i32);
        let dst = Point::new(v.dst_x as i32, v.dst_y as i32);

        imgproc::arrowed_line(img, src, dst, color, 1, imgproc::LINE_8, 0, 0.3)?;
        imgproc::circle(
            img,
            src,
            1,
            Scalar::new(255.0, 255.0, 255.0, 0.0),
            -1,
            imgproc::LINE_8,
            0,
        )?;
    }

    Ok(())
}

/// Get the maximum frame number across all vectors.
pub fn get_max_frame(vectors: &[MotionVector]) -> i32 {
    vectors
        .iter()
        .map(|v| v.frame)
        .max()
        .unwrap_or(0)
}

/// Get all vectors for a specific frame number.
pub fn get_frame_vectors(all_vectors: &[MotionVector], frame_number: i32) -> Vec<MotionVector> {
    all_vectors
        .iter()
        .filter(|v| v.frame == frame_number)
        .cloned()
        .collect()
}
