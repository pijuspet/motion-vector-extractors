# README.md

## Getting Started

To set up and use this project, follow these steps:

1. **Clone the repository**
```bash
git clone --recurse-submodules https://github.com/pijuspet/motion-vector-extractors
```

2. **Install dependencies**
```bash
sudo make install
```
Run it with `sudo`, not as a root shell. VTune, the apt packages and the sysctl settings (`ptrace_scope=0`, `perf_event_paranoid=1`) need root. Rust (rustup), the Python venv, `git lfs pull` and `executables/` run as the invoking user (`$SUDO_USER`), so nothing ends up in `/root` or owned by root. Open a new shell afterwards so `cargo` is on `PATH`.

3. **Build both FFmpeg versions** (standard + custom-patched)
```bash
make setup_ffmpeg
```
Both are submodules of [pijuspet/ffmpeg](https://github.com/pijuspet/ffmpeg), each on its own branch, and install into `ffmpeg/install/` (see [FFmpeg sources](#ffmpeg-sources)).

4. **Build all extractors**
```bash
make build
```
This compiles every extractor twice — once linked against the standard FFmpeg (`target/extractor-sys`), and once against the custom-patched FFmpeg (`target/extractor-cust`). Binaries are copied into `executables/`.

It also builds the [edge264](https://github.com/tvlabs/edge264) submodule and `extractor8` (benchmark method 8), a third-party from-scratch H.264 decoder driven by `edge264/extractor.c`. If the submodule has not been checked out the step is skipped with a hint rather than failing:

```bash
git submodule update --init edge264
make setup_edge264       # rebuild just the decoder after a submodule update
make setup_edge264_pgo   # instrument -> train -> rebuild, same recipe as setup_ffmpeg_pgo
```

## Usage

`make` with no target prints the help screen with the current variable values. Every variable below can be overridden on the command line, e.g. `make benchmark VIDEO_NAME=bigbunny.mp4 THREAD_COUNT=4`.

### Setup and build

| Command | What it does |
|---|---|
| `sudo make install` | Installs the toolchain and dependencies, and creates `.env` from `.env_template` |
| `make setup_ffmpeg` | Builds both FFmpeg trees: the regular one (sys) and the custom-patched fork (cust) |
| `make setup_ffmpeg_pgo` | Profile-guided build of the custom fork: instrumented build, then a training run, then an optimized rebuild, then `make build` |
| `make setup_edge264` | Rebuilds only the edge264 decoder (method 8), e.g. after a submodule update |
| `make setup_edge264_pgo` | The same PGO recipe for edge264 |
| `make build` | Builds every extractor against both FFmpeg trees plus edge264, and copies the binaries and runtime libs into `executables/` |
| `make build_tools` | `cargo build --workspace --release` against the regular FFmpeg |
| `make test` | `cargo test --workspace`. There are no tests yet, so it only compiles the workspace |

PGO training is controlled by `PGO_TRAIN_CLIPS`, `PGO_TRAIN_TYPES` and `PGO_TRAIN_THREADS`. edge264 uses `PGO_E264_TRAIN_CLIPS` and `PGO_E264_TRAIN_TYPES` instead. Example:
```bash
make setup_ffmpeg_pgo PGO_TRAIN_TYPES=h264_cabac PGO_TRAIN_THREADS=1
```
The default training clip (`MCTTR0102b`) is also a benchmark clip. Measure PGO gains on a different clip (school or bus), otherwise you are testing on the training data.

### Benchmarks

| Command | What it does |
|---|---|
| `make benchmark` | Benchmarks one video (`VIDEO_TYPE`/`VIDEO_NAME`). Without `STEPS` it asks which steps to run (see below) |
| `make benchmark_all` | Benchmarks `VIDEO_NAME` for every type in `VIDEO_TYPES`. Add `TYPE=sys` or `TYPE=cust` (default `cust`) to pick one tree |
| `make all` | Runs `benchmark_all` for both `sys` and `cust` |
| `make benchmark_threads` | Runs every video at 1, 2, 4, … up to `MAX_THREADS` threads |
| `make benchmark_skip_nth` | Sweeps `MV_SKIP_EVERY_NTH` from `SKIP_NTH_FROM` to `SKIP_NTH_TO` in steps of `SKIP_NTH_STEP`, with `MV_SKIP_FRAME=bidir` |
| `make benchmark_decode_nth` | Sweeps `MV_DECODE_EVERY_NTH` from `DECODE_NTH_FROM` to `DECODE_NTH_TO` in steps of `DECODE_NTH_STEP` |

The `full_benchmark` steps, which you select with `STEPS="..."` or at the prompt:

| Step | |
|---|---|
| `1` | Build |
| `2` | Extract (run the benchmark) |
| `3` | Compare motion vectors between `COMPARE_FIRST` and `COMPARE_SECOND` |
| `4` | Plots and PowerPoint |
| `5` | VTune profile of `PROFILER_EXTRACTOR` (Linux only) |
| `6` | Flamegraph (Linux only) |
| `0` | All of the above. This takes much longer than extraction alone because it includes the full reporting |

```bash
make benchmark STEPS="2 4"          # extract + plots, no prompt
```

### Results, videos and reports

Each run writes its MV CSVs and VTune/flamegraph output to `results/<video_type>/<date>_<time>_.../`. Plot images (`.png`) and the PowerPoint go to `plots/`. The commands below read from the newest results dir, so run `make benchmark` first.

| Command | What it does |
|---|---|
| `make generate_video` | Renders the MV overlay and the side-by-side comparison video from the newest results dir (needs `method0_output_0.csv` and `method5_output_0.csv`) |
| `make generate_videos_since` | The same for every results dir created on or after `SINCE_DAY` and `SINCE` (`HHMM`). Uses the first CSV it finds among `CUST_METHODS` |
| `make compare_mvs` | Compares the method 0 and method 5 MV CSVs in the newest results dir and writes `mv_diff_neg1.txt` |
| `make decode_ffmpeg` | Remuxes `VIDEO_FILE` with the custom FFmpeg CLI into the newest results dir |
| `make publish` | Publishes the report for the newest results dir |
| `make publish_titles` | Dry run: prints the page titles `results/bulk` would be published under |

### Common variables

| Variable | Default | Meaning |
|---|---|---|
| `VIDEO_NAME` / `VIDEO_TYPE` | `MCTTR0102b.mp4` / `h264_cabac` | Single video for `benchmark`, `generate_video` and `compare_mvs`. Types: `h264_cabac`, `h264_cavlc`, `h264_avi`, `h265` |
| `VIDEO_NAMES` / `VIDEO_TYPES` | school, bus, MCTTR / `h264_cabac` | Video set for the multi-video targets. Missing files are skipped |
| `STREAMS`, `NRUNS` | `20`, `3` | Parallel streams and repeat runs |
| `THREAD_COUNT` | `1` | Decoder threads per extractor (`0` lets FFmpeg choose) |
| `WRITE_CSV` | `0` | `1` = write per-method MV CSVs |
| `L0_ONLY` | `1` | Export list-0 vectors only |
| `MV_SKIP_FRAME` | empty | `noref` / `bidir` / `nointra` / `nokey` frame skipping |
| `MV_SKIP_EVERY_NTH` | `0` | Skip every Nth picture. Always use it with `MV_SKIP_FRAME=bidir` |
| `MV_DECODE_EVERY_NTH` | `0` | Decode only every Nth picture |
| `COMPARE_FIRST`, `COMPARE_SECOND` | `1`, `4` | The method pair compared in step 3 |
| `PROFILER_EXTRACTOR` | `4` | The extractor profiled in steps 5 and 6 |

## Frame decimation

The extractors can skip pictures before they are entropy-decoded, using
`MV_SKIP_FRAME`, `MV_SKIP_EVERY_NTH` and `MV_DECODE_EVERY_NTH`.

### Fixing stale Rust bindings after a header change

`ffmpeg-sys-next` generates Rust FFI bindings via bindgen at build time. Cargo caches these bindings and only regenerates them when `PKG_CONFIG_PATH` changes — it does **not** watch the FFmpeg header files themselves. If you rebuild the custom FFmpeg (e.g. by reapplying or updating the patch) after `make build` has already run, the cached bindings in `target/extractor-cust` will be stale and will be missing `AVMotionVectorCompact` and `AV_FRAME_DATA_MOTION_VECTORS_COMPACT`, causing compile errors like:

```
unresolved import `ffmpeg_sys_next::AVMotionVectorCompact`
no variant or associated item named `AV_FRAME_DATA_MOTION_VECTORS_COMPACT` found for enum `AVFrameSideDataType`
```

Fix: delete the stale bindgen cache and rebuild.

```bash
rm -rf target/extractor-cust/release/build/ffmpeg-sys-next-*
make build
```

This forces bindgen to re-run against the updated headers. You only need to do this after the custom FFmpeg headers themselves change.

## FFmpeg sources

`ffmpeg/` is a plain folder holding two submodules of [pijuspet/ffmpeg](https://github.com/pijuspet/ffmpeg). Each branch builds on the one before it:

| Path | Branch | Contents | Used by |
|---|---|---|---|
| — | `release/8.0` | unmodified upstream FFmpeg `release/8.0` | base of the other two |
| `ffmpeg/FFmpeg-8.0` | `release/8.0-hevc-mv` | upstream + HEVC motion-vector export | regular build: methods 0, 1, 2, 7 |
| `ffmpeg/FFmpeg-8.0-custom` | `release/8.0-develop` | the above + the MV-only patch | custom build: methods 3, 4, 5, 6 |

`make setup_ffmpeg` configures and builds each checkout in place and installs it into `ffmpeg/install/FFmpeg-8.0[-custom]/` (gitignored). A built binary reports the commit it came from: `ffmpeg -version` prints e.g. `e22fcf859f custom`.