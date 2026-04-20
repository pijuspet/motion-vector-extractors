use std::ffi::CStr;
use std::fs::File;
use std::io::BufWriter;
use std::ffi::CString;
use std::ptr;

use ffmpeg_sys_next::{self as ff, AVMotionVector};
use mv_types::motion_vector::{MotionVector, MotionVectorCsvWriter, MvCompactCsvWriter};

#[cfg(feature = "custom_ffmpeg")]
use ffmpeg_sys_next::AVMotionVectorCompact;
#[cfg(feature = "custom_ffmpeg")]
use mv_types::motion_vector::MvCompact;

/// CLI arguments shared by every extractor binary.
///
/// Layout: `<input file> <print mv> <output file> <is verbose> <is single threaded>`
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

/// Compact counterpart to `FileMvWriter`, used when the decoder emits
/// `AV_FRAME_DATA_MOTION_VECTORS_COMPACT` instead of the full-size side data.
pub type FileMvCompactWriter = MvCompactCsvWriter<BufWriter<File>>;

/// Open the CSV output file and wrap it in a streaming writer.
pub fn open_mv_writer(path: &str) -> std::io::Result<FileMvWriter> {
    let file = File::create(path)?;
    MotionVectorCsvWriter::new(BufWriter::new(file))
}

pub fn open_mv_compact_writer(path: &str) -> std::io::Result<FileMvCompactWriter> {
    let file = File::create(path)?;
    MvCompactCsvWriter::new(BufWriter::new(file))
}

/// Open an `MvWriter`, picking the compact flavour when the custom FFmpeg
/// feature is enabled and the full flavour otherwise. Keeps the dispatch out
/// of every extractor binary.
pub fn open_mv_any(path: &str) -> std::io::Result<MvWriter> {
    #[cfg(feature = "custom_ffmpeg")]
    {
        Ok(MvWriter::Compact(open_mv_compact_writer(path)?))
    }
    #[cfg(not(feature = "custom_ffmpeg"))]
    {
        Ok(MvWriter::Full(open_mv_writer(path)?))
    }
}

/// Unified writer the extractors hold. Kept as an enum so the same binary can
/// deal with either side-data flavour without pulling feature gates into every
/// call site.
pub enum MvWriter {
    Full(FileMvWriter),
    Compact(FileMvCompactWriter),
}

impl MvWriter {
    pub fn total(&self) -> i32 {
        match self {
            MvWriter::Full(w) => w.total(),
            MvWriter::Compact(w) => w.total(),
        }
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        match self {
            MvWriter::Full(w) => w.flush(),
            MvWriter::Compact(w) => w.flush(),
        }
    }
}

/// Convert a raw `AVMotionVector` into our shared `MotionVector` type. Any
/// extractor writing to the CSV goes through this so the wire format stays in
/// sync with `mv_types::motion_vector::MV_COLUMNS`.
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

#[cfg(feature = "custom_ffmpeg")]
fn convert_av_mv_compact(frame_num: i32, mv: &AVMotionVectorCompact) -> MvCompact {
    MvCompact {
        frame: frame_num,
        source: mv.source,
        src_x: mv.src_x,
        src_y: mv.src_y,
        dst_x: mv.dst_x,
        dst_y: mv.dst_y,
    }
}

/// Compact counterpart to `write_side_data`. Reads a contiguous array of
/// `AVMotionVectorCompact` produced by the custom FFmpeg patch when
/// `motion_vectors_only=1` and appends each one to the compact writer.
///
/// # Safety
/// Same contract as `write_side_data` but for `AVMotionVectorCompact`.
#[cfg(feature = "custom_ffmpeg")]
pub unsafe fn write_side_data_compact(
    writer: &mut FileMvCompactWriter,
    frame_num: i32,
    mvs: *const AVMotionVectorCompact,
    size_bytes: usize,
) {
    if mvs.is_null() {
        eprintln!("Invalid motion vector");
        return;
    }
    let count = size_bytes / std::mem::size_of::<AVMotionVectorCompact>();
    for i in 0..count {
        let av = unsafe { &*mvs.add(i) };
        let v = convert_av_mv_compact(frame_num, av);
        if let Err(e) = writer.write(&v) {
            eprintln!("Failed to write motion vector: {}", e);
            return;
        }
    }
}

/// Pull motion vectors off a decoded frame and feed them to the writer,
/// picking the compact or full side-data slot based on what the decoder
/// populated. Returns `true` if any side data was found.
///
/// When built with `custom_ffmpeg`, `AV_FRAME_DATA_MOTION_VECTORS_COMPACT`
/// is checked first — the custom decoder populates exactly one of the two
/// slots based on `motion_vectors_only`.
///
/// # Safety
/// `frame` must be a valid, non-null `*mut AVFrame` owned by the caller.
pub unsafe fn write_frame_mvs(
    writer: &mut MvWriter,
    frame_num: i32,
    frame: *mut ff::AVFrame,
) -> bool {
    #[cfg(feature = "custom_ffmpeg")]
    if let MvWriter::Compact(w) = writer {
        let sd = ff::av_frame_get_side_data(
            frame,
            ff::AVFrameSideDataType::AV_FRAME_DATA_MOTION_VECTORS_COMPACT,
        );
        if !sd.is_null() && !(*sd).data.is_null() && (*sd).size > 0 {
            write_side_data_compact(
                w,
                frame_num,
                (*sd).data as *const AVMotionVectorCompact,
                (*sd).size as usize,
            );
            return true;
        }
        return false;
    }

    if let MvWriter::Full(w) = writer {
        let sd = ff::av_frame_get_side_data(
            frame,
            ff::AVFrameSideDataType::AV_FRAME_DATA_MOTION_VECTORS,
        );
        if !sd.is_null() && !(*sd).data.is_null() && (*sd).size > 0 {
            write_side_data(
                w,
                frame_num,
                (*sd).data as *const AVMotionVector,
                (*sd).size as usize,
            );
            return true;
        }
    }
    false
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

/// Print the FFmpeg runtime version to stderr.
pub fn print_ffmpeg_version() {
    unsafe {
        let v = CStr::from_ptr(ff::av_version_info()).to_string_lossy();
        eprintln!("FFmpeg version: {}", v);
    }
}

pub unsafe fn set_av_flags(fmt_ctx: &mut ff::AVFormatContext) -> Vec<*mut ff::AVDictionary> {
    let nb_streams = (*fmt_ctx).nb_streams as usize;
    let mut stream_opts: Vec<*mut ff::AVDictionary> = vec![ptr::null_mut(); nb_streams];

    let mv_only_key = CString::new("motion_vectors_only").unwrap();
    let mv_only_val = CString::new("1").unwrap();
    let export_key = CString::new("export_side_data").unwrap();
    let export_val = CString::new("+mvs").unwrap();

    for i in 0..nb_streams {
        ff::av_dict_set(&mut stream_opts[i], mv_only_key.as_ptr(), mv_only_val.as_ptr(), 0);
        ff::av_dict_set(&mut stream_opts[i], export_key.as_ptr(), export_val.as_ptr(), 0);
    }

    stream_opts
}

pub unsafe fn unset_av_flags(stream_opts: Vec<*mut ff::AVDictionary>) { 
    for mut dict in stream_opts {
        if !dict.is_null() {
            ff::av_dict_free(&mut dict);
        }
    }
}