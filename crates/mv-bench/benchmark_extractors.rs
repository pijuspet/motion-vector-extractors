use std::ffi::CString;
use std::fs;
use std::io::Read;
use std::os::unix::io::{FromRawFd, RawFd};
use std::process;
use std::time::SystemTime;

use crate::benchmark::BenchmarkResult as SharedBenchmarkResult;

const MAX_STREAMS: i32 = 100;

struct MethodInfo {
    id: i32,
    name: &'static str,
}

const METHODS: &[MethodInfo] = &[
    MethodInfo { id: 0, name: "Original FFmpeg MV only" },
    MethodInfo { id: 1, name: "Original FFmpeg - Flush decoder" },
    MethodInfo { id: 2, name: "Original FFMPEG decode frames" },
    MethodInfo { id: 3, name: "Original FFmpeg - Key frames only" },
    MethodInfo { id: 4, name: "Custom FFmpeg MV-Only - RTSP protocol" },
    MethodInfo { id: 5, name: "Custom FFmpeg - Flush decoder" },
    MethodInfo { id: 6, name: "Custom FFmpeg" },
    MethodInfo { id: 7, name: "Custom FFmpeg - Key frames only" },
];

impl MethodInfo {
    fn exe_path(&self, exe_dir: &str) -> String {
        format!("{}/executables/extractor{}", exe_dir, self.id)
    }

    fn output_csv_prefix(&self) -> String {
        format!("method{}_output", self.id)
    }
}

#[derive(Default)]
struct BenchmarkResult {
    name: String,
    // Time
    // Wall-clock time for all streams to finish
    total_time_ms: f64,
    // Per-stream latency: wall_time / frames_per_stream
    avg_time_per_frame_ms: f64,
    // Aggregate: (1000 / avg_ms) * stream_count
    throughput_fps: f64,

    // CPU
    // Avg CPU per stream  (user+sys time)
    cpu_usage_percent: f64,
    // Total CPU across all streams (useful for system load view)
    cpu_total_percent: f64,

    // Memory
    // Sum of peak RSS across all streams
    memory_total_kb: i64,
    // Peak RSS of the single worst stream

    // Counts
    memory_per_stream_kb: i64,
    cpu_ms_per_frame: f64,
    frame_count: i32,
}

struct ChildProcess {
    pid: libc::pid_t,
    pipe_fd: RawFd,
    usage: libc::rusage,
    self_rss_kb: i64,
}

fn get_timestamp_ms() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn spawn_processes(
    method: &MethodInfo,
    video_file: &str,
    stream_count: i32,
    print_csv: bool,
    output_dir: &str,
    exe_dir: &str,
    is_verbose: bool,
    single_threaded: bool,
) -> Vec<ChildProcess> {
    let mut processes = Vec::with_capacity(stream_count as usize);

    for i in 0..stream_count {
        let mut pipe_fds = [0i32; 2];
        if unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } == -1 {
            eprintln!("Pipe failed");
            process::exit(1);
        }

        let pid = unsafe { libc::fork() };
        if pid < 0 {
            eprintln!("Fork failed");
            process::exit(1);
        }

        if pid == 0 {
            // Child process
            unsafe {
                libc::close(pipe_fds[0]);
                libc::dup2(pipe_fds[1], libc::STDOUT_FILENO);
                libc::close(pipe_fds[1]);
            }

            let csv_path = format!("{}/{}_{}.csv", output_dir, method.output_csv_prefix(), i);
            let exe_path = method.exe_path(exe_dir);
            let print_str = if print_csv { "1" } else { "0" };
            let verbose_str = if i > 0 {
                "0"
            } else {
                if is_verbose {
                    "1"
                } else {
                    "0"
                }
            };
            let single_thr_str = if single_threaded { "1" } else { "0" };

            let c_exe = CString::new(exe_path.as_str()).unwrap();
            let c_video = CString::new(video_file).unwrap();
            let c_print = CString::new(print_str).unwrap();
            let c_csv = CString::new(csv_path.as_str()).unwrap();
            let c_verbose = CString::new(verbose_str).unwrap();
            let c_single = CString::new(single_thr_str).unwrap();

            unsafe {
                libc::execl(
                    c_exe.as_ptr(),
                    c_exe.as_ptr(),
                    c_video.as_ptr(),
                    c_print.as_ptr(),
                    c_csv.as_ptr(),
                    c_verbose.as_ptr(),
                    c_single.as_ptr(),
                    std::ptr::null::<libc::c_char>(),
                );
                // execl only returns on error
                libc::_exit(127);
            }
        }

        // Parent process
        unsafe {
            libc::close(pipe_fds[1]);
        }
        processes.push(ChildProcess {
            pid,
            pipe_fd: pipe_fds[0],
            usage: unsafe { std::mem::zeroed() },
            self_rss_kb: 0,
        });
        if is_verbose {
            println!("Forked child {} with pid {}", i, pid);
        }
    }

    processes
}

