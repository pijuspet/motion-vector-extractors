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
    int supports_high_profile;
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
    int supports_high_profile = 0;
};

std::vector<MethodInfo> methods = {
    {"Original FFmpeg MV extraction", "/extractors/executables/extractor0", "method0_output", 1}, // Original FFmpeg, takes out motion vectors out of video
    {"Same Code Not Patched", "/extractors/executables/extractor1", "method1_output", 1}, // Original FFmpeg, but custom flags are passed? ask Louise
    {"Custom FFmpeg MV-Only - FFMPEG Patched", "/extractors/executables/extractor2", "method2_output", 1}, // Custom FFmpeg RTSP protocol
    // {"FFMPEG decode frames", "/extractors/executables/extractor3", "method3_output", 1}, // why this one is used? produces no csv
    {"Custom FFmpeg - Flush decoder", "/extractors/executables/extractor4", "method4_output", 1},
    {"Custom FFmpeg", "/extractors/executables/extractor5", "method5_output", 1}
};

double now_ms() {
    struct timeval tv;
    if (gettimeofday(&tv, nullptr) != 0) {
        perror("gettimeofday failed");
        return 0.0;
    }
    return 1000.0 * tv.tv_sec + tv.tv_usec / 1000.0;
}

BenchmarkResult run_benchmark_parallel(const MethodInfo& m, const std::string& video_file, int par_streams, int do_print, std::string& absolute_path, std::string& current_dir) {
    BenchmarkResult r;
    r.name = m.name;
    r.supports_high_profile = m.supports_high_profile;
    printf("Starting %d parallel streams for method: %s\n", par_streams, m.name.c_str());
    double t_start = now_ms();

    std::vector<pid_t> pids(par_streams);
    std::vector<int> statuses(par_streams);
    std::vector<struct rusage> usage(par_streams);
    int pipes_fd[par_streams][2];
    int total_mvs = 0;
    int total_frames = 0;

    for (int i = 0; i < par_streams; ++i) {
        if (pipe(pipes_fd[i]) == -1) {
            perror("Pipe failed");
            exit(1);
        }
        pid_t pid = fork();
        if (pid < 0) {
            perror("Fork failed");
            exit(1);
        }
        else if (pid == 0) {
            close(pipes_fd[i][0]);
            dup2(pipes_fd[i][1], STDOUT_FILENO);
            close(pipes_fd[i][1]);

            char csv_filename[256];
            snprintf(csv_filename, sizeof(csv_filename), "%s/%s_%d.csv", absolute_path.c_str(), m.output_csv.c_str(), i);

            std::string exe_str = current_dir + m.exe;
            char* exe = const_cast<char*>(exe_str.c_str());
            char* video_file_input = const_cast<char*>(video_file.c_str());
            std::string print_to_file = std::to_string(do_print);
            execl(exe, exe, video_file_input, print_to_file.c_str(), csv_filename, nullptr);

            fprintf(stderr, "Child %d: exec failed for command %s %s: %s\n", i, m.exe.c_str(), video_file.c_str(), strerror(errno));
            exit(127);
        }
        else {
            close(pipes_fd[i][1]);
            pids[i] = pid;
            printf("Forked child %d with pid %d\n", i, pid);
        }
    }
    for (int i = 0; i < par_streams; ++i) {
        if (wait4(pids[i], &statuses[i], 0, &usage[i]) == -1) {
            perror("wait4 failed");
            statuses[i] = -1;
        }
        else {
            if (WIFEXITED(statuses[i])) {
                printf("Child %d (pid %d) exited with code %d", i, pids[i], WEXITSTATUS(statuses[i]));

                char buffer[64];
                ssize_t bytes_read = read(pipes_fd[i][0], buffer, sizeof(buffer) - 1);
                if (bytes_read > 0) {
                    buffer[bytes_read] = '\0';
                    int frames = 0, mvs = 0;
                    if (sscanf(buffer, "%d %d", &frames, &mvs) == 2) {
                        printf("; reported %d frames and %d motion vectors\n", frames, mvs);
                        total_frames += frames;
                        total_mvs += mvs;
                    }
                    else {
                        fprintf(stderr, "Warning: failed to parse output from child %d\n", i);
                    }
                }
            }
            else if (WIFSIGNALED(statuses[i])) {
                printf("Child %d (pid %d) killed by signal %d\n", i, pids[i], WTERMSIG(statuses[i]));
            }
            else {
                printf("Child %d (pid %d) ended abnormally\n", i, pids[i]);
            }

            close(pipes_fd[i][0]);

            if (i != 0 && do_print) {
                char csv_filename[256];
                snprintf(csv_filename, sizeof(csv_filename),
                    "%s/%s_%d.csv", absolute_path.c_str(),
                    m.output_csv.c_str(), i);
                if (remove(csv_filename) != 0) {
                    fprintf(stderr, "Warning: failed to remove '%s': %s\n",
                        csv_filename, strerror(errno));
                }
            }
        }
    }
    double t_end = now_ms();
    printf("All children done; total wall time elapsed: %.2f ms\n", t_end - t_start);

    long max_rss_kb = 0;
    double total_user_cpu_sec = 0;
    for (int i = 0; i < par_streams; ++i) {
        if (usage[i].ru_maxrss > max_rss_kb)
            max_rss_kb = usage[i].ru_maxrss;

        double u_sec = usage[i].ru_utime.tv_sec + usage[i].ru_utime.tv_usec / 1e6;
        total_user_cpu_sec += u_sec;
    }

    r.total_time_ms = t_end - t_start;
    r.frame_count = total_frames;
    r.total_motion_vectors = total_mvs;
    r.memory_peak_kb = max_rss_kb;
    r.cpu_usage_percent = (r.total_time_ms > 0) ? (total_user_cpu_sec / (r.total_time_ms / 1000.0)) * 100.0 : 0.0;
    r.avg_time_per_frame_ms = (total_frames > 0) ? (r.total_time_ms / total_frames) : 0;
    r.throughput_fps = (r.avg_time_per_frame_ms > 0) ? 1000.0 / r.avg_time_per_frame_ms : 0;
    return r;
}

