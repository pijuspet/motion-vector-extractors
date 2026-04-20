use std::env;
use std::io::{self, Write};
use std::process;

use mv_bench::full_benchmark::BenchmarkRunner;

// ── CLI ─────────────────────────────────────────────────────────────────────

fn usage() {
    let exe = env::args().next().unwrap_or_else(|| "full_benchmark".to_string());
    println!();
    println!("Usage: {} <input_video_or_rtsp_url> [streams] <video_type> <build_type> <n_runs> [steps...]", exe);
    println!("  Set the input (video filename or RTSP URL) as the first argument.");
    println!("  The number of 'streams' for benchmarking is optional (default = 1).");
    println!("  You will then be prompted to pick which step(s) to run.");
    println!("    1 = Build");
    println!("    2 = Extract (run benchmark)");
    println!("    3 = Generate MV comparison");
    println!("    4 = Generate Plots and PowerPoint");
    println!("    5 = Profiler (VTune on FFmpeg hacked)");
    println!("    6 = Flamegraph (perf or VTune data)");
    println!("    0 = Run ALL steps");
    println!();
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        usage();
        process::exit(1);
    }

    let video_file = &args[1];
    let streams: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let video_type = args.get(3).map(|s| s.as_str()).unwrap_or("h264_cabac");
    let build_type = args.get(4).map(|s| s.as_str()).unwrap_or("cust");
    let n_runs: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(3);

    if streams < 1 {
        eprintln!("Error: streams argument must be a positive integer");
        process::exit(1);
    }

    let runner = BenchmarkRunner::new(video_file, video_type, build_type, streams, n_runs);

    println!();
    println!("Select steps to run (enter one or more numbers separated by space):");
    println!("  1: Build");
    println!("  2: Extract (run benchmark)");
    println!("  3: Generate MV comparison");
    println!("  4: Generate Plots and PowerPoint");
    println!("  5: Profiler (VTune on FFmpeg hacked)");
    println!("  6: Flamegraph (perf or VTune data)");
    println!("  0: Run ALL steps");
    println!();

    let choices: Vec<String> = if args.len() > 6 {
        args[6..].to_vec()
    } else {
        print!("Choice(s): ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap_or(0);
        input.split_whitespace().map(String::from).collect()
    };

    for step in &choices {
        match step.as_str() {
            "1" => { runner.build(); }
            "2" => runner.extract(),
            "3" => runner.generate_mv_comparison(),
            "4" => runner.plot(),
            "5" => runner.profiler(),
            "6" => runner.flamegraph(),
            "0" => {
                runner.run_all();
                break;
            }
            _ => println!("Invalid step: {}", step),
        }
    }
}