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

/// Compact motion vector mirroring `AVMotionVectorCompact` from the custom
/// FFmpeg patch. Stores only the two endpoints (src + dst) and direction,
/// dropping w/h/flags/motion_x/motion_y/motion_scale.
#[derive(Debug, Clone, Default)]
pub struct MvCompact {
    pub frame: i32,
    pub source: i32,
    pub src_x: i16,
    pub src_y: i16,
    pub dst_x: i16,
    pub dst_y: i16,
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

#[derive(Clone, Copy)]
struct MvCols {
    frame: usize,
    src_x: usize,
    src_y: usize,
    dst_x: Option<usize>,
    dst_y: Option<usize>,
    source: Option<usize>,
    w: Option<usize>,
    h: Option<usize>,
    flags: Option<usize>,
    motion_x: Option<usize>,
    motion_y: Option<usize>,
    motion_scale: Option<usize>,
}

impl MvCols {
    fn from_header(header: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let headers: Vec<&str> = header
            .trim_end_matches(&['\r', '\n'][..])
            .split(',')
            .map(|s| s.trim())
            .collect();
        let find = |name: &str| headers.iter().position(|h| *h == name);
        Ok(Self {
            frame: find("frame").ok_or("Missing 'frame' column")?,
            src_x: find("src_x").ok_or("Missing 'src_x' column")?,
            src_y: find("src_y").ok_or("Missing 'src_y' column")?,
            dst_x: find("dst_x"),
            dst_y: find("dst_y"),
            source: find("source"),
            w: find("w"),
            h: find("h"),
            flags: find("flags"),
            motion_x: find("motion_x"),
            motion_y: find("motion_y"),
            motion_scale: find("motion_scale"),
        })
    }

    fn parse_row(&self, fields: &[&str]) -> Option<MotionVector> {
        let frame_val = fields.get(self.frame).and_then(|s| parse_f64(s))? as i32;
        let src_x_val = fields.get(self.src_x).and_then(|s| parse_f64(s))?;
        let src_y_val = fields.get(self.src_y).and_then(|s| parse_f64(s))?;

        let get_f = |idx: Option<usize>| -> Option<f64> {
            idx.and_then(|i| fields.get(i)).and_then(|s| parse_f64(s))
        };

        let motion_scale_val = get_f(self.motion_scale).unwrap_or(0.0);
        let motion_x_val = get_f(self.motion_x).unwrap_or(0.0);
        let motion_y_val = get_f(self.motion_y).unwrap_or(0.0);

        let derive_dst = |src: f64, motion: f64| -> f64 {
            if motion_scale_val > 0.0 {
                src + motion / motion_scale_val
            } else {
                src
            }
        };
        let dst_x_val = get_f(self.dst_x).unwrap_or_else(|| derive_dst(src_x_val, motion_x_val));
        let dst_y_val = get_f(self.dst_y).unwrap_or_else(|| derive_dst(src_y_val, motion_y_val));

        Some(MotionVector {
            frame: frame_val,
            source: get_f(self.source).unwrap_or(0.0) as i32,
            w: get_f(self.w).unwrap_or(0.0) as i32,
            h: get_f(self.h).unwrap_or(0.0) as i32,
            src_x: src_x_val,
            src_y: src_y_val,
            dst_x: dst_x_val,
            dst_y: dst_y_val,
            flags: self
                .flags
                .and_then(|i| fields.get(i))
                .and_then(|s| parse_flags(s))
                .unwrap_or(0),
            motion_x: motion_x_val,
            motion_y: motion_y_val,
            motion_scale: motion_scale_val,
        })
    }
}

/// Streaming CSV reader — yields one `MotionVector` per `next()` without
/// buffering the whole file. Lets `mv_compare` walk multi-GB CSVs without
/// holding both inputs in memory at once.
pub struct MvCsvReader<R: BufRead> {
    inner: R,
    cols: MvCols,
    line: String,
}

impl<R: BufRead> MvCsvReader<R> {
    pub fn new(mut inner: R) -> Result<Self, Box<dyn std::error::Error>> {
        let mut header = String::new();
        if inner.read_line(&mut header)? == 0 {
            return Err("Empty CSV file".into());
        }
        let cols = MvCols::from_header(&header)?;
        Ok(Self { inner, cols, line: String::new() })
    }
}

impl<R: BufRead> Iterator for MvCsvReader<R> {
    type Item = std::io::Result<MotionVector>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.line.clear();
            match self.inner.read_line(&mut self.line) {
                Ok(0) => return None,
                Ok(_) => {}
                Err(e) => return Some(Err(e)),
            }
            let trimmed = self.line.trim_end_matches(&['\r', '\n'][..]);
            if trimmed.trim().is_empty() {
                continue;
            }
            let fields: Vec<&str> = trimmed.split(',').collect();
            if let Some(mv) = self.cols.parse_row(&fields) {
                return Some(Ok(mv));
            }
        }
    }
}

