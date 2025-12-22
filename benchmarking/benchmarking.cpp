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
        {"FFMPEG decode frames", "/extractors/executables/extractor6", "method6_output", 1},
        {"FFmpeg MV", "/extractors/executables/extractor0", "method0_output", 1},
        {"Same Code Not Patched", "/extractors/executables/extractor1", "method1_output", 1},
        {"Optimized MV-Only - FFMPEG Patched", "/extractors/executables/extractor2", "method2_output", 1},
        {"Custom H.264 Parser", "/extractors/executables/extractor3", "method3_output", 0},
        {"LIVE555 Parser", "/extractors/executables/extractor4", "method4_output", 0},
        {"Python mv-extractor", "/extractors/executables/extractor5", "method5_output", 1},
        {"FFMPEG Patched - Minimal", "/extractors/executables/extractor7", "method7_output", 1},
        {"FFMPEG Patched!", "/extractors/executables/extractor8", "method8_output", 1}
};

double now_ms() {
    struct timeval tv;
    if (gettimeofday(&tv, nullptr) != 0) {
        perror("gettimeofday failed");
        return 0.0;
    }
    return 1000.0 * tv.tv_sec + tv.tv_usec / 1000.0;
}

void parse_csv(const std::string& fname, int* frames, int* mvs) {
    std::ifstream file(fname);
    if (!file) {
        fprintf(stderr, "Warning: cannot open CSV file '%s': %s\n", fname.c_str(), strerror(errno));
        *frames = 0;
        *mvs = 0;
        return;
    }
    std::string line;
    int last = -1;
    *frames = 0;
    *mvs = 0;
    if (!std::getline(file, line))
        return;
    while (std::getline(file, line)) {
        std::istringstream iss(line);
        int frame = -1;
        if (iss >> frame) {
            (*mvs)++;
            if (frame != last) {
                (*frames)++;
                last = frame;
            }
        }
    }
}

BenchmarkResult run_benchmark_parallel(const MethodInfo& m, const std::string& input, int par_streams, std::string& absolute_path, std::string& current_dir, std::string& venv_dir) {
    BenchmarkResult r;
    r.name = m.name;
    r.supports_high_profile = m.supports_high_profile;
    printf("Starting %d parallel streams for method: %s\n", par_streams, m.name.c_str());
    double t_start = now_ms();

    std::vector<pid_t> pids(par_streams);
    std::vector<int> statuses(par_streams);
    std::vector<struct rusage> usage(par_streams);

    for (int i = 0; i < par_streams; ++i) {
        pid_t pid = fork();
        if (pid < 0) {
            perror("fork failed");
            exit(1);
        }
        else if (pid == 0) {
            char csv_filename[256];
            snprintf(csv_filename, sizeof(csv_filename), "%s/%s_%d.csv", absolute_path.c_str(), m.output_csv.c_str(), i);

            std::string exe_str = current_dir + m.exe;
            char* exe = const_cast<char*>(exe_str.c_str());
            char* arg1 = const_cast<char*>(input.c_str());

            execl(exe, exe, arg1, "1", csv_filename, current_dir.c_str(), venv_dir.c_str(), nullptr);

            fprintf(stderr, "Child %d: exec failed for command %s %s: %s\n", i, m.exe.c_str(), input.c_str(), strerror(errno));
            exit(127);
        }
        else {
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
                printf("Child %d (pid %d) exited with code %d\n", i, pids[i], WEXITSTATUS(statuses[i]));
            } else if (WIFSIGNALED(statuses[i])) {
                printf("Child %d (pid %d) killed by signal %d\n", i, pids[i], WTERMSIG(statuses[i]));
            } else {
                printf("Child %d (pid %d) ended abnormally\n", i, pids[i]);
            }
        }
    }
    double t_end = now_ms();
    printf("All children done; total wall time elapsed: %.2f ms\n", t_end - t_start);

    long max_rss_kb = 0;
    double total_user_cpu_sec = 0;
    int total_mvs = 0;
    for (int i = 0; i < par_streams; ++i) {
        if (usage[i].ru_maxrss > max_rss_kb) {
            max_rss_kb = usage[i].ru_maxrss;
        }
        double u_sec = usage[i].ru_utime.tv_sec + usage[i].ru_utime.tv_usec / 1e6;
        total_user_cpu_sec += u_sec;
        char csv_filename[256];
        snprintf(csv_filename, sizeof(csv_filename), "%s/%s_%d.csv", absolute_path.c_str(), m.output_csv.c_str(), i);
        int frames = 0, mvs = 0;
        parse_csv(csv_filename, &frames, &mvs);
        printf("Parsed file '%s': frames=%d, mvs=%d\n", csv_filename, frames, mvs);
        total_mvs += mvs;
    }
    // Fixed frames per stream as requested
    int fixed_frames_per_stream = 298;
    int total_frames = fixed_frames_per_stream * par_streams;
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
        printf("%-30s | %10.2f ms | %6.1f | %8.1f%% | %9ld | %10d | %8d | %s\n",
               r[i].name.c_str(), r[i].avg_time_per_frame_ms, r[i].throughput_fps,
               r[i].cpu_usage_percent, r[i].memory_peak_kb,
               r[i].total_motion_vectors, r[i].frame_count,
               r[i].supports_high_profile ? "✅" : "❌");
    }
}

int main(int argc, char** argv) {
     if (argc < 2 || argc > 6) {
        fprintf(stderr, "Usage: %s <video_file_or_rtsp_url> [streams]\n", argv[0]);
        return 1;
    }
    std::string input = argv[1];
    std::string absolute_path = argv[3];
    std::string current_dir = argv[4];
    std::string venv_dir = argv[5];
    int par_streams = 1;
    if (argc == 3)
        par_streams = std::atoi(argv[2]);
    if (par_streams < 1 || par_streams > 100) {
        std::cerr << "Streams must be between 1 and 100." << std::endl;
        return 1;
    }
    std::vector<BenchmarkResult> results;
    printf("🔍 Starting benchmarking on: %s\n", input.c_str());
    printf("   Streams per method: %d\n\n", par_streams);
    for (int i = 0; i < methods.size(); ++i) {
        printf("▶️  Running: %s\n", methods[i].name.c_str());
        results.push_back(run_benchmark_parallel(methods[i], input, par_streams, absolute_path, current_dir, venv_dir));
        printf("✅ Done: %d frames, %.2f ms/frame, %.1f FPS\n\n",
               results[i].frame_count, results[i].avg_time_per_frame_ms, results[i].throughput_fps);
    }
    print_complete_results(results, par_streams);
    return 0;
}

