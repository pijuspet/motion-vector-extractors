#include <iostream>
#include <fstream>
#include <sstream>
#include <vector>
#include <string>
#include <array>
#include <memory>
#include <cstring>
#include <cstdlib>
#include <cstdio>
#include <cerrno>
#include <iomanip>
#include <unistd.h>
#include <sys/time.h>
#include <sys/resource.h>
#include <sys/wait.h>

static constexpr int CSV_PATH_MAX = 512;
static constexpr int PIPE_BUF_SIZE = 128;
static constexpr int MAX_STREAMS = 100;

struct MethodInfo {
    std::string name;
    std::string exe;
    std::string output_csv;

    MethodInfo(int id, const std::string& name)
        : name(name)
        , exe("/extractors/executables/extractor" + std::to_string(id))
        , output_csv("method" + std::to_string(id) + "_output")
    {}
};

struct BenchmarkResult {
    std::string name;
    // Time
    // Wall-clock time for all streams to finish
    double total_time_ms = 0;
    // Per-stream latency: wall_time / frames_per_stream
    double avg_time_per_frame_ms = 0;
    // Aggregate: (1000 / avg_ms) * stream_count
    double throughput_fps = 0;

    // CPU
    // Avg CPU per stream  (user+sys time)
    double cpu_usage_percent = 0;
    // Total CPU across all streams (useful for system load view)
    double cpu_total_percent = 0;

    // Memory
    // Sum of peak RSS across all streams
    long memory_total_kb = 0;
    // Peak RSS of the single worst stream
    long memory_per_stream_kb = 0;

    // Counts
    int total_motion_vectors = 0;
    int frame_count = 0;
    int stream_count = 0;

    double cpu_ms_per_frame = 0;
};

std::vector<MethodInfo> methods = {
    {0, "Original FFmpeg MV only"},
    {1, "Original FFmpeg - Flush decoder"}, // for loop was faster, check
    {2, "Original FFMPEG decode frames"}, // To compare with no mv vs with extraction of mv, not changed
    {3, "Custom FFmpeg MV-Only - RTSP protocol"}, // with rtsp should be faster
    {4, "Custom FFmpeg - Flush decoder"},
    {5, "Custom FFmpeg"}
};

double get_timestamp_ms() {
    struct timeval tv;
    if (gettimeofday(&tv, nullptr) != 0) {
        perror("gettimeofday failed");
        return 0.0;
    }
    return 1000.0 * tv.tv_sec + tv.tv_usec / 1000.0;
}

struct ChildProcess {
    pid_t pid = -1;
    int pipe_fd = -1;
    int status = 0;
    struct rusage usage = {};
};


std::vector<ChildProcess> spawn_processes(
    const MethodInfo& method,
    const std::string& video_file,
    int stream_count,
    bool print_csv,
    const std::string& output_dir,
    const std::string& exe_dir,
    bool is_verbose,
    bool single_threaded
) {
    std::vector<ChildProcess> processes(stream_count);

    for (int i = 0; i < stream_count; ++i) {
        int pipe_fds[2];
        if (pipe(pipe_fds) == -1) {
            perror("Pipe failed");
            exit(1);
        }

        pid_t pid = fork();
        if (pid < 0) {
            perror("Fork failed");
            exit(1);
        }

        if (pid == 0) {
            close(pipe_fds[0]);
            dup2(pipe_fds[1], STDOUT_FILENO);
            close(pipe_fds[1]);

            char csv_path[CSV_PATH_MAX];
            snprintf(csv_path, sizeof(csv_path), "%s/%s_%d.csv",
                output_dir.c_str(), method.output_csv.c_str(), i);

            std::string exe_path = exe_dir + method.exe;
            std::string print_to_file = std::to_string(print_csv);
            std::string verbose = std::to_string(i > 0 ? 0 : is_verbose); // print only 1st stream
            std::string is_single_thr = std::to_string(single_threaded);

            execl(exe_path.c_str(), exe_path.c_str(),
                video_file.c_str(),
                print_to_file.c_str(),
                csv_path,
                verbose.c_str(),
                is_single_thr.c_str(),
                nullptr);

            fprintf(stderr, "Child %d: execl failed: %s\n", i, strerror(errno));
            exit(127);
        }

        close(pipe_fds[1]);
        processes[i].pid = pid;
        processes[i].pipe_fd = pipe_fds[0];
        if (is_verbose)
            printf("Forked child %d with pid %d\n", i, pid);
    }

    return processes;
}