fn collect_process_results(
    processes: &mut Vec<ChildProcess>,
    output_dir: &str,
    output_prefix: &str,
    print_csv: bool,
    is_verbose: bool,
) -> (i32, i64) {
    let mut total_frames = 0i32;
    let mut total_mvs = 0i64;

    for (i, proc) in processes.iter_mut().enumerate() {
        let mut status: i32 = 0;

        let wait_result = unsafe { libc::wait4(proc.pid, &mut status, 0, &mut proc.usage) };

        if wait_result == -1 {
            eprintln!("wait4 failed for child {}", i);
            unsafe {
                libc::close(proc.pipe_fd);
            }
            continue;
        }

        if libc::WIFEXITED(status) {
            if is_verbose {
                let exit_code = libc::WEXITSTATUS(status);
                print!(
                    "Child {} (pid {}) exited with code {}",
                    i, proc.pid, exit_code
                );
            }

            let mut buffer = [0u8; 128];
            let mut file = unsafe { std::fs::File::from_raw_fd(proc.pipe_fd) };
            if let Ok(bytes) = file.read(&mut buffer) {
                if bytes > 0 {
                    let text = String::from_utf8_lossy(&buffer[..bytes]);
                    let parts: Vec<&str> = text.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let (Ok(frames), Ok(mvs)) =
                            (parts[0].parse::<i32>(), parts[1].parse::<i64>())
                        {
                            if is_verbose {
                                println!("; {} frames, {} motion vectors", frames, mvs);
                            }
                            total_frames += frames;
                            total_mvs += mvs;

                            if parts.len() >= 3 {
                                if let Ok(rss) = parts[2].parse::<i64>() {
                                    proc.self_rss_kb = rss;
                                }
                            }
                        }
                    }
                }
            }
            // File dropped here closes the fd via from_raw_fd ownership

            // Clean up CSV files from non-primary streams
            if i != 0 && print_csv {
                let csv_path = format!("{}/{}_{}.csv", output_dir, output_prefix, i);
                let _ = fs::remove_file(&csv_path);
            }
        } else if libc::WIFSIGNALED(status) {
            let sig = libc::WTERMSIG(status);
            println!("Child {} (pid {}) killed by signal {}", i, proc.pid, sig);
            unsafe {
                libc::close(proc.pipe_fd);
            }
        } else {
            unsafe {
                libc::close(proc.pipe_fd);
            }
        }
    }

    (total_frames, total_mvs)
}

fn run_benchmark(
    method: &MethodInfo,
    video_file: &str,
    stream_count: i32,
    print_csv: bool,
    output_dir: &str,
    exe_dir: &str,
    is_verbose: bool,
    single_threaded: bool,
) -> BenchmarkResult {
    let mut result = BenchmarkResult::default();
    result.name = method.name.to_string();

    if is_verbose {
        println!(
            "Starting {} parallel streams for: {}",
            stream_count, method.name
        );
    }

    let start_time = get_timestamp_ms();
    let mut processes = spawn_processes(
        method,
        video_file,
        stream_count,
        print_csv,
        output_dir,
        exe_dir,
        is_verbose,
        single_threaded,
    );

    let (total_frames, _total_mvs) = collect_process_results(
        &mut processes,
        output_dir,
        &method.output_csv_prefix(),
        print_csv,
        is_verbose,
    );

    let end_time = get_timestamp_ms();

    if is_verbose {
        println!("Completed in {:.2} ms", end_time - start_time);
    }

    // Calculate metrics
    result.total_time_ms = end_time - start_time;
    result.frame_count = total_frames;

    let frames_per_stream = if stream_count > 0 && total_frames > 0 {
        total_frames / stream_count
    } else {
        0
    };

    result.avg_time_per_frame_ms = if frames_per_stream > 0 {
        result.total_time_ms / frames_per_stream as f64
    } else {
        0.0
    };

    result.throughput_fps = if result.avg_time_per_frame_ms > 0.0 {
        (1000.0 * total_frames as f64) / result.total_time_ms
    } else {
        0.0
    };

    // CPU
    let mut total_cpu_time = 0.0f64;
    let mut total_memory = 0i64;
    let mut peak_memory = 0i64;

    for proc in &processes {
        let user_time =
            proc.usage.ru_utime.tv_sec as f64 * 1000.0 + proc.usage.ru_utime.tv_usec as f64 / 1e3;
        let sys_time =
            proc.usage.ru_stime.tv_sec as f64 * 1000.0 + proc.usage.ru_stime.tv_usec as f64 / 1e3;
        total_cpu_time += user_time + sys_time;

        // Prefer self-reported VmRSS (read after exec, before cleanup)
        // over ru_maxrss which is inflated by the parent's RSS at fork time.
        let rss_kb = if proc.self_rss_kb > 0 {
            proc.self_rss_kb
        } else {
            proc.usage.ru_maxrss
        };
        total_memory += rss_kb;
        peak_memory = peak_memory.max(rss_kb);
    }

    if result.total_time_ms > 0.0 && stream_count > 0 {
        let avg_cpu_per_stream = total_cpu_time / stream_count as f64;
        result.cpu_usage_percent = (avg_cpu_per_stream / result.total_time_ms) * 100.0;

        let num_cores = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
        result.cpu_total_percent = if num_cores > 0 {
            (total_cpu_time / result.total_time_ms) * 100.0 / num_cores as f64
        } else {
            (total_cpu_time / result.total_time_ms) * 100.0
        };
    }

    result.cpu_ms_per_frame = if frames_per_stream > 0 {
        total_cpu_time / stream_count as f64 / frames_per_stream as f64
    } else {
        0.0
    };

    result.memory_total_kb = total_memory;
    result.memory_per_stream_kb = peak_memory;
    result
}

