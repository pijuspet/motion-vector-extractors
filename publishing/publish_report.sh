#!/bin/bash
run_all() {
  echo "Running full benchmark and publishing results..."
  
  # Run the benchmark to get NEW results directory
  LATEST_RESULTS_DIR=$(run_benchmark | tail -n 1) 
  echo "Latest results directory: $LATEST_RESULTS_DIR"
  
  # Set the FIRST results directory (you can make this dynamic if needed)
  FIRST_RESULTS_DIR="/media/loab/f53f31e5-20d9-427c-b719-3e150951a7ec/published/20250922_133754"
  
  # Get git commit URL for the NEW run
  LATEST_GIT_COMMIT=$(publish_git | grep GIT_COMMIT_URL | awk -F '=' '{print $2}')
  if [[ -z "$LATEST_GIT_COMMIT" ]]; then
    LATEST_GIT_COMMIT="https://github.com/ablouise/ffmpeg-8.0-ourversion/commit/$(git -C /home/loab/Documents/ffmpeg-8.0-ourversion rev-parse HEAD)"
  fi
  
  # Set the FIRST git commit (hardcoded for comparison)
  FIRST_GIT_COMMIT="https://github.com/ablouise/ffmpeg-8.0-ourversion/commit/6faaff56c675b77dc783afc89a1dfb113c07bcf9"
  
  echo "Publishing to Confluence with:"
  echo "  First results dir: $FIRST_RESULTS_DIR"
  echo "  Latest results dir: $LATEST_RESULTS_DIR"
  echo "  First git commit: $FIRST_GIT_COMMIT"
  echo "  Latest git commit: $LATEST_GIT_COMMIT"
  
  publish_confluence "$FIRST_RESULTS_DIR" "$LATEST_RESULTS_DIR" "$FIRST_GIT_COMMIT" "$LATEST_GIT_COMMIT"
}

publish_git() {
  REPO_PATH="${PWD}/ffmpeg"
  echo "Committing and pushing all changes to git in $REPO_PATH..."
  git -C "$REPO_PATH" add .
  git -C "$REPO_PATH" commit -m "Automated benchmark and report update $(date +'%Y-%m-%d %H:%M:%S')" || echo "Nothing to commit."
  git -C "$REPO_PATH" push origin
  COMMIT_HASH=$(git -C "$REPO_PATH" rev-parse HEAD)
  REMOTE_URL=$(git -C "$REPO_PATH" config --get remote.origin.url)
  if [[ "$REMOTE_URL" =~ ^git@github.com:(.*)\.git$ ]]; then
    GH_PATH="${BASH_REMATCH[1]}"
    COMMIT_URL="https://github.com/$GH_PATH/commit/$COMMIT_HASH"
  elif [[ "$REMOTE_URL" =~ ^https://github.com/(.*)\.git$ ]]; then
    GH_PATH="${BASH_REMATCH[1]}"
    COMMIT_URL="https://github.com/$GH_PATH/commit/$COMMIT_HASH"
  else
    COMMIT_URL="$COMMIT_HASH"
  fi
  echo "GIT_COMMIT_URL=$COMMIT_URL"
}

publish_confluence() {
  local first_dir="$1"
  local latest_dir="$2"
  local git_commit_run1="$3"
  local git_commit_run2="$4"
  
  echo "Publishing report to Confluence..."
  echo "  First results directory: $first_dir"
  echo "  Latest results directory: $latest_dir"
  echo "  Git commit run 1: $git_commit_run1"
  echo "  Git commit run 2: $git_commit_run2"
  
  if [[ -z "$first_dir" || -z "$latest_dir" || -z "$git_commit_run1" || -z "$git_commit_run2" ]]; then
    echo "Error: You must provide FIRST and LATEST results directories and both git commit URLs as arguments."
    echo "Usage: publish_confluence <first_results_dir> <latest_results_dir> <git_commit_run1> <git_commit_run2>"
    exit 1
  fi
  
  if [[ ! -d "$first_dir" ]]; then
    echo "Error: First results directory '$first_dir' does not exist."
    exit 1
  fi
  
  if [[ ! -d "$latest_dir" ]]; then
    echo "Error: Latest results directory '$latest_dir' does not exist."
    exit 1
  fi
  
  echo "Calling Python script with parameters..."
  (cd publishing; python3 publish_to_confluence.py "$first_dir" "$latest_dir" "$git_commit_run1" "$git_commit_run2")

  PUBLISHED_DIR="${latest_dir/\/results\//\/published\/}"

  mkdir -p "$(dirname "$PUBLISHED_DIR")"
  cp -r "$latest_dir" "$PUBLISHED_DIR"
  echo "Published results copied to: $PUBLISHED_DIR"
}

run_benchmark() {
  CURRENT_DIR=$(pwd)
  echo "DEBUG: Starting benchmark..."
  echo 0 | ./benchmarking/run_full_benchmark.sh "$CURRENT_DIR/videos/bigbunny.mp4" 15
  echo "DEBUG: Benchmark script finished."

  RESULTS_DIR=$(ls -d "$CURRENT_DIR"/results/* | sort | tail -n 1)
  echo "$RESULTS_DIR"
}

# --- Interactive menu ---

# Allow passing menu choice as a command-line argument for non-interactive use
if [[ -n "$1" ]]; then
  CHOICES="$1"
else
  echo
  echo "Select publish step to run (enter one or more numbers separated by space):"
  echo "  1: Run Full Benchmark"
  echo "  2: Commit to Git"
  echo "  3: Publish to Confluence"
  echo "  0: Run ALL (benchmark, git, confluence)"
  echo
  read -p "Choice(s): " CHOICES
fi

for step in $CHOICES; do
  case "$step" in
    1)
      run_benchmark
      ;;
    2)
      publish_git
      ;;
    3)
      # If four arguments are provided, use them as the results directories and git commit URLs
      if [[ -n "$2" && -n "$3" && -n "$4" && -n "$5" ]]; then
        publish_confluence "$2" "$3" "$4" "$5"
      else
        # Prompt for all four arguments interactively
        read -p "Enter the path to the FIRST results directory: " FIRST_RESULTS_DIR
        read -p "Enter the path to the LATEST results directory: " LATEST_RESULTS_DIR
        read -p "Enter the Git commit URL for RUN 1: " GIT_COMMIT_RUN1
        read -p "Enter the Git commit URL for RUN 2: " GIT_COMMIT_RUN2
        publish_confluence "$FIRST_RESULTS_DIR" "$LATEST_RESULTS_DIR" "$GIT_COMMIT_RUN1" "$GIT_COMMIT_RUN2"
      fi
      ;;
    0)
      run_all
      break
      ;;
    *)
      echo "Invalid step: $step"
      ;;
  esac
done
