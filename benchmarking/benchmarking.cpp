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

constexpr int N_METHODS = 6;

struct MethodInfo {
    std::string name;
    std::string exe;
    std::string output_csv;
    int supports_high_profile;
    std::string path;
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

std::array<MethodInfo, N_METHODS> methods = {{
    // {"FFMPEG decode frames", "/extractors/executables/extractor6", "method6_output", 1, "LD_LIBRARY_PATH=/usr/local/lib/"},
    {"FFmpeg MV", "/extractors/executables/extractor0", "method0_output", 1, "LD_LIBRARY_PATH=/usr/local/lib/"},
    {"Same Code Not Patched", "/extractors/executables/extractor1", "method1_output", 1, "LD_LIBRARY_PATH=/usr/local/lib/"},
    {"Optimized MV-Only - FFMPEG Patched", "/extractors/executables/extractor2", "method2_output", 1, "LD_LIBRARY_PATH=/home/ppet/Milestone/motion-vector-extractors-/ffmpeg-8.0/ffmpeg-8.0-ourversion/lib"},
    //{"Custom H.264 Parser", "/extractors/executables/extractor3", "method3_output", 0, "LD_LIBRARY_PATH=/usr/local/lib/"},
    //{"LIVE555 Parser", "/extractors/executables/extractor4", "method4_output", 0, "LD_LIBRARY_PATH=/usr/local/lib/"},
    {"Python mv-extractor", "/extractors/executables/extractor5", "method5_output", 1, "LD_LIBRARY_PATH=/usr/local/lib/"},
    {"FFMPEG Patched - Minimal", "/extractors/executables/extractor7", "method7_output", 1, "LD_LIBRARY_PATH=/home/ppet/Milestone/motion-vector-extractors-/ffmpeg-8.0/ffmpeg-8.0-ourversion/lib"},
    {"FFMPEG Patched!", "/extractors/executables/extractor8", "method8_output", 1, "LD_LIBRARY_PATH=/home/ppet/Milestone/motion-vector-extractors-/ffmpeg-8.0/ffmpeg-8.0-ourversion/lib"}
}};

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
        std::cerr << "Warning: cannot open CSV file '" << fname << "': " << strerror(errno) << std::endl;
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

void print_env_vars(pid_t pid) {
    char path[256];
    snprintf(path, sizeof(path), "/proc/%d/environ", pid);
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        perror("Failed to open environ file");
        return;
    }
    char ch;
    while (file.get(ch)) {
        if (ch == '\0')
            std::cout << '\n';
        else
            std::cout << ch;
    }
}

BenchmarkResult run_benchmark_parallel(const MethodInfo& m, const std::string& input, int par_streams, std::string& absolute_path, std::string& current_dir, std::string& venv_dir) {
    BenchmarkResult r;
    r.name = m.name;
    r.supports_high_profile = m.supports_high_profile;
    std::cout << "Starting " << par_streams << " parallel streams for method: " << m.name << std::endl;
    double t_start = now_ms();

    std::vector<pid_t> pids(par_streams);
    std::vector<int> statuses(par_streams);
    std::vector<struct rusage> usage(par_streams);

    for (int i = 0; i < par_streams; ++i) {
        pid_t pid = fork();
        if (pid < 0) {
            perror("fork failed");
            exit(1);
        } else if (pid == 0) {
            char csv_filename[256];
            snprintf(csv_filename, sizeof(csv_filename), "%s/%s_%d.csv", absolute_path.c_str(), m.output_csv.c_str(), i);

            /*if (!freopen(csv_filename, "w", stdout)) {
                std::cerr << "Child " << i << ": freopen to '" << csv_filename << "' failed: " << strerror(errno) << std::endl;
                exit(1);
            }*/

            // Prepare environment array with one element + null terminator.
            // Need to cast to char* because execle expects char* const envp[].
            char* const envvec[] = { const_cast<char*>(m.path.c_str()), nullptr };

            std::string exe_str = current_dir + m.exe;
            printf("%s\n", exe_str.c_str());
            char* exe = const_cast<char*>(exe_str.c_str()); 
            char* arg0 = exe;
            char* arg1 = const_cast<char*>(input.c_str());

            // Exec with correct sentinels: terminate args with nullptr before env
            execle(exe, arg0, arg1, "1", csv_filename, current_dir.c_str(), venv_dir.c_str(), nullptr, envvec);

            std::cerr << "Child " << i << ": exec failed for command " << m.exe << " " << input << ": " << strerror(errno) << std::endl;
            exit(127);
        } else {
            pids[i] = pid;
            std::cout << "Forked child " << i << " with pid " << pid << std::endl;
            const char* value = getenv("LD_LIBRARY_PATH");
            std::cout << "Env: " << (value ? value : "") << std::endl;
        }
    }
    for (int i = 0; i < par_streams; ++i) {
        if (wait4(pids[i], &statuses[i], 0, &usage[i]) == -1) {
            perror("wait4 failed");
            statuses[i] = -1;
        } else {
            if (WIFEXITED(statuses[i])) {
                std::cout << "Child " << i << " (pid " << pids[i] << ") exited with code " << WEXITSTATUS(statuses[i]) << std::endl;
            } else if (WIFSIGNALED(statuses[i])) {
                std::cout << "Child " << i << " (pid " << pids[i] << ") killed by signal " << WTERMSIG(statuses[i]) << std::endl;
            } else {
                std::cout << "Child " << i << " (pid " << pids[i] << ") ended abnormally" << std::endl;
            }
        }
    }
    double t_end = now_ms();
    std::cout << "All children done; total wall time elapsed: " << (t_end - t_start) << " ms" << std::endl;

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
        std::cout << "Parsed file '" << csv_filename << "': frames=" << frames << ", mvs=" << mvs << std::endl;
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

void print_complete_results(const std::array<BenchmarkResult, N_METHODS>& r, int par_streams) {
    std::cout << "\n======================\n";
    std::cout << "                                   COMPLETE MOTION VECTOR EXTRACTION BENCHMARK\n";
    std::cout << "                              Streams per Method: " << par_streams << "\n";
    std::cout << "======================\n\n";
    std::cout << std::left
              << std::setw(30) << "Method"
              << " | " << std::setw(10) << "Time/Frame"
              << " | " << std::setw(6) << "FPS"
              << " | " << std::setw(9) << "CPU Usage"
              << " | " << std::setw(9) << "Mem Δ KB"
              << " | " << std::setw(10) << "Total MVs"
              << " | " << std::setw(8) << "Frames"
              << " | " << "High Profile\n";
    std::cout << "------------------------------------------------------------------------------------------------------------\n";

    for (const auto& result : r) {
        std::cout << std::left << std::setw(30) << result.name
            << " | " << std::setw(10) << std::fixed << std::setprecision(2) << result.avg_time_per_frame_ms << " ms"
            << " | " << std::setw(6) << std::fixed << std::setprecision(1) << result.throughput_fps
            << " | " << std::setw(8) << std::fixed << std::setprecision(1) << result.cpu_usage_percent << "%"
            << " | " << std::setw(9) << result.memory_peak_kb
            << " | " << std::setw(10) << result.total_motion_vectors
            << " | " << std::setw(8) << result.frame_count
            << " | " << (result.supports_high_profile ? "✅" : "❌")
            << std::endl;
    }
}

int main(int argc, char** argv) {
    if (argc < 2 || argc > 6) {
        std::cerr << "Usage: " << argv[0] << " <video_file_or_rtsp_url> [streams]" << std::endl;
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
    std::array<BenchmarkResult, N_METHODS> results;
    std::cout << "🔍 Starting benchmarking on: " << input << std::endl;
    std::cout << "   Streams per method: " << par_streams << "\n\n";
    for (int i = 0; i < N_METHODS; ++i) {
        std::cout << "▶️  Running: " << methods[i].name << std::endl;
        results[i] = run_benchmark_parallel(methods[i], input, par_streams, absolute_path, current_dir, venv_dir);
        std::cout << "✅ Done: " << results[i].frame_count << " frames, "
                  << results[i].avg_time_per_frame_ms << " ms/frame, "
                  << results[i].throughput_fps << " FPS\n\n";
    }
    print_complete_results(results, par_streams);
    return 0;
}

