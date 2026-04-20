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

3. **Build both FFmpeg versions** (standard + custom-patched)
```bash
make setup_ffmpeg
```
This clones FFmpeg `release/8.0` into `ffmpeg/FFmpeg-8.0/FFmpeg` and `ffmpeg/FFmpeg-8.0-custom/FFmpeg`, applies the patch from `ffmpeg_installer/`, and compiles both. Takes several minutes.

4. **Build all extractors**
```bash
make build
```
This compiles every extractor twice — once linked against the standard FFmpeg (`target/extractor-sys`), and once against the custom-patched FFmpeg (`target/extractor-cust`). Binaries are copied into `executables/`.

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

## Updating the Custom FFmpeg Patch

The `ffmpeg_installer/` submodule ships the diff that transforms a vanilla FFmpeg `release/8.0` checkout into the custom-patched build. When you change the FFmpeg source under `ffmpeg/FFmpeg-8.0-custom/FFmpeg/`, regenerate the diff and commit it so others can apply the same changes.

### Generate the diff

```bash
make installer_diff
```

This clones a fresh copy of `FFmpeg release/8.0` into `/tmp/ffmpeg-8.0-fresh` (skipped if it already exists), diffs it against `ffmpeg/FFmpeg-8.0-custom/FFmpeg/`, and writes the result to `ffmpeg_installer/custom_ffmpeg.diff`. Build artifacts, binaries, and generated files are excluded automatically.

### Stage and commit

```bash
make installer_publish
```

Runs `installer_diff` then stages `ffmpeg_installer/ffmpeg_version.diff` in the submodule.

## Running the Benchmark

To run the full benchmark run:
```
make benchmark
```

To run all experiments for original and custom FFmpeg:
```
make all
```

- Replace video with your input video file from the videos in `videos/`.

During execution, you’ll be presented with options. If you select **option `0`**, the script will:
- Run all benchmarks.
- Generate charts.
- Create a PowerPoint presentation (PPT).
- Compare original and custom FFmpegs extracted motion vectors
- Generate Vtune and flamegraph useage plots. 

> **Note:** Selecting option 0 will take longer because it performs both the benchmarks and the full reporting.

## Generate motion vector video
```
make generate_video
```

videos are saved in `/results/[date]` folder (requires `method0_output_0.csv` and `method4_output_0.csv` files, run `make benchmark` with flag 0 beforehand).

## Results Output

After the benchmarks are complete:
- All plot images (`.png`) and the PowerPoint presentation (`.ppt`), including the results, will be available in the `plot` folder.
- Motion vectors, vtune results are saved in `/results/[date]/` folder.

## Current Results 

> **Note:** The 3 with FFMPEG Patched use the Naive return version of FFMPEG, and the one called "Same" - is a copy of the code that performs best on the patched running not  on the Patched

<img width="1600" height="900" alt="grouped_barchart_fps" src="https://github.com/user-attachments/assets/21f15b0b-f9a1-4ca6-8f5a-04c6f3347246" />

<img width="1600" height="900" alt="scaling_timeperframe" src="https://github.com/user-attachments/assets/16ae1c73-3a82-4525-b752-12fa3311d01d" />

<img width="3077" height="1112" alt="detail_table_15streams" src="https://github.com/user-attachments/assets/e1e74285-9eb4-4354-b18c-13a192364db4" />

