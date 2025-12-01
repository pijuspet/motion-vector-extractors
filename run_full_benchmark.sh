#!/bin/bash

set -e

# --- Results Directory Setup ---
RESULTS_BASE="/media/loab/f53f31e5-20d9-427c-b719-3e150951a7ec/results"
mkdir -p "$RESULTS_BASE"
RUN_TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
RESULTS_DIR="$RESULTS_BASE/$RUN_TIMESTAMP"
mkdir -p "$RESULTS_DIR"
export RESULTS_DIR

build() {
  echo "Building all extractors and tools..."
  make
  g++ -O2 -o benchmark_all_9 benchmarking.cpp \
    `pkg-config --cflags --libs libavformat libavcodec libavutil libswscale` -lm
  gcc -O2 -o combine_mv_csv combine_mv_csv.c
  gcc -O2 -o complete_video_generator_9 vidgenerator.c \
    `pkg-config --cflags --libs libavformat libavcodec libavutil libswscale` -lm
 
  echo "Build complete."
}

extract() {
  if [[ -z "$INPUT" ]]; then
    echo "Extraction step skipped: set INPUT environment variable to input file."
    return 1
  fi
  echo "Running 9-method benchmark suite..."
  ./benchmark_all_9 "$INPUT" "$INPUT1" "$RESULTS_DIR"
  
  echo "Benchmarks complete."
}

combine() {
  echo "Combining only method6, and method7 CSV outputs..."
  TMP_COMBINE_DIR=$(mktemp -d)
  for m in 6 7; do
    for f in "$RESULTS_DIR"/method${m}_output.csv_*.csv; do
      if [[ -f "$f" ]]; then
        cp "$f" "$TMP_COMBINE_DIR/"
      fi
    done
  done
  ./combine_mv_csv "$TMP_COMBINE_DIR"
  mv all_motion_vectors.csv "$RESULTS_DIR/all_motion_vectors.csv"
  rm -rf "$TMP_COMBINE_DIR"
  echo "Combined motion vectors (method6,7) saved to $RESULTS_DIR/all_motion_vectors.csv."
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
  python3 benchmark_python.py "$INPUT" "$INPUT1" "$RESULTS_DIR/plots"
  echo "Plotting complete. Plots and PPTX in $RESULTS_DIR/plots."
}

generate_mv_comparison() {
  python3 mv_compare.py "$RESULTS_DIR/method0_output.csv_0.csv" "$RESULTS_DIR/method7_output.csv_0.csv" 10 100 "$RESULTS_DIR/mv_comparison_result.txt"
  if [[ -f "$RESULTS_DIR/mv_comparison_result.txt" ]]; then
    echo "Motion vector comparison result saved to $RESULTS_DIR/mv_comparison_result.txt."
  else
    echo "Error: mv_comparison_result.txt was not created."
  fi
}

profiler() {
  echo "Running VTune profiler on extractor7 with motion_vectors_only=1..."
  export LD_LIBRARY_PATH="/home/loab/Documents/motion-vector-extractors-/ffmpeg-8.1/hacked/lib/libavutil:/home/loab/Documents/motion-vector-extractors-/ffmpeg-8.1/hacked/lib/libavformat:$LD_LIBRARY_PATH"
  vtune_dir="$RESULTS_DIR/vtune_results"
  vtune_dir_mem="$RESULTS_DIR/vtune_results_mem"
  mkdir -p "$vtune_dir"
  mkdir -p "$vtune_dir_mem"

  # Step 1: Collect data

  vtune -collect hotspots -result-dir "$vtune_dir" -- ./extractor7 "$INPUT" 0

  vtune -report hotspots -result-dir "$vtune_dir" -format csv -report-output "$vtune_dir/hotspots.csv" 

  vtune -report top-down -result-dir "$vtune_dir" -format csv -report-output "$vtune_dir/topdown.csv"
  # Generate call tree and hotspots bar chart from VTune topdown CSV
  python3 vtune_hotspots_plot.py "$vtune_dir/topdown.csv"

  #vtune -collect memory-access -result-dir "$vtune_dir_mem" -- ./extractor7 "$INPUT" 0
  #vtune -report memory-access -result-dir "$vtune_dir_mem" -format csv -report-output "$vtune_dir_mem/memory_report.csv"

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


# Print only the absolute results directory as the last line for automation
echo #"$(realpath "$RESULTS_DIR")"
echo "$(realpath "$RESULTS_DIR")"

