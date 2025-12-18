#!/bin/bash

set -e

# --- Results Directory Setup ---
CURRENT_DIR="${PWD}"
RESULTS_BASE="${PWD}/results"
mkdir -p "$RESULTS_BASE"
RUN_TIMESTAMP=$(date +"%Y%m%d_%H%M")
RESULTS_DIR="$RESULTS_BASE/$RUN_TIMESTAMP"
mkdir -p "$RESULTS_DIR"
export RESULTS_DIR

# VIDEO_DIR="${PWD}"
# BENCHMARKING_DIR="${PWD}/benchmarking"
# BENCHMARKING_EXECUTABLES="$BENCHMARKING_DIR/executables"

build() {
  echo "Building all extractors and tools..."
  make all
  (cd benchmarking; g++ -O2 -o executables/benchmark_all_9 benchmarking.cpp \
    `pkg-config --cflags --libs libavformat libavcodec libavutil libswscale` -lm)
  (cd utils; g++ -O2 -o executables/combine_csv combine_csv.cpp)
 
  echo "Build complete."
}

extract() {
  if [[ -z "$INPUT" ]]; then
    echo "Extraction step skipped: set INPUT environment variable to input file."
    return 1
  fi
  echo "Running 9-method benchmark suite..."
  (cd benchmarking/executables; ./benchmark_all_9 "$INPUT" "$INPUT1" "$RESULTS_DIR")
  echo "Benchmarks complete."
}


combine() {
  echo "Combining all CSV outputs..."
  (cd utils/executables ; ./combine_csv $RESULTS_DIR)
  echo "Combined motion vectors saved to all_motion_vectors.csv."
}

decode() {
  if [ ! -f "$RESULTS_DIR/decoded_output.mp4" ]; then
    echo "Creating decoded_output.mp4 reference video using ffmpeg..."
    ffmpeg -y -i "$INPUT" -c copy -an "$RESULTS_DIR/decoded_output.mp4"
  fi
  # Move combined_motion_vectors_with_video.mp4 if it exists
  if [ -f combined_motion_vectors_with_video.mp4 ]; then
    mv combined_motion_vectors_with_video.mp4 "$RESULTS_DIR/combined_motion_vectors_with_video.mp4"
    echo "Combined motion vector and video output saved as $RESULTS_DIR/combined_motion_vectors_with_video.mp4."
  fi
}

plot() {
  if [[ -z "$INPUT" ]]; then
    echo "Plotting step skipped: set INPUT argument."
    return 1
  fi
  if [[ -z "$INPUT1" ]]; then
    echo "Streams count not specified; defaulting to 1."
    INPUT1=1
  fi
  mkdir -p "$RESULTS_DIR/plots"
  echo "Running Python benchmark visualization and PPT generation..."
  (cd benchmarking; python3 benchmark_python.py "$INPUT" "$INPUT1" "$CURRENT_DIR" "$RESULTS_DIR" "$RESULTS_DIR/plots")
  echo "Plotting complete. Plots and PPTX in $RESULTS_DIR/plots."
}

generate_mv_comparison() {
  (cd utils; python3 mv_compare.py "$RESULTS_DIR/method0_output_0.csv" "$RESULTS_DIR/method7_output_0.csv" 10 100 "$RESULTS_DIR/mv_comparison_result.txt")
  if [[ -f "$RESULTS_DIR/mv_comparison_result.txt" ]]; then
    echo "Motion vector comparison result saved to $RESULTS_DIR/mv_comparison_result.txt."
  else
    echo "Error: mv_comparison_result.txt was not created."
  fi
}

profiler() {
  echo "Running VTune profiler on extractor7 with motion_vectors_only=1..."

  FFMPEG_LIB="${PWD}/ffmpeg-8.0/ffmpeg-8.0-ourversion/FFmpeg/lib"
  export LD_LIBRARY_PATH="$FFMPEG_LIB/libavutil:$FFMPEG_LIB/libavformat:$LD_LIBRARY_PATH"
  vtune_dir="$RESULTS_DIR/vtune_results"
  mkdir -p "$vtune_dir"

  # Step 1: Collect data

  (cd ./extractors/executables; vtune -collect hotspots -result-dir "$vtune_dir" -- ./extractor7 "$INPUT" 0 "$RESULTS_DIR/method7_output_vtune.csv")

  vtune -report hotspots -result-dir "$vtune_dir" -format csv -report-output "$vtune_dir/hotspots.csv" 

  vtune -report top-down -result-dir "$vtune_dir" -format csv -report-output "$vtune_dir/topdown.csv"
  # Generate call tree and hotspots bar chart from VTune topdown CSV
  (cd utils; python3 vtune_hotspots_plot.py "$vtune_dir/topdown.csv")

  echo "Profiler run complete. Results in $vtune_dir."
}

run_all() {
  build
  extract
  combine
  decode
  generate_mv_comparison
  plot
  profiler
  echo "$(realpath "$RESULTS_DIR")"
}

usage() {
  echo
  echo "Usage: $0 <input_video_or_rtsp_url> [streams]"
  echo "  Set the input (video filename or RTSP URL) as the first argument."
  echo "  The number of 'streams' for benchmarking is optional (default = 1)."
  echo "  You will then be prompted to pick which step(s) to run."
  echo "    1 = Build"
  echo "    2 = Extract (run benchmark)"
  echo "    3 = Combine CSVs"
  echo "    4 = Decode Reference Video"
  echo "    5 = Generate Plots and PowerPoint"
  echo "    6 = Generate MV comparison"
  echo "    7 = Profiler (VTune on FFmpeg hacked)"
  echo "    0 = Run ALL steps in sequence"
  echo
}

# --- Main ---

if [[ -z "$1" ]]; then
  usage
  exit 1
fi

INPUT="$1"
INPUT1="${2:-1}"    # Defaults to 1 if second argument missing

# Optional validation of streams argument:
if ! [[ "$INPUT1" =~ ^[0-9]+$ && "$INPUT1" -ge 1 ]]; then
  echo "Error: streams argument must be a positive integer"
  exit 1
fi

echo
echo "Select steps to run (enter one or more numbers separated by space):"
echo "  1: Build"
echo "  2: Extract (run benchmark)"
echo "  3: Combine CSVs"
echo "  4: Decode Reference Video"
echo "  5: Generate Plots and PowerPoint"
echo "  6: Generate MV comparison"
echo "  7: Profiler (VTune on FFmpeg hacked)"
echo "  0: Run ALL steps"
echo
read -p "Choice(s): " CHOICES

for step in $CHOICES; do
  case "$step" in
    1) build ;;
    2) extract ;;
    3) combine ;;
    4) decode ;;
    5) plot ;;
    6) generate_mv_comparison ;;
    7) profiler ;;
    0) run_all; break ;;
    *) echo "Invalid step: $step" ;;
  esac
done