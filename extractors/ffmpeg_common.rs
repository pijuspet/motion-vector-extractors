use std::ffi::CStr;
use std::fs::File;
use std::io::BufWriter;

use ffmpeg_next::sys::{self as ff, AVMotionVector};
use utils::motion_vector::{MotionVector, MotionVectorCsvWriter};

/// CLI arguments shared by every extractor binary.
///
/// All extractors take the same positional layout as the original C++:
///     <input file> <print mv> <output file> <is verbose> <is single threaded>
pub struct ExtractorArgs {
    pub video_file: String,
    pub do_print: bool,
    pub output_file: String,
    pub is_verbose: bool,
    pub is_single_threaded: bool,
}

impl ExtractorArgs {
    pub fn from_env() -> Option<Self> {
        let argv: Vec<String> = std::env::args().collect();
        if argv.len() < 6 {
            let exe = argv.first().cloned().unwrap_or_else(|| "extractor".to_string());
            eprintln!(
                "Usage: {} <input file> <print mv> <output file> <is verbose> <is single threaded>",
                exe
            );
            return None;
        }
        Some(Self {
            video_file: argv[1].clone(),
            do_print: argv[2].parse::<i32>().unwrap_or(0) != 0,
            output_file: argv[3].clone(),
            is_verbose: argv[4].parse::<i32>().unwrap_or(0) != 0,
            is_single_threaded: argv[5].parse::<i32>().unwrap_or(0) != 0,
        })
    }
}

/// Convenience wrapper around `MotionVectorCsvWriter<BufWriter<File>>` so
/// extractors don't have to spell out the generics.
pub type FileMvWriter = MotionVectorCsvWriter<BufWriter<File>>;

/// Open the CSV output file and wrap it in a streaming writer.
pub fn open_mv_writer(path: &str) -> std::io::Result<FileMvWriter> {
    let file = File::create(path)?;
    MotionVectorCsvWriter::new(BufWriter::new(file))
}

/// Convert a raw `AVMotionVector` into our shared `MotionVector` type. Any
/// extractor writing to the CSV goes through this so the wire format stays in
/// sync with `utils::motion_vector::MV_COLUMNS`.
///
/// Returns `None` when the motion vector has a zero dimension — matching the
/// guard from `extractors/writer.cpp`.
fn convert_av_mv(frame_num: i32, mv: &AVMotionVector) -> Option<MotionVector> {
    if mv.w == 0 || mv.h == 0 {
        eprintln!(
            "Invalid motion vector dimensions: {} x {}",
            mv.w as i32, mv.h as i32
        );
        return None;
    }
    Some(MotionVector {
        frame: frame_num,
        source: mv.source,
        w: mv.w as i32,
        h: mv.h as i32,
        src_x: mv.src_x as f64,
        src_y: mv.src_y as f64,
        dst_x: mv.dst_x as f64,
        dst_y: mv.dst_y as f64,
        flags: mv.flags,
        motion_x: mv.motion_x as f64,
        motion_y: mv.motion_y as f64,
        motion_scale: mv.motion_scale as f64,
    })
}

/// Append every motion vector from a frame's side-data blob to the writer.
///
/// # Safety
/// `mvs` must point to `size_bytes / size_of::<AVMotionVector>()` valid,
/// contiguous `AVMotionVector`s — exactly the pointer/size pair produced by
/// `av_frame_get_side_data` on `AV_FRAME_DATA_MOTION_VECTORS`.
pub unsafe fn write_side_data(
    writer: &mut FileMvWriter,
    frame_num: i32,
    mvs: *const AVMotionVector,
    size_bytes: usize,
) {
    if mvs.is_null() {
        eprintln!("Invalid motion vector");
        return;
    }
    let count = size_bytes / std::mem::size_of::<AVMotionVector>();
    for i in 0..count {
        let av = unsafe { &*mvs.add(i) };
        if let Some(v) = convert_av_mv(frame_num, av) {
            if let Err(e) = writer.write(&v) {
                eprintln!("Failed to write motion vector: {}", e);
                return;
            }
        }
    }
}

/// Read the current VmRSS of this process from `/proc/self/status`.
/// Returns the RSS in kilobytes, or 0 if it cannot be read.
///
/// Unlike `ru_maxrss` from `wait4`, this returns the *current* RSS after
/// `exec` — it is not inflated by the parent's RSS at `fork` time.
pub fn get_current_rss_kb() -> i64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(val) = line.strip_prefix("VmRSS:") {
            let val = val.trim().trim_end_matches(" kB").trim();
            return val.parse().unwrap_or(0);
        }
    }
    0
}

/// Print the FFmpeg runtime version to stderr (matches the `is_verbose` branch
/// of the C++ extractors).
pub fn print_ffmpeg_version() {
    unsafe {
        let v = CStr::from_ptr(ff::av_version_info()).to_string_lossy();
        eprintln!("FFmpeg version: {}", v);
    }
}