fn print_results(results: &[BenchmarkResult], stream_count: i32) {
    let line = 166;
    let sep = "=".repeat(line);
    let dash = "-".repeat(line);

    let title = "COMPLETE MOTION VECTOR EXTRACTION BENCHMARK";
    let num_cores = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    let streams_title = format!(
        "Streams per Method: {}   |   Logical Cores: {}",
        stream_count, num_cores
    );

    println!("\n{}", sep);
    println!("{:^width$}", title, width = line);
    println!("{:^width$}\n", streams_title, width = line);
    println!("{}\n", sep);

    println!(
        "{:<32} | {:>13} | {:>10} | {:>16} | {:>12} | {:>12} | {:>11} | {:>11} | {:>8}",
        "Method",
        "ms/frame(strm)",
        "Total FPS",
        "CPU ms/frame",
        "CPU %/stream",
        "CPU % total",
        "Mem Total KB",
        "Mem/Strm KB",
        "Frames"
    );
    println!("{}", dash);

    for r in results {
        println!(
            "{:<32} | {:>9.2} ms | {:>8.1}   | {:>11.3} ms | {:>10.1} %  | {:>10.1} %  | {:>11} | {:>11} | {:>8}",
            r.name,
            r.avg_time_per_frame_ms,
            r.throughput_fps,
            r.cpu_ms_per_frame,
            r.cpu_usage_percent,
            r.cpu_total_percent,
            r.memory_total_kb,
            r.memory_per_stream_kb,
            r.frame_count
        );
    }

    println!();
    println!(
        "  ms/frame(strm)  = wall time / (total frames / streams)      — per-stream decode latency"
    );
    println!("  Total FPS       = (1000 / ms_per_frame) * streams            — aggregate system throughput");
    println!("  CPU ms/frame    = avg(user+sys CPU time per process) / frames_per_stream");
    println!("                    — actual CPU work burned per frame; lower = more efficient");
    println!("                    — comparable across stream counts, unlike CPU%");
    println!("  CPU %/stream    = avg CPU time per stream / wall time * 100  — single-stream CPU utilisation");
    println!(
        "  CPU % total     = total CPU time / wall time / cores * 100   — system-wide CPU load"
    );
    println!("{}", dash);
}

pub fn run_benchmark_extractors(
    video_file: &str,
    stream_count: i32,
    output_dir: &str,
    exe_dir: &str,
    single_threaded: bool,
    is_verbose: bool,
    print_csv: bool,
) -> Option<Vec<SharedBenchmarkResult>> {
    if stream_count < 1 || stream_count > MAX_STREAMS {
        eprintln!("Streams must be between 1 and {}", MAX_STREAMS);
        return None;
    }

    if is_verbose {
        println!("Video file       : {}", video_file);
        println!("Streams / method : {}", stream_count);
        println!(
            "Single-threaded  : {}",
            if single_threaded { "yes" } else { "no" }
        );
        println!(
            "Print CSV        : {}\n",
            if print_csv { "yes" } else { "no" }
        );
    }

    let mut results = Vec::with_capacity(METHODS.len());
    for method in METHODS {
        if is_verbose {
            println!("Running: {}", method.name);
        }
        let result = run_benchmark(
            method,
            video_file,
            stream_count,
            print_csv,
            output_dir,
            exe_dir,
            is_verbose,
            single_threaded,
        );
        if is_verbose {
            println!(
                "Done: {} frames, {:.2} ms/frame, {:.1} FPS, {:.1}% CPU/stream\n",
                result.frame_count,
                result.avg_time_per_frame_ms,
                result.throughput_fps,
                result.cpu_usage_percent
            );
        }
        results.push(result);
    }

    print_results(&results, stream_count);

    let mapped: Vec<SharedBenchmarkResult> = results
        .into_iter()
        .map(|r| SharedBenchmarkResult {
            method: r.name,
            streams: stream_count,
            time_per_frame: r.avg_time_per_frame_ms,
            fps: r.throughput_fps,
            cpu_ms_per_frame: r.cpu_ms_per_frame,
            memory: r.memory_total_kb as f64,
            mem_per_stream: r.memory_per_stream_kb as f64,
            frames: r.frame_count,
        })
        .collect();
    Some(mapped)
}