void collect_process_results(
    std::vector<ChildProcess>& processes,
    int& total_frames,
    int& total_mvs,
    const std::string& output_dir,
    const std::string& output_prefix,
    bool print_csv,
    bool is_verbose
) {
    for (size_t i = 0; i < processes.size(); ++i) {
        auto& proc = processes[i];

        if (wait4(proc.pid, &proc.status, 0, &proc.usage) == -1) {
            perror("wait4 failed");
            close(proc.pipe_fd);
            continue;
        }

        if (WIFEXITED(proc.status)) {
            if (is_verbose)
                printf("Child %zu (pid %d) exited with code %d",
                    i, proc.pid, WEXITSTATUS(proc.status));

            char buffer[PIPE_BUF_SIZE];
            ssize_t bytes = read(proc.pipe_fd, buffer, sizeof(buffer) - 1);
            if (bytes > 0) {
                buffer[bytes] = '\0';
                int frames = 0, mvs = 0;
                if (sscanf(buffer, "%d %d", &frames, &mvs) == 2) {
                    if (is_verbose)
                        printf("; %d frames, %d motion vectors\n", frames, mvs);
                    total_frames += frames;
                    total_mvs += mvs;
                }
                else {
                    fprintf(stderr, "Warning: failed to parse output from child %zu\n", i);
                }
            }
        }
        else if (WIFSIGNALED(proc.status)) {
            printf("Child %zu (pid %d) killed by signal %d\n",
                i, proc.pid, WTERMSIG(proc.status));
        }

        close(proc.pipe_fd);

        // Clean up CSV files from non-primary streams
        if (i != 0 && print_csv) {
            char csv_path[CSV_PATH_MAX];
            snprintf(csv_path, sizeof(csv_path), "%s/%s_%zu.csv",
                output_dir.c_str(), output_prefix.c_str(), i);
            if (remove(csv_path) != 0)
                fprintf(stderr, "Warning: failed to remove '%s': %s\n",
                    csv_path, strerror(errno));
        }
    }
}

BenchmarkResult run_benchmark(
    const MethodInfo& method,
    const std::string& video_file,
    int stream_count,
    bool print_csv,
    const std::string& output_dir,
    const std::string& exe_dir,
    bool is_verbose,
    bool single_threaded
) {
    BenchmarkResult result;
    result.name = method.name;
    result.stream_count = stream_count;

    if (is_verbose)
        printf("Starting %d parallel streams for: %s\n", stream_count, method.name.c_str());

    double start_time = get_timestamp_ms();
    auto processes = spawn_processes(method, video_file, stream_count, print_csv, output_dir, exe_dir, is_verbose, single_threaded);

    int total_frames = 0, total_mvs = 0;
    collect_process_results(processes, total_frames, total_mvs, output_dir, method.output_csv, print_csv, is_verbose);

    double end_time = get_timestamp_ms();

    if (is_verbose)
        printf("Completed in %.2f ms\n", end_time - start_time);

    // Calculate metrics
    result.total_time_ms = end_time - start_time;
    result.frame_count = total_frames;
    result.total_motion_vectors = total_mvs;

    // Per-stream latency: wall time divided by frames in a *single* stream.
    // FIX: previously divided by total_frames (sum across all N streams),
    // making it look N× faster when running more streams — not a real speedup.
    int frames_per_stream = (stream_count > 0 && total_frames > 0)
        ? total_frames / stream_count
        : 0;

    result.avg_time_per_frame_ms = (frames_per_stream > 0)
        ? result.total_time_ms / static_cast<double>(frames_per_stream)
        : 0.0;

    // Aggregate throughput = how many frames/sec the whole system delivers
    result.throughput_fps = (result.avg_time_per_frame_ms > 0)
        ? (1000.0 * total_frames) / result.total_time_ms
        : 0.0;

    // ── CPU ─────────────────────────────────────────────────────────────────
    double total_cpu_time = 0.0;
    long   total_memory = 0;
    long   peak_memory = 0;

    for (const auto& proc : processes) {
        double user_time = proc.usage.ru_utime.tv_sec * 1000.0
            + proc.usage.ru_utime.tv_usec / 1e3;
        double sys_time = proc.usage.ru_stime.tv_sec * 1000.0
            + proc.usage.ru_stime.tv_usec / 1e3;
        total_cpu_time += user_time + sys_time;

        long rss_kb = proc.usage.ru_maxrss;
        total_memory += rss_kb;
        peak_memory = std::max(peak_memory, rss_kb);
    }

    if (result.total_time_ms > 0 && stream_count > 0) {
        // Average CPU utilisation per stream (comparable across stream counts)
        double avg_cpu_per_stream = total_cpu_time / stream_count;
        result.cpu_usage_percent = (avg_cpu_per_stream / result.total_time_ms) * 100.0;

        // Total CPU load across all streams as % of all available logical cores
        long num_cores = sysconf(_SC_NPROCESSORS_ONLN);
        result.cpu_total_percent = (num_cores > 0)
            ? (total_cpu_time / result.total_time_ms) * 100.0 / static_cast<double>(num_cores)
            : (total_cpu_time / result.total_time_ms) * 100.0;
    }
    result.cpu_ms_per_frame = (frames_per_stream > 0)
        ? (total_cpu_time / stream_count / frames_per_stream)
        : 0.0;

    // ── Memory ──────────────────────────────────────────────────────────────
    result.memory_total_kb = total_memory;
    result.memory_per_stream_kb = peak_memory;
    return result;
}