pub fn load_motion_vectors(
    csv_file: &str,
) -> Result<Vec<MotionVector>, Box<dyn std::error::Error>> {
    let file = File::open(csv_file)?;
    let reader = MvCsvReader::new(BufReader::new(file))?;
    let mut vectors = Vec::new();
    for mv in reader {
        vectors.push(mv?);
    }
    Ok(vectors)
}

/// Streaming CSV writer — writes the header on construction and lets callers
/// append rows incrementally. Used by the extractor binaries, which emit one
/// batch of motion vectors per decoded frame and do not want to buffer the
/// entire video in memory. Keeps a running `total` so the binary can print the
/// same `<frames> <mvs>` summary as the C++ version.
///
/// Rows are written directly to the underlying writer via `itoa`/`ryu` byte
/// formatters — no intermediate `String` or `Vec<String>` allocation per row.
/// Profiling showed the old `.to_string()`/`Vec::from_iter` path dominated
/// runtime on MV-heavy videos (≈30% of total), almost entirely from `malloc`
/// churn on the tiny per-column `String`s.
pub struct MotionVectorCsvWriter<W: Write> {
    inner: W,
    total: i64,
}

const MV_HEADER: &[u8] =
    b"frame,source,w,h,src_x,src_y,dst_x,dst_y,flags,motion_x,motion_y,motion_scale\n";

impl<W: Write> MotionVectorCsvWriter<W> {
    pub fn new(mut inner: W) -> std::io::Result<Self> {
        inner.write_all(MV_HEADER)?;
        Ok(Self { inner, total: 0 })
    }

    pub fn write(&mut self, v: &MotionVector) -> std::io::Result<()> {
        let mut ibuf = itoa::Buffer::new();
        let mut fbuf = ryu::Buffer::new();
        let w = &mut self.inner;
        w.write_all(ibuf.format(v.frame).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(ibuf.format(v.source).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(ibuf.format(v.w).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(ibuf.format(v.h).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(fbuf.format(v.src_x).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(fbuf.format(v.src_y).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(fbuf.format(v.dst_x).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(fbuf.format(v.dst_y).as_bytes())?;
        w.write_all(b",")?;
        // flags: hex, preserving "0x..." format for compatibility with the C++ writer.
        write!(w, "0x{:x}", v.flags)?;
        w.write_all(b",")?;
        w.write_all(fbuf.format(v.motion_x).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(fbuf.format(v.motion_y).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(fbuf.format(v.motion_scale).as_bytes())?;
        w.write_all(b"\n")?;
        self.total += 1;
        Ok(())
    }

    pub fn total(&self) -> i64 {
        self.total
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub struct MvCompactCsvWriter<W: Write> {
    inner: W,
    total: i64,
}

const MV_COMPACT_HEADER: &[u8] = b"frame,source,src_x,src_y,dst_x,dst_y\n";

impl<W: Write> MvCompactCsvWriter<W> {
    pub fn new(mut inner: W) -> std::io::Result<Self> {
        inner.write_all(MV_COMPACT_HEADER)?;
        Ok(Self { inner, total: 0 })
    }

    pub fn write(&mut self, v: &MvCompact) -> std::io::Result<()> {
        let mut buf = itoa::Buffer::new();
        let w = &mut self.inner;
        w.write_all(buf.format(v.frame).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(buf.format(v.source).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(buf.format(v.src_x).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(buf.format(v.src_y).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(buf.format(v.dst_x).as_bytes())?;
        w.write_all(b",")?;
        w.write_all(buf.format(v.dst_y).as_bytes())?;
        w.write_all(b"\n")?;
        self.total += 1;
        Ok(())
    }

    pub fn total(&self) -> i64 {
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
            let dx = v.dst_x - v.src_x;
            let dy = v.dst_y - v.src_y;
            let mag = (dx * dx + dy * dy).sqrt();
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