void print_complete_results(const std::vector<BenchmarkResult>& r, int par_streams) {
    printf("\n==========================================================================================================\n");
    printf("                                   COMPLETE MOTION VECTOR EXTRACTION BENCHMARK\n");
    printf("                              Streams per Method: %d\n", par_streams);
    printf("==========================================================================================================\n\n");
    printf("%-30s | %-12s | %-6s | %-10s | %-9s | %-12s | %-8s | %s\n",
        "Method", "Time/Frame", "FPS", "CPU Usage", "Mem Δ KB", "Total MVs", "Frames", "High Profile");
    printf("------------------------------------------------------------------------------------------------------------\n");

    for (int i = 0; i < r.size(); i++) {
        printf("%-30s | %10.2f ms | %6.1f | %8.1f%% | %9ld | %10d | %8d | %d\n",
            r[i].name.c_str(), r[i].avg_time_per_frame_ms, r[i].throughput_fps,
            r[i].cpu_usage_percent, r[i].memory_peak_kb,
            r[i].total_motion_vectors, r[i].frame_count,
            r[i].supports_high_profile);
    }
}

int main(int argc, char** argv) {
    if (argc < 2 || argc > 6) {
        fprintf(stderr, "Usage: %s <video_file_or_rtsp_url> [streams]\n", argv[0]);
        return 1;
    }
    std::string video_file = argv[1];
    std::string absolute_path = argv[3];
    std::string current_dir = argv[4];
    int par_streams = 1;
    if (argc >= 3)
        par_streams = std::atoi(argv[2]);

    int do_print = 0;
    if (argc >= 6)
        do_print = std::atoi(argv[5]);

    if (par_streams < 1 || par_streams > 100) {
        fprintf(stderr, "Streams must be between 1 and 100");
        return 1;
    }
    std::vector<BenchmarkResult> results;
    printf("Starting benchmarking on: %s\n", video_file.c_str());
    printf("Streams per method: %d\n\n", par_streams);
    for (int i = 0; i < methods.size(); ++i) {
        printf("Running: %s\n", methods[i].name.c_str());
        results.push_back(run_benchmark_parallel(methods[i], video_file, par_streams, do_print, absolute_path, current_dir));
        printf("Done: %d frames, %.2f ms/frame, %.1f FPS\n\n",
            results[i].frame_count, results[i].avg_time_per_frame_ms, results[i].throughput_fps);
    }
    print_complete_results(results, par_streams);
    return 0;
}