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

struct MethodInfo {
    std::string name;
    std::string exe;
    std::string output_csv;
    int index;

    MethodInfo(int id, const std::string& name)
        : name(name)
        , exe("/extractors/executables/extractor" + std::to_string(id))
        , output_csv("method" + std::to_string(id) + "_output")
        , index(id)
    {}
};

struct BenchmarkResult {
    std::string name;
    double total_time_ms = 0;
    double avg_time_per_frame_ms = 0;
    double throughput_fps = 0;
    double cpu_usage_percent = 0;
    long memory_peak_kb = 0;
    int total_motion_vectors = 0;
    int frame_count = 0;
};

std::vector<MethodInfo> methods = {
    {0, "Original FFmpeg MV extraction"}, // Original FFmpeg, takes out motion vectors out of video
    {1, "Same Code Not Patched"}, // Original FFmpeg, but custom flags are passed? ask Louise
    {2, "Custom FFmpeg MV-Only - FFMPEG Patched"}, // Custom FFmpeg RTSP protocol
    // {3, "FFMPEG decode frames"}, // why this one is used? produces no csv
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
    pid_t pid;
    int pipe_fd;
    int status = 0;
    struct rusage usage = {};
};


std::vector<ChildProcess> spawn_processes(
    const MethodInfo& method,
    const std::string& video_file,
    int stream_count,
    bool print_csv,
    const std::string& output_dir,
    const std::string& exe_dir
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

            char csv_path[256];
            snprintf(csv_path, sizeof(csv_path), "%s/%s_%d.csv",
                output_dir.c_str(), method.output_csv.c_str(), i);

            std::string exe_path = exe_dir + method.exe;
            char* exe = const_cast<char*>(exe_path.c_str());
            char* video_file_input = const_cast<char*>(video_file.c_str());
            std::string print_to_file = std::to_string(print_csv);
            std::string extractor_index = std::to_string(method.index);
            execl(exe, exe, video_file_input, print_to_file.c_str(), csv_path, extractor_index.c_str(), nullptr);

            fprintf(stderr, "Child %d: exec failed: %s\n", i, strerror(errno));
            exit(127);
        }

        close(pipe_fds[1]);
        processes[i].pid = pid;
        processes[i].pipe_fd = pipe_fds[0];
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
    bool print_csv
) {
    for (size_t i = 0; i < processes.size(); ++i) {
        auto& proc = processes[i];

        if (wait4(proc.pid, &proc.status, 0, &proc.usage) == -1) {
            perror("wait4 failed");
            continue;
        }

        if (WIFEXITED(proc.status)) {
            printf("Child %zu (pid %d) exited with code %d",
                i, proc.pid, WEXITSTATUS(proc.status));

            char buffer[64];
            ssize_t bytes = read(proc.pipe_fd, buffer, sizeof(buffer) - 1);
            if (bytes > 0) {
                buffer[bytes] = '\0';
                int frames = 0, mvs = 0;
                if (sscanf(buffer, "%d %d", &frames, &mvs) == 2) {
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
            char csv_path[256];
            snprintf(csv_path, sizeof(csv_path), "%s/%s_%zu.csv",
                output_dir.c_str(), output_prefix.c_str(), i);
            if (remove(csv_path) != 0) {
                fprintf(stderr, "Warning: failed to remove '%s': %s\n",
                    csv_path, strerror(errno));
            }
        }
    }
}

BenchmarkResult run_benchmark(
    const MethodInfo& method,
    const std::string& video_file,
    int stream_count,
    bool print_csv,
    const std::string& output_dir,
    const std::string& exe_dir
) {
    BenchmarkResult result;
    result.name = method.name;

    printf("Starting %d parallel streams for: %s\n", stream_count, method.name.c_str());

    double start_time = get_timestamp_ms();
    auto processes = spawn_processes(method, video_file, stream_count, print_csv, output_dir, exe_dir);

    int total_frames = 0, total_mvs = 0;
    collect_process_results(processes, total_frames, total_mvs, output_dir, method.output_csv, print_csv);

    double end_time = get_timestamp_ms();
    printf("Completed in %.2f ms\n", end_time - start_time);

    // Calculate metrics
    result.total_time_ms = end_time - start_time;
    result.frame_count = total_frames;
    result.total_motion_vectors = total_mvs;

    long max_memory = 0;
    double total_cpu_time = 0;
    for (const auto& proc : processes) {
        max_memory = std::max(max_memory, proc.usage.ru_maxrss);
        total_cpu_time += proc.usage.ru_utime.tv_sec + proc.usage.ru_utime.tv_usec / 1e6;
    }

    result.memory_peak_kb = max_memory;
    result.cpu_usage_percent = (result.total_time_ms > 0)
        ? (total_cpu_time / (result.total_time_ms / 1000.0)) * 100.0
        : 0.0;
    result.avg_time_per_frame_ms = (total_frames > 0)
        ? result.total_time_ms / total_frames
        : 0;
    result.throughput_fps = (result.avg_time_per_frame_ms > 0)
        ? 1000.0 / result.avg_time_per_frame_ms
        : 0;

    return result;
}

void print_results(const std::vector<BenchmarkResult>& results, int stream_count) {
    int line_size = 104;
    std::string title = "COMPLETE MOTION VECTOR EXTRACTION BENCHMARK";
    int title_offset = (line_size - title.length()) / 2;
    char streams_title[32];
    snprintf(streams_title, sizeof(streams_title), "Streams per Method: %d\n", stream_count);
    int streams_title_offset = (line_size - 32) / 2;
    printf("\n%s\n", std::string(line_size, '=').c_str());
    printf("%*s%s\n", title_offset, "", title.c_str());
    printf("%*s%s\n", streams_title_offset, "", streams_title);
    printf("%s\n\n", std::string(line_size, '=').c_str());

    printf("%-30s | %12s | %6s | %10s | %9s | %12s | %8s\n",
        "Method", "Time/Frame", "FPS", "CPU Usage", "Mem KB", "Total MVs", "Frames");
    printf("%s\n", std::string(line_size, '-').c_str());

    for (const auto& r : results) {
        printf("%-30s | %10.2f ms | %6.1f | %8.1f%% | %9ld | %12d | %8d\n",
            r.name.c_str(), r.avg_time_per_frame_ms, r.throughput_fps,
            r.cpu_usage_percent, r.memory_peak_kb,
            r.total_motion_vectors, r.frame_count);
    }
}

int main(int argc, char** argv) {
    if (argc < 5) {
        fprintf(stderr, "Usage: %s <video_file> <streams> <output_dir> <exe_dir> [print_csv]\n", argv[0]);
        return 1;
    }

    std::string video_file = argv[1];
    int stream_count = std::atoi(argv[2]);
    std::string output_dir = argv[3];
    std::string exe_dir = argv[4];
    bool print_csv = (argc >= 6) ? std::atoi(argv[5]) : false;

    if (stream_count < 1 || stream_count > 100) {
        fprintf(stderr, "Streams must be between 1 and 100\n");
        return 1;
    }

    printf("Starting benchmark on: %s\n", video_file.c_str());
    printf("Streams per method: %d\n\n", stream_count);

    std::vector<BenchmarkResult> results;
    for (const auto& method : methods) {
        printf("Running: %s\n", method.name.c_str());
        auto result = run_benchmark(method, video_file, stream_count, print_csv, output_dir, exe_dir);
        results.push_back(result);
        printf("Done: %d frames, %.2f ms/frame, %.1f FPS\n\n",
            result.frame_count, result.avg_time_per_frame_ms, result.throughput_fps);
    }

    print_results(results, stream_count);
    return 0;
}