void print_results(const std::vector<BenchmarkResult>& results, int stream_count) {
    int LINE = 166;

    std::string title = "COMPLETE MOTION VECTOR EXTRACTION BENCHMARK";
    char streams_title[64];
    long num_cores = sysconf(_SC_NPROCESSORS_ONLN);
    snprintf(streams_title, sizeof(streams_title),
        "Streams per Method: %d   |   Logical Cores: %ld", stream_count, num_cores);

    printf("\n%s\n", std::string(LINE, '=').c_str());
    printf("%*s%s\n", (int)(LINE - title.length()) / 2, "", title.c_str());
    printf("%*s%s\n\n", (int)(LINE - strlen(streams_title)) / 2, "", streams_title);
    printf("%s\n\n", std::string(LINE, '=').c_str());

    printf("%-32s | %13s | %10s | %16s | %12s | %12s | %11s | %11s | %8s\n",
        "Method",
        "ms/frame(strm)",
        "Total FPS",
        "CPU ms/frame",
        "CPU %/stream",
        "CPU % total",
        "Mem Total KB",
        "Mem/Strm KB",
        "Frames");
    printf("%s\n", std::string(LINE, '-').c_str());

    for (const auto& r : results) {
        printf("%-32s | %11.2f ms | %8.1f   | %13.3f ms | %10.1f %%  | %10.1f %%  | %11ld | %11ld | %8d\n",
            r.name.c_str(),
            r.avg_time_per_frame_ms,
            r.throughput_fps,
            r.cpu_ms_per_frame,
            r.cpu_usage_percent,
            r.cpu_total_percent,
            r.memory_total_kb,
            r.memory_per_stream_kb,
            r.frame_count);
    }

    printf("\n");
    printf("  ms/frame(strm)  = wall time / (total frames / streams)      — per-stream decode latency\n");
    printf("  Total FPS       = (1000 / ms_per_frame) * streams            — aggregate system throughput\n");
    printf("  CPU ms/frame    = avg(user+sys CPU time per process) / frames_per_stream\n");
    printf("                    — actual CPU work burned per frame; lower = more efficient\n");
    printf("                    — comparable across stream counts, unlike CPU%%\n");
    printf("  CPU %%/stream    = avg CPU time per stream / wall time * 100  — single-stream CPU utilisation\n");
    printf("  CPU %% total     = total CPU time / wall time / cores * 100   — system-wide CPU load\n");
    printf("%s\n", std::string(LINE, '-').c_str());
}

int main(int argc, char** argv) {
    if (argc < 7) {
        fprintf(stderr,
            "Usage: %s <video_file> <streams> <output_dir> <exe_dir> "
            "<single_threaded> <verbose> [print_csv]\n", argv[0]);
        return 1;
    }

    std::string video_file = argv[1];
    int stream_count = std::atoi(argv[2]);
    std::string output_dir = argv[3];
    std::string exe_dir = argv[4];
    bool single_thr = std::atoi(argv[5]);
    bool is_verbose = std::atoi(argv[6]);
    bool print_csv = (argc >= 8) ? std::atoi(argv[7]) : false;

    if (stream_count < 1 || stream_count > MAX_STREAMS) {
        fprintf(stderr, "Streams must be between 1 and %d\n", MAX_STREAMS);
        return 1;
    }

    if (is_verbose) {
        printf("Video file       : %s\n", video_file.c_str());
        printf("Streams / method : %d\n", stream_count);
        printf("Single-threaded  : %s\n", single_thr ? "yes" : "no");
        printf("Print CSV        : %s\n\n", print_csv ? "yes" : "no");
    }

    std::vector<BenchmarkResult> results;
    results.reserve(methods.size());
    for (const auto& method : methods) {
        if (is_verbose)
            printf("Running: %s\n", method.name.c_str());
        auto result = run_benchmark(method, video_file, stream_count, print_csv, output_dir, exe_dir, is_verbose, single_thr);
        results.push_back(result);
        if (is_verbose)
            printf("Done: %d frames, %.2f ms/frame, %.1f FPS, %.1f%% CPU/stream\n\n",
                result.frame_count, result.avg_time_per_frame_ms, result.throughput_fps, result.cpu_usage_percent);
    }

    print_results(results, stream_count);
    return 0;
}