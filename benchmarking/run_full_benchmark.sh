#!/bin/bash

set -e

# --- Results Directory Setup ---
CURRENT_DIR="${PWD}"
RESULTS_BASE="${PWD}/results"
mkdir -p "$RESULTS_BASE"
RUN_TIMESTAMP=$(date +"%Y%m%d_%H%M")
RESULTS_DIR="$RESULTS_BASE/$RUN_TIMESTAMP"
mkdir -p "$RESULTS_DIR"

BENCHMARKING_DIR="${CURRENT_DIR}/benchmarking"
SLIDES_CONFIG="${BENCHMARKING_DIR}/slides_config.json"

build() {
  echo "Building all extractors and tools..."
  make all
  (cd benchmarking; g++ -O2 -o executables/benchmark_all_9 benchmarking.cpp \
    `pkg-config --cflags --libs libavformat libavcodec libavutil libswscale` -lm)
 
  echo "Build complete."
}

extract() {
  if [[ -z "$VIDEO_FILE" ]]; then
    echo "Extraction step skipped: set VIDEO_FILE environment variable to input file."
    return 1
  fi
  echo "Running 9-method benchmark suite..."
  (cd benchmarking/executables; ./benchmark_all_9 "$VIDEO_FILE" "$STREAMS" "$RESULTS_DIR" "$CURRENT_DIR" "1")
  find $RESULTS_DIR -type f -name "method*_output_*.csv" ! -name "*_0.csv" -delete
  echo "Benchmarks complete."
}


plot() {
  if [[ -z "$VIDEO_FILE" ]]; then
    echo "Plotting step skipped: set VIDEO_FILE argument."
    return 1
  fi
  if [[ -z "$STREAMS" ]]; then
    echo "Streams count not specified; defaulting to 1."
    STREAMS=1
  fi
  mkdir -p "$RESULTS_DIR/plots"
  echo "Running Python benchmark visualization and PPT generation..."
  (cd benchmarking; python3 benchmark_python.py "$VIDEO_FILE" "$STREAMS" "$BENCHMARKING_DIR/executables" "$CURRENT_DIR" "$RESULTS_DIR" "$SLIDES_CONFIG" "$RESULTS_DIR/plots")
  echo "Plotting complete. Plots and PPTX in $RESULTS_DIR/plots."
}

generate_mv_comparison() {
  (cd utils; python3 mv_compare.py "$RESULTS_DIR/method0_output_0.csv" "$RESULTS_DIR/method6_output_0.csv" 10 100 "$RESULTS_DIR/mv_comparison_result.txt")
  if [[ -f "$RESULTS_DIR/mv_comparison_result.txt" ]]; then
    echo "Motion vector comparison result saved to $RESULTS_DIR/mv_comparison_result.txt."
  else
    echo "Error: mv_comparison_result.txt was not created."
  fi
}

profiler() {
  echo "Running VTune profiler on extractor6 with motion_vectors_only=1..."
	. ~/intel/oneapi/setvars.sh --force
  FFMPEG_LIB="${PWD}/ffmpeg/ffmpeg-8.0-custom/lib"
  export LD_LIBRARY_PATH="$FFMPEG_LIB/libavutil:$FFMPEG_LIB/libavformat:$LD_LIBRARY_PATH"
  vtune_dir="$RESULTS_DIR/vtune_results"
  mkdir -p "$vtune_dir"

  (cd ./extractors/executables; vtune -collect hotspots -result-dir "$vtune_dir" -- ./extractor6 "$VIDEO_FILE" 0 "$RESULTS_DIR/method6_output_vtune.csv")

  vtune -report hotspots -result-dir "$vtune_dir" -format csv -report-output "$vtune_dir/hotspots.csv" 
  vtune -report top-down -result-dir "$vtune_dir" -format csv -report-output "$vtune_dir/topdown.csv"

  # Generate call tree and hotspots bar chart from VTune topdown CSV
  (cd utils; python3 vtune_hotspots_plot.py "$vtune_dir/topdown.csv")

  echo "Profiler run complete. Results in $vtune_dir."
}

run_all() {
  build
  extract
  generate_mv_comparison
  plot
  profiler
}

usage() {
  echo
  echo "Usage: $0 <input_video_or_rtsp_url> [streams]"
  echo "  Set the input (video filename or RTSP URL) as the first argument."
  echo "  The number of 'streams' for benchmarking is optional (default = 1)."
  echo "  You will then be prompted to pick which step(s) to run."
  echo "    1 = Build"
  echo "    2 = Extract (run benchmark)"
  echo "    3 = Generate Plots and PowerPoint"
  echo "    4 = Generate MV comparison"
  echo "    5 = Profiler (VTune on FFmpeg hacked)"
  echo "    0 = Run ALL steps in sequence"
  echo
}

# --- Main ---

if [[ -z "$1" ]]; then
  usage
  exit 1
fi

VIDEO_FILE="$1"
STREAMS="${2:-1}"    # Defaults to 1 if second argument missing

# Optional validation of streams argument:
if ! [[ "$STREAMS" =~ ^[0-9]+$ && "$STREAMS" -ge 1 ]]; then
  echo "Error: streams argument must be a positive integer"
  exit 1
fi

echo
echo "Select steps to run (enter one or more numbers separated by space):"
echo "  1: Build"
echo "  2: Extract (run benchmark)"
echo "  3: Generate Plots and PowerPoint"
echo "  4: Generate MV comparison"
echo "  5: Profiler (VTune on FFmpeg hacked)"
echo "  0: Run ALL steps"
echo
read -p "Choice(s): " CHOICES

for step in $CHOICES; do
  case "$step" in
    1) build ;;
    2) extract ;;
    3) plot ;;
    4) generate_mv_comparison ;;
    5) profiler ;;
    0) run_all; break ;;
    *) echo "Invalid step: $step" ;;
  esac
done