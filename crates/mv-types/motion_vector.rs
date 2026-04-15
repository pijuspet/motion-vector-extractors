use std::fs::File;
use std::io::{BufRead, BufReader, Write};

#[derive(Debug, Clone, Default)]
pub struct MotionVector {
    pub frame: i32,
    pub source: i32, // remove this?
    pub w: i32, // remove this?
    pub h: i32, // remove this?
    pub src_x: f64,
    pub src_y: f64,
    pub dst_x: f64,
    pub dst_y: f64,
    // `u64` to match `AVMotionVector::flags`. Serialized in hex (`0x...`) to
    // stay compatible with the C++ writer.
    pub flags: u64, // remove this?
    pub motion_x: f64,
    pub motion_y: f64,
    pub motion_scale: f64, // remove this??
}

fn parse_f64(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn parse_flags(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(stripped) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(stripped, 16).ok()
    } else {
        t.parse::<u64>().ok()
    }
}

pub fn load_motion_vectors(
    csv_file: &str,
) -> Result<Vec<MotionVector>, Box<dyn std::error::Error>> {
    let file = File::open(csv_file)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

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
            flags: col_flags
                .and_then(|i| fields.get(i))
                .and_then(|s| parse_flags(s))
                .unwrap_or(0),
            motion_x: motion_x_val,
            motion_y: motion_y_val,
            motion_scale: get_field(&fields, col_motion_scale).unwrap_or(0.0),
        };

        vectors.push(mv);
    }

    Ok(vectors)
}

/// Columns written by `write_motion_vectors` / `print_motion_vectors`.
///
/// This list is the single source of truth for the CSV output format. When a
/// field becomes unused (e.g. behind a future "fast" flag), remove it from here
/// and the loader will continue to parse older files correctly — `load_motion_vectors`
/// treats every column except `frame`/`src_x`/`src_y`/`dst_x`/`dst_y` as optional
/// and falls back to sensible defaults when a column is absent.
const MV_COLUMNS: &[(&str, fn(&MotionVector) -> String)] = &[
    ("frame", |v| v.frame.to_string()),
    ("source", |v| v.source.to_string()),
    ("w", |v| v.w.to_string()),
    ("h", |v| v.h.to_string()),
    ("src_x", |v| format!("{}", v.src_x)),
    ("src_y", |v| format!("{}", v.src_y)),
    ("dst_x", |v| format!("{}", v.dst_x)),
    ("dst_y", |v| format!("{}", v.dst_y)),
    ("flags", |v| format!("0x{:x}", v.flags)),
    ("motion_x", |v| format!("{}", v.motion_x)),
    ("motion_y", |v| format!("{}", v.motion_y)),
    ("motion_scale", |v| format!("{}", v.motion_scale)),
];

fn write_row<W: Write>(w: &mut W, row: &[String]) -> std::io::Result<()> {
    let mut first = true;
    for cell in row {
        if !first {
            w.write_all(b",")?;
        }
        w.write_all(cell.as_bytes())?;
        first = false;
    }
    w.write_all(b"\n")
}

/// Streaming CSV writer — writes the header on construction and lets callers
/// append rows incrementally. Used by the extractor binaries, which emit one
/// batch of motion vectors per decoded frame and do not want to buffer the
/// entire video in memory. Keeps a running `total` so the binary can print the
/// same `<frames> <mvs>` summary as the C++ version.
pub struct MotionVectorCsvWriter<W: Write> {
    inner: W,
    total: i32,
}

impl<W: Write> MotionVectorCsvWriter<W> {
    pub fn new(mut inner: W) -> std::io::Result<Self> {
        let header: Vec<String> = MV_COLUMNS.iter().map(|(name, _)| name.to_string()).collect();
        write_row(&mut inner, &header)?;
        Ok(Self { inner, total: 0 })
    }

    pub fn write(&mut self, v: &MotionVector) -> std::io::Result<()> {
        let row: Vec<String> = MV_COLUMNS.iter().map(|(_, fmt)| fmt(v)).collect();
        write_row(&mut self.inner, &row)?;
        self.total += 1;
        Ok(())
    }

    pub fn total(&self) -> i32 {
        self.total
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub fn get_frame_vectors(all_vectors: &[MotionVector], frame_number: i32) -> Vec<MotionVector> {
    all_vectors
        .iter()
        .filter(|v| v.frame == frame_number)
        .cloned()
        .collect()
}

pub fn get_max_frame(vectors: &[MotionVector]) -> i32 {
    vectors.iter().map(|v| v.frame).max().unwrap_or(0)
}

pub fn reduce_motion_vectors(
    frame_data: &[MotionVector],
    max_vectors: usize,
) -> Vec<MotionVector> {
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
        mag_indices.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        mag_indices.truncate(max_vectors);
    }

    mag_indices
        .iter()
        .map(|(_, idx)| frame_data[*idx].clone())
        .collect()
}
