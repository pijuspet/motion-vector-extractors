use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{exit, Command};
use std::thread;

// (command-line name, menu label). "picked" browses the links earlier picks left
// in videos/nas/<type>/ instead of the NAS tree.
const DATASETS: [(&str, &str); 3] =
    [("meva", "MEVA"), ("ilids", "iLIDS"), ("picked", "previously picked (videos/nas)")];
const VIDEO_EXTS: [&str; 7] = ["mp4", "avi", "mkv", "mov", "ts", "h264", "hevc"];
const FFPROBE_PREFIX: &str = "ffmpeg/install/FFmpeg-8.0";

/// ffprobe codec name -> benchmark video type.
// ponytail: H.264 is assumed CABAC (High profile, what both datasets ship);
// telling CAVLC apart needs a decode, add it if CAVLC clips appear.
fn video_type(codec: &str) -> Option<&'static str> {
    match codec {
        "h264" => Some("h264_cabac"),
        "hevc" => Some("h265"),
        "mpeg4" => Some("h264_avi"),
        _ => None,
    }
}

fn die(msg: impl AsRef<str>) -> ! {
    eprintln!("{}", msg.as_ref());
    exit(1)
}

fn ffprobe(root: &Path, path: &Path, entries: &str) -> String {
    let prefix = root.join(FFPROBE_PREFIX);
    Command::new(prefix.join("bin/ffprobe"))
        .env("LD_LIBRARY_PATH", prefix.join("lib"))
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", entries, "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn fmt_info(bytes: u64, secs: Option<u64>) -> String {
    let size = format!("{:.1} MB", bytes as f64 / 1e6);
    match secs {
        None => format!("{size}, ?:??"),
        Some(s) if s >= 3600 => format!("{size}, {}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60),
        Some(s) => format!("{size}, {}:{:02}", s / 60, s % 60),
    }
}

/// "312.4 MB, 4:56" - file size and container duration.
fn describe(root: &Path, path: &Path) -> String {
    let bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let secs = ffprobe(root, path, "format=duration")
        .lines()
        .last()
        .and_then(|l| l.parse::<f64>().ok())
        .map(|s| s as u64);
    fmt_info(bytes, secs)
}

fn prompt(msg: &str) -> String {
    print!("{msg}");
    io::stdout().flush().ok();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
        exit(0); // EOF
    }
    line.trim().to_string()
}

fn is_video(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

fn browse(root: &Path, base: &Path, start: &Path) -> PathBuf {
    let (mut cur, mut filt) = (start.to_path_buf(), String::new());
    loop {
        let mut entries: Vec<PathBuf> = fs::read_dir(&cur)
            .unwrap_or_else(|e| die(format!("cannot list {}: {e}", cur.display())))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir() || is_video(p))
            .filter(|p| name(p).to_lowercase().contains(&filt.to_lowercase()))
            .collect();
        entries.sort();

        // One ffprobe per clip, run concurrently: listing is NAS latency-bound.
        let info: Vec<Option<String>> = thread::scope(|s| {
            let handles: Vec<_> = entries
                .iter()
                .map(|p| s.spawn(move || p.is_file().then(|| describe(root, p))))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap_or(None)).collect()
        });

        let rel = cur.strip_prefix(base).unwrap_or(&cur).display().to_string();
        println!("\n{rel}/{}", if filt.is_empty() { String::new() } else { format!("  (filter: {filt})") });
        let width = entries.iter().map(|p| name(p).len()).max().unwrap_or(0);
        for (i, (p, inf)) in entries.iter().zip(&info).enumerate() {
            match inf {
                Some(inf) => println!("  {:3}  {:<width$}  {inf}", i + 1, name(p)),
                None => println!("  {:3}  {}/", i + 1, name(p)),
            }
        }

        let ans = prompt("number, /text to filter, .. up, q quit > ");
        match ans.as_str() {
            "q" => exit(0),
            ".." => {
                if cur != start {
                    cur.pop();
                }
                filt.clear();
            }
            a if a.starts_with('/') => filt = a[1..].to_string(),
            a => match a.parse::<usize>().ok().and_then(|n| entries.get(n.wrapping_sub(1))) {
                Some(p) if p.is_file() => return p.clone(),
                Some(p) => (cur, filt) = (p.clone(), String::new()),
                None => println!("?"),
            },
        }
    }
}

fn name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn main() {
    let root = env::current_dir().unwrap_or_else(|e| die(format!("no current dir: {e}")));
    let nas = env::var_os("NAS_MOUNT").map(PathBuf::from).unwrap_or_else(|| root.join("nas"));

    let (mut dataset, mut target, mut make_vars) = (None, "benchmark".to_string(), Vec::new());
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--target" => target = args.next().unwrap_or_else(|| die("--target needs a value")),
            "-h" | "--help" => {
                println!("usage: pick_video [meva|ilids|picked] [--target \"benchmark ...\"] [MAKE_VAR=value ...]");
                return;
            }
            _ if a.contains('=') => make_vars.push(a),
            _ => dataset = Some(a.to_lowercase()),
        }
    }

    let dir: (&str, &str) = loop {
        let d = dataset.take().unwrap_or_else(|| {
            println!();
            for (i, (_, dir)) in DATASETS.iter().enumerate() {
                println!("  {:3}  {dir}", i + 1);
            }
            prompt("dataset number > ").to_lowercase()
        });
        // Accepts the menu number or the name (DATASET=meva on the command line).
        let pick = d.parse::<usize>().ok().and_then(|n| DATASETS.get(n.wrapping_sub(1)));
        if let Some((key, dir)) = pick.or_else(|| DATASETS.iter().find(|(k, _)| *k == d)) {
            break (*key, *dir);
        }
    };
    // Listing paths are shown relative to `base`.
    let (base, start) = match dir {
        ("picked", _) => (root.clone(), root.join("videos/nas")),
        (_, d) => (nas.clone(), nas.join(d).join("videos")),
    };
    if !start.is_dir() {
        die(match dir.0 {
            "picked" => format!("{} not found - nothing picked yet", start.display()),
            _ => format!("{} not found - run `make mount_nas` first", start.display()),
        });
    }

    // A picked entry is a link into the NAS: follow it so the clip is re-linked
    // to its real file (and a link whose NAS is unmounted fails here, clearly).
    let clip = browse(&root, &base, &start);
    let clip = fs::canonicalize(&clip)
        .unwrap_or_else(|e| die(format!("{}: {e} - run `make mount_nas` first", clip.display())));
    let stream = ffprobe(&root, &clip, "stream=codec_name,profile,width,height");
    let codec = stream.split(',').next().unwrap_or("");
    let vtype = format!(
        "nas/{}",
        video_type(codec).unwrap_or_else(|| die(format!(
            "unsupported codec '{codec}' in {} (benchmarks take h264, hevc, mpeg4)",
            clip.display()
        )))
    );

    let clip_name = name(&clip);
    let link_dir = root.join("videos").join(&vtype);
    fs::create_dir_all(&link_dir).unwrap_or_else(|e| die(format!("{}: {e}", link_dir.display())));
    let link = link_dir.join(&clip_name);
    let _ = fs::remove_file(&link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&clip, &link).unwrap_or_else(|e| die(format!("{}: {e}", link.display())));
    #[cfg(not(unix))]
    die("pick_video needs symlinks (WSL/Linux)");

    let mut cmd = Command::new("make");
    cmd.arg("-C").arg(&root).args(target.split_whitespace());
    cmd.arg(format!("VIDEO_TYPE={vtype}")).arg(format!("VIDEO_NAME={clip_name}")).args(&make_vars);
    let rel = clip.strip_prefix(&nas).unwrap_or(&clip).display();
    println!("\n{rel}  [{stream}, {}] -> {vtype}", describe(&root, &clip));
    println!("$ make -C {} {target} VIDEO_TYPE={vtype} VIDEO_NAME={clip_name} {}\n", root.display(), make_vars.join(" "));
    let status = cmd.status().unwrap_or_else(|e| die(format!("make: {e}")));
    exit(status.code().unwrap_or(1));
}