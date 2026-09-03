# =============================================================================
# motion-vector-extractors — build, benchmark and reporting driver
# =============================================================================
# One makefile for every platform. Everything platform-specific lives in
# mk/<platform>.mk; everything below this header is shared.
#
#   make <target>                    Linux, or MSYS2 MINGW64 (auto-detected)
#   make PLATFORM=msvc <target>      Windows, native MSVC toolchain
#
# PLATFORM is exported, so any recursive $(MAKE) — and anything this makefile
# spawns that shells back out to make, such as full_benchmark — stays on the
# platform it was invoked with.

SHELL := /bin/bash

# -----------------------------------------------------------------------------
# Platform selection
# -----------------------------------------------------------------------------
# MSVC is never auto-detected: it and MinGW both run under MSYS2 and `uname -s`
# reports MINGW64_NT/MSYS_NT for both, so it has to be asked for explicitly.
ifeq ($(origin PLATFORM),undefined)
  UNAME_S := $(shell uname -s)
  ifneq (,$(filter MINGW% MSYS%,$(UNAME_S)))
    PLATFORM := mingw
  else
    PLATFORM := linux
  endif
endif
# Exported so recursive $(MAKE) calls stay on the same platform.
export PLATFORM

# =============================================================================
# CONFIGURATION & GLOBAL VARIABLES
# =============================================================================

STREAMS = 15
NRUNS = 3
# Set KEYFRAMES_ONLY=1 to have every extractor decode I-frames only.
KEYFRAMES_ONLY ?= 0
# Set THREAD_COUNT=N to pin every extractor to N threads (0 = FFmpeg auto).
THREAD_COUNT ?= 1
# Set WRITE_CSV=0 to skip writing per-extractor MV output CSV files.
WRITE_CSV ?= 0
# List-0-only MV export (drops list-1/forward-reference rows), on by default
# so CSV sizes are directly comparable across every method: extractor1/3/5/6
# (custom FFmpeg) have always been list-0-only via mv_l0_only, and extractor9/10
# (from-scratch Rust/C) now match via E9_L0_ONLY/E10_L0_ONLY. Set L0_ONLY=0 to
# get full both-list output everywhere (e.g. for B-frame/list-1 investigation).
L0_ONLY ?= 1
# Motion-vector export filters in the custom FFmpeg (extractor1/3/5/6; the
# regular FFmpeg build has no such options and ignores them). They shrink the
# OUTPUT only: the picture is fully entropy-decoded before any of this runs, so
# decode cost is unchanged.
#
# Order: size threshold first, then the grid, so a cell is claimed by a vector
# that actually passed the threshold rather than by a sub-threshold one
# suppressing the real vector behind it.
#
# Spatial thinning: split the picture into MV_GRID x MV_GRID pixel cells and
# keep at most one vector per cell per picture. 0 = keep every vector. This
# gives an evenly spread field, which is what a fixed camera wants - unlike
# sampling by decode order, which clusters wherever the macroblock walk went.
MV_GRID ?= 0
# Drop motion vectors whose displacement is shorter than this many whole pixels
# (Euclidean length of dst-src). 0 = no size filter, export every vector.
MV_MIN_SIZE ?= 0
# --- Temporal decimation. The filters above cost full decode time; these do
# not - they drop whole pictures before any bin is entropy-decoded.
#
# MV_SKIP_FRAME uses FFmpeg's own skip_frame vocabulary: noref, bidir, nointra,
# nokey (note 'nointra', not 'nonintra'). Empty = decode everything.
# bidir drops B pictures: measured 1.99s -> 1.31s on MCTTR, and every surviving
# vector is bit-identical to an undecimated run.
MV_SKIP_FRAME ?=
# Skip every Nth picture, decoding the rest (0/1 = decode all). IDR always
# decoded. Note the direction: 2 drops half, 3 a third, 4 a quarter, so a LARGER
# N is a gentler trim. That is deliberate - a skipped picture cannot be
# recovered afterwards (interpolating one from its neighbours measured a larger
# error than the motion itself), so this is for mild trimming, not decimation.
#
# ALWAYS pair with MV_SKIP_FRAME=bidir: on its own it leaves B slices using
# temporal direct mode reading a collocated picture that was skipped, which
# measured 2.8-4.2% wrong vectors. With bidir, 0% wrong.
#
# The counter runs over ALL pictures in decode order, so on a stream with a
# regular GOP the skip period can alias against it and remove far fewer decoded
# pictures than 1/N suggests - measure per clip rather than assuming.
MV_SKIP_EVERY_NTH ?= 0
# Which two methods the "Generate MV comparison" step's full (both-lists)
# sanity check compares (step 3, logged as "first"/"second" rather than bare
# method numbers). Default 1/4: both built from extractor1.rs, one against
# the regular FFmpeg and one against the custom fork.
COMPARE_FIRST ?= 1
COMPARE_SECOND ?= 4
# Extractor number to profile with perf/VTune (step 5/6).
PROFILER_EXTRACTOR ?= 4

# VIDEO_NAME ?= bigbunny.mp4
# VIDEO_NAME ?= stickman.mp4
# VIDEO_NAME ?= dashcam.mp4
# VIDEO_NAME ?= 2018-03-05.09-50-15.09-55-01.school.G423.r13.mp4
# VIDEO_NAME ?= 2018-03-15.15-55-00.16-00-00.bus.G475.r13.mp4
VIDEO_NAME ?= MCTTR0102b.mp4
# VIDEO_NAME ?= bigbunny_walking.mp4

# All video names iterated by benchmark_keyframes and benchmark_threads.
# Files that don't exist for a given type are silently skipped.
VIDEO_NAMES ?= 2018-03-05.09-50-15.09-55-01.school.G423.r13.mp4 \
               2018-03-15.15-55-00.16-00-00.bus.G475.r13.mp4 \
               MCTTR0102b.mp4 \
			   bigbunny_walking.mp4 \
               stickman.mp4 \
               dashcam.mp4 

VIDEO_TYPES ?= h264_cabac
# VIDEO_TYPES ?= h264_cabac h264_cavlc h264_avi h265
VIDEO_TYPE  ?= h264_cabac
# VIDEO_TYPE = h264_cavlc
# VIDEO_TYPE = h264_avi
# VIDEO_TYPE = h265

# $(CURDIR) is portable and shell-free; under MSYS2 MINGW64 it yields the
# forward-slash POSIX-style path the rest of this file expects.
CURRENT_DIR := $(CURDIR)
PARENT_DIR  := $(patsubst %/,%,$(dir $(CURRENT_DIR)))

EXECUTABLES_DIR := executables

CUSTOM_PREFIX  := $(CURRENT_DIR)/ffmpeg/FFmpeg-8.0-custom
REGULAR_PREFIX := $(CURRENT_DIR)/ffmpeg/FFmpeg-8.0
include mk/$(PLATFORM).mk

# =============================================================================
# PATHS & ENVIRONMENTS
# =============================================================================

PYTHON := $(VENV_FOLDER)/bin/python$(EXE_EXT)

TARGET_SYS    := $(CARGO_TARGET_BASE)/extractor-sys
TARGET_CUST   := $(CARGO_TARGET_BASE)/extractor-cust
VIDEO_FILE := $(CURRENT_DIR)/videos/$(VIDEO_TYPE)/$(VIDEO_NAME)

INITIAL_RUN_DATA := $(CURRENT_DIR)/published/$(VIDEO_TYPE)/initial_results_$(VIDEO_TYPE)
# Trailing slash on the glob so `ls -d` yields only directories: reports written
# alongside the run folders (e.g. compare_runs output) would otherwise sort last
# and make this resolve to a plain file.
LAST_RESULTS_DIR = $(patsubst %/,%,$(shell ls -d "$(CURRENT_DIR)/results/$(VIDEO_TYPE)/"*/ 2>/dev/null | sort | tail -n 1))

# NB: no trailing comments on these two — make keeps the whitespace before a
# `#` as part of the value, and these are used quoted.
CSV_FILE_PATH_ORIG = $(LAST_RESULTS_DIR)/method0_output_0.csv
CSV_FILE_PATH_CUST = $(LAST_RESULTS_DIR)/method5_output_0.csv

ifeq ($(PLATFORM),msvc)
MAKE_HINT := make PLATFORM=msvc
else
MAKE_HINT := make
endif

.DEFAULT_GOAL := help

# =============================================================================
# INSTALLATION
# =============================================================================

# Platform-specific dependency installation lives in mk/<platform>.mk as
# `platform_install`; only the shared tail is here.
install: platform_install
	@if [ ! -f .env ] && [ -f .env_template ]; then cp .env_template .env; fi
	@mkdir -p '$(EXECUTABLES_DIR)'
	@echo "[OK]    Install complete."

# =============================================================================
# FFMPEG SETUP
# =============================================================================

# $(1) = install prefix. The tree is configured and built in place under
# <prefix>/FFmpeg and installed into <prefix>.
define ffmpeg_build
	cd '$(1)/FFmpeg' && \
	chmod +x ./configure ./ffbuild/*.sh && \
	./configure --prefix='$(1)' $(FF_CONFIGURE_FLAGS) && \
	make -j"$$(nproc)" && make install
endef

# `build` links extractor11 against SLIM_PREFIX via pkg-config, so the slim tree
# has to exist before any benchmark target runs — on a fresh checkout
# `setup_ffmpeg` followed by `all` used to fail there with empty pkg-config
# flags. Building all three prefixes here is what makes method 11 reachable
# from `make all`.
setup_ffmpeg: $(PLATFORM_GUARD)
	$(call ffmpeg_build,$(CUSTOM_PREFIX))
	$(call ffmpeg_build,$(REGULAR_PREFIX))

# -----------------------------------------------------------------------------
# Profile-guided optimization
# -----------------------------------------------------------------------------
# Three phases: instrument, train, rebuild with the profile. `make clean`
# between them is mandatory — configure regenerates config.mak but leaves the
# objects behind, and they must be recompiled under the new profile flags or
# the profile is silently ignored. The exact instrument/optimize flags are
# per-toolchain and live in mk/<platform>.mk.
#
# What the training run covers. All three are overridable on the command line
# and apply to BOTH setup_ffmpeg_pgo and setup_ffmpeg_slim_pgo — a profile is
# only as good as the workload it saw, so narrow these when you want the build
# tuned for one specific case rather than the whole corpus:
#
#   # tune the fork for single-threaded CABAC only
#   make setup_ffmpeg_pgo PGO_TRAIN_TYPES=h264_cabac PGO_TRAIN_THREADS=1
#
#   # tune the slim tree for your own footage, both thread regimes
#   make setup_ffmpeg_slim_pgo PGO_TRAIN_CLIPS="clipA clipB" PGO_TRAIN_THREADS="1 16"
#
# Defaults: one clip across all four corpora, so the profile covers every
# decoder these trees carry (h264 CABAC, h264 CAVLC, HEVC, AVI / MPEG-4 Part 2).
# PGO wants branch coverage, not volume — one clip per codec is enough. Note the
# default clip is also a benchmark clip, so measure PGO gains on a held-out clip
# (school / bus) or the number is train-on-test.
PGO_TRAIN_CLIPS   ?= MCTTR0102b
PGO_TRAIN_TYPES   ?= h264_cabac h264_cavlc h265 h264_avi
# Both extremes by default: t1 exercises the serial decode path (where PGO's
# gain concentrates), t16 the frame-threading and synchronisation paths. Set
# to just "1" if the target workload is single-threaded — the profile then
# stops spending budget on thread-handoff branches that will never be taken.
PGO_TRAIN_THREADS ?= 1 16

PGO_CUST_DIR := $(CUSTOM_PREFIX)/pgo

# $(1) = extractor binary to train with, $(2) = profile directory.
# Loops clips x corpora x thread counts. A missing file is skipped rather than
# fatal, so narrowing PGO_TRAIN_TYPES does not require a matching clip in every
# corpus. Writes a TRAINED_ON manifest next to the profile: a profile directory
# with no record of the workload behind it cannot be reasoned about later.
define pgo_train_run
	@echo "  clips=[$(PGO_TRAIN_CLIPS)] corpora=[$(PGO_TRAIN_TYPES)] threads=[$(PGO_TRAIN_THREADS)]"
	@for clip in $(PGO_TRAIN_CLIPS); do \
		for vtype in $(PGO_TRAIN_TYPES); do \
			if [ "$$vtype" = "h264_avi" ]; then \
				f='$(CURRENT_DIR)'/videos/$$vtype/$$clip.avi; \
			else \
				f='$(CURRENT_DIR)'/videos/$$vtype/$$clip.mp4; \
			fi; \
			if [ -f "$$f" ]; then \
				for t in $(PGO_TRAIN_THREADS); do \
					echo "  train: $$vtype/$$clip t$$t"; \
					$(1) "$$f" 0 /dev/null 0 $$t 0 >/dev/null || exit 1; \
				done; \
			else \
				echo "  SKIP (missing): $$f"; \
			fi; \
		done; \
	done
	@printf 'trained: %s\nclips:   %s\ncorpora: %s\nthreads: %s\n' \
		"$$(date -Is)" "$(PGO_TRAIN_CLIPS)" "$(PGO_TRAIN_TYPES)" "$(PGO_TRAIN_THREADS)" \
		> '$(2)/TRAINED_ON'
endef

# $(1) = profile directory. Proves the profile was actually produced — a silent
# no-op PGO otherwise looks identical to a successful one.
define pgo_check_profile
	@n=$$(find '$(1)' -name '$(PGO_PROFILE_GLOB)' | wc -l); \
	echo "  collected $$n profile files"; \
	if [ "$$n" -eq 0 ]; then echo "ERROR: no profile data — aborting"; exit 1; fi
endef

setup_ffmpeg_pgo: $(PLATFORM_GUARD)
	@echo "===== PGO 1/3: instrumented custom build ====="
	rm -rf '$(PGO_CUST_DIR)' && mkdir -p '$(PGO_CUST_DIR)'
	cd '$(CUSTOM_PREFIX)/FFmpeg' && \
	chmod +x ./configure ./ffbuild/*.sh && \
	$(call pgo_configure_cust_gen,$(PGO_CUST_DIR)) && \
	make clean && make -j"$$(nproc)" && make install
	$(call build_extractors,$(CUSTOM_PREFIX),$(TARGET_CUST),--features=custom_ffmpeg)
	@mkdir -p '$(EXECUTABLES_DIR_CUST)'
	cp '$(TARGET_CUST)/$(REL)/extractor5$(EXE_EXT)' '$(EXECUTABLES_DIR_CUST)/extractor5$(EXE_EXT)'
	$(call copy_runtime_libs,$(CUSTOM_PREFIX),$(EXECUTABLES_DIR_CUST))
	@echo "===== PGO 2/3: training run ====="
	$(call pgo_train_run,'$(CURRENT_DIR)/$(EXECUTABLES_DIR_CUST)/extractor5$(EXE_EXT)',$(PGO_CUST_DIR))
	$(call pgo_check_profile,$(PGO_CUST_DIR))
	@echo "===== PGO 3/3: optimized rebuild ====="
	cd '$(CUSTOM_PREFIX)/FFmpeg' && \
	$(call pgo_configure_cust_use,$(PGO_CUST_DIR)) && \
	make clean && make -j"$$(nproc)" && make install
	$(MAKE) build
	@echo "[OK]    PGO custom build complete."


# =============================================================================
# BUILD TARGETS
# =============================================================================

# The workspace pulls in ffmpeg-sys-next (via mv-extract), so it is built
# against the regular FFmpeg prefix rather than whatever incomplete/older
# FFmpeg happens to be on the system pkg-config path. See cargo_sys_env in
# mk/<platform>.mk for how that prefix is wired up.
build_tools: $(PLATFORM_GUARD)
	$(call cargo_sys_env,build --workspace $(CARGO_EXCLUDE) --release $(CARGO_TARGET_FLAG))

# Build every extractor twice: once against the regular FFmpeg and once against
# the custom patched FFmpeg. Extractors 0/1/2 are deployed from the system
# build; 3/5/6 from the custom build; extractor1 from the custom build is
# renamed to extractor4 (custom-FFmpeg flush-decoder variant), and extractor6
# from the system build becomes extractor7 (same trick).
build: $(PLATFORM_GUARD)
	$(call build_extractors,$(REGULAR_PREFIX),$(TARGET_SYS),)
	$(call build_extractors,$(CUSTOM_PREFIX),$(TARGET_CUST),--features=custom_ffmpeg)
	@mkdir -p '$(EXECUTABLES_DIR_SYS)' '$(EXECUTABLES_DIR_CUST)'
	cp '$(TARGET_SYS)/$(REL)/extractor0$(EXE_EXT)'  '$(EXECUTABLES_DIR_SYS)/extractor0$(EXE_EXT)'
	cp '$(TARGET_SYS)/$(REL)/extractor1$(EXE_EXT)'  '$(EXECUTABLES_DIR_SYS)/extractor1$(EXE_EXT)'
	cp '$(TARGET_SYS)/$(REL)/extractor2$(EXE_EXT)'  '$(EXECUTABLES_DIR_SYS)/extractor2$(EXE_EXT)'
	cp '$(TARGET_CUST)/$(REL)/extractor3$(EXE_EXT)' '$(EXECUTABLES_DIR_CUST)/extractor3$(EXE_EXT)'
	cp '$(TARGET_CUST)/$(REL)/extractor1$(EXE_EXT)' '$(EXECUTABLES_DIR_CUST)/extractor4$(EXE_EXT)'
	cp '$(TARGET_CUST)/$(REL)/extractor5$(EXE_EXT)' '$(EXECUTABLES_DIR_CUST)/extractor5$(EXE_EXT)'
	cp '$(TARGET_CUST)/$(REL)/extractor6$(EXE_EXT)' '$(EXECUTABLES_DIR_CUST)/extractor6$(EXE_EXT)'
	cp '$(TARGET_SYS)/$(REL)/extractor6$(EXE_EXT)'  '$(EXECUTABLES_DIR_SYS)/extractor7$(EXE_EXT)'
	$(call copy_runtime_libs,$(REGULAR_PREFIX),$(EXECUTABLES_DIR_SYS))
	$(call copy_runtime_libs,$(CUSTOM_PREFIX),$(EXECUTABLES_DIR_CUST))
	@echo "[OK]    Build complete."

# System-only build: every extractor links against the regular system FFmpeg.
# Useful for isolating whether a regression comes from the custom patch or from
# the extractor code itself. extractor4 here is just extractor1 under a
# different name — identical binary to method 1 — and method11 is extractor5
# again, which is the point of this target.
build_sys: $(PLATFORM_GUARD)
	$(call build_extractors,$(REGULAR_PREFIX),$(TARGET_SYS),)
	@mkdir -p '$(EXECUTABLES_DIR_SYS)' '$(EXECUTABLES_DIR_SLIM)'
	cp '$(TARGET_SYS)/$(REL)/extractor0$(EXE_EXT)' '$(EXECUTABLES_DIR_SYS)/extractor0$(EXE_EXT)'
	cp '$(TARGET_SYS)/$(REL)/extractor1$(EXE_EXT)' '$(EXECUTABLES_DIR_SYS)/extractor1$(EXE_EXT)'
	cp '$(TARGET_SYS)/$(REL)/extractor2$(EXE_EXT)' '$(EXECUTABLES_DIR_SYS)/extractor2$(EXE_EXT)'
	cp '$(TARGET_SYS)/$(REL)/extractor3$(EXE_EXT)' '$(EXECUTABLES_DIR_SYS)/extractor3$(EXE_EXT)'
	cp '$(TARGET_SYS)/$(REL)/extractor1$(EXE_EXT)' '$(EXECUTABLES_DIR_SYS)/extractor4$(EXE_EXT)'
	cp '$(TARGET_SYS)/$(REL)/extractor5$(EXE_EXT)' '$(EXECUTABLES_DIR_SYS)/extractor5$(EXE_EXT)'
	cp '$(TARGET_SYS)/$(REL)/extractor6$(EXE_EXT)' '$(EXECUTABLES_DIR_SYS)/extractor6$(EXE_EXT)'
	cp '$(TARGET_SYS)/$(REL)/extractor6$(EXE_EXT)' '$(EXECUTABLES_DIR_SYS)/extractor7$(EXE_EXT)'
	$(call copy_runtime_libs,$(REGULAR_PREFIX),$(EXECUTABLES_DIR_SYS))
	$(call copy_runtime_libs,$(REGULAR_PREFIX),$(EXECUTABLES_DIR_SLIM))

# =============================================================================
# BENCHMARKING
# =============================================================================

# L0_ONLY / COMPARE_FIRST / COMPARE_SECOND are inherited by every extractor
# child process via the spawned process environment — see extractor1/3/5/6.rs
# (mv_l0_only AVOption) and E9_L0_ONLY/E10_L0_ONLY relayed from it in
# crates/mv-bench/benchmark_extractors.rs.
BENCH_ENV = L0_ONLY=$(L0_ONLY) COMPARE_FIRST=$(COMPARE_FIRST) COMPARE_SECOND=$(COMPARE_SECOND) \
            MV_GRID=$(MV_GRID) MV_MIN_SIZE=$(MV_MIN_SIZE) \
            MV_SKIP_FRAME=$(MV_SKIP_FRAME) MV_SKIP_EVERY_NTH=$(MV_SKIP_EVERY_NTH)
BENCH_CMD = cargo run $(CARGO_TARGET_FLAG) --bin full_benchmark

all:
	$(MAKE) benchmark_all TYPE=sys
	$(MAKE) benchmark_all TYPE=cust

#   make benchmark_all VIDEO_NAME=bigbunny.mp4
#   make benchmark_all VIDEO_NAME=bigbunny.mp4 TYPE=sys
benchmark_all:
	@vname="$(VIDEO_NAME)"; \
	for vtype in $(VIDEO_TYPES); do \
		if [ "$$vtype" = "h264_avi" ]; then \
			filepath="$(CURRENT_DIR)/videos/$$vtype/$${vname%.*}.avi"; \
		else \
			filepath="$(CURRENT_DIR)/videos/$$vtype/$$vname"; \
		fi; \
		if [ -f "$$filepath" ]; then \
			echo ""; \
			echo "========== $$vtype / $$filepath =========="; \
			$(BENCH_ENV) $(BENCH_CMD) "$$filepath" $(STREAMS) $$vtype $(if $(TYPE),$(TYPE),cust) \
				$(NRUNS) $(THREAD_COUNT) $(KEYFRAMES_ONLY) $(WRITE_CSV) $(PROFILER_EXTRACTOR) 0; \
		else \
			echo "SKIP: $$filepath not found"; \
		fi; \
	done

# STEPS (optional) selects benchmark steps non-interactively (e.g. STEPS=2 to
# run only the extraction pass). Left empty, full_benchmark prompts on stdin —
# which is why BENCH_WRAPPER (winpty under MinGW) is applied only in that case.
benchmark:
	$(BENCH_ENV) $(if $(strip $(STEPS)),,$(BENCH_WRAPPER)) $(BENCH_CMD) '$(VIDEO_FILE)' \
		$(STREAMS) $(VIDEO_TYPE) cust $(NRUNS) $(THREAD_COUNT) $(KEYFRAMES_ONLY) \
		$(WRITE_CSV) $(PROFILER_EXTRACTOR) $(STEPS)

# Run the benchmark with keyframe-only decoding across every video in
# VIDEO_NAMES x VIDEO_TYPES.
benchmark_keyframes:
	@for vname in $(VIDEO_NAMES); do \
		for vtype in $(VIDEO_TYPES); do \
			if [ "$$vtype" = "h264_avi" ]; then \
				filepath="$(CURRENT_DIR)/videos/$$vtype/$${vname%.*}.avi"; \
			else \
				filepath="$(CURRENT_DIR)/videos/$$vtype/$$vname"; \
			fi; \
			if [ -f "$$filepath" ]; then \
				echo ""; \
				echo "========== $$vname / $$vtype (keyframes only) =========="; \
				$(BENCH_ENV) $(BENCH_CMD) "$$filepath" $(STREAMS) $$vtype cust \
					$(NRUNS) $(THREAD_COUNT) 1 $(WRITE_CSV) $(PROFILER_EXTRACTOR) 4; \
			fi; \
		done; \
	done

# Sweep thread counts 1->2->4->...->MAX_THREADS across every video in
# VIDEO_NAMES x VIDEO_TYPES. Useful for understanding multi-thread scaling.
# Override: make benchmark_threads MAX_THREADS=16
benchmark_threads:
	@t=1; while [ $$t -le $(MAX_THREADS) ]; do \
		echo ""; \
		echo "========================================"; \
		echo "  THREAD COUNT = $$t"; \
		echo "========================================"; \
		for vname in $(VIDEO_NAMES); do \
			for vtype in $(VIDEO_TYPES); do \
				if [ "$$vtype" = "h264_avi" ]; then \
					filepath="$(CURRENT_DIR)/videos/$$vtype/$${vname%.*}.avi"; \
				else \
					filepath="$(CURRENT_DIR)/videos/$$vtype/$$vname"; \
				fi; \
				if [ -f "$$filepath" ]; then \
					echo ""; \
					echo "--- $$vname / $$vtype ---"; \
					$(BENCH_ENV) $(BENCH_CMD) "$$filepath" $(STREAMS) $$vtype cust \
						$(NRUNS) $$t $(KEYFRAMES_ONLY) $(WRITE_CSV) $(PROFILER_EXTRACTOR) 4 6; \
				fi; \
			done; \
		done; \
		t=$$((t * 2)); \
	done

# =============================================================================
# DEVELOPMENT & TESTING TOOLS
# =============================================================================

# Run the workspace test suite. Mirrors build_tools' FFmpeg env so the
# ffmpeg-linking crates compile and the test binaries resolve libs at runtime.
# There are no tests yet, so today this just compiles the workspace and runs
# zero tests — it's the placeholder the future suite will hang off of.
test:
	$(call cargo_sys_env,test --workspace $(CARGO_EXCLUDE) $(CARGO_TARGET_FLAG))

test_ffmpeg:
	$(call FFMPEG_BUILD,$(CUSTOM_PREFIX))
	cargo run --bin full_benchmark $(VIDEO_FILE) $(STREAMS) $(VIDEO_TYPE) cust $(NRUNS) $(THREAD_COUNT) $(KEYFRAMES_ONLY) $(WRITE_CSV) $(PROFILER_EXTRACTOR) 1 2 5
#   chromium --no-sandbox $(shell ls -d $(CURRENT_DIR)/results/$(VIDEO_TYPE)/* | sort | tail -n 1)/vtune_results/call_tree.html

decode_ffmpeg:
	LD_LIBRARY_PATH=$(CUSTOM_PREFIX)/lib:$$LD_LIBRARY_PATH $(CUSTOM_PREFIX)/bin/ffmpeg -y -i $(VIDEO_FILE) -c copy -an $(LAST_RESULTS_DIR)/decoded_output.mp4

# =============================================================================
# PUBLISHING & VIDEO GENERATION
# =============================================================================

publish:
	$(BENCH_WRAPPER) cargo run $(CARGO_TARGET_FLAG) --bin publish_report 3 \
		'$(INITIAL_RUN_DATA)' '$(LAST_RESULTS_DIR)' $(VIDEO_TYPE) test_git test_git 1

generate_video:
	cargo run $(CARGO_TARGET_FLAG) --bin generate_motion_vectors_video '$(CSV_FILE_PATH_CUST)' '$(LAST_RESULTS_DIR)'
	cargo run $(CARGO_TARGET_FLAG) --bin combine_motion_vectors_with_video '$(VIDEO_FILE)' \
		'$(CSV_FILE_PATH_ORIG)' '$(CSV_FILE_PATH_CUST)' '$(LAST_RESULTS_DIR)'

# Same as generate_video, but for every results dir at/after a given time
# instead of just the newest one.
#   make generate_videos_since                # today from 09:44
#   make generate_videos_since SINCE=1400     # today from 14:00
#   make generate_videos_since SINCE_DAY=20260726 SINCE=0000
#
# Each run dir encodes its source video, so the video type, stem and the
# matching file under videos/ are derived per-dir rather than taken from
# VIDEO_TYPE/VIDEO_FILE. The "custom" CSV is whichever of CUST_METHODS the run
# actually produced (a run driven at method5 has no method4 CSV), so this
# doesn't assume the CSV_FILE_PATH_CUST default.
SINCE        ?= 0944
SINCE_DAY    ?= $(shell date +%Y%m%d)
CUST_METHODS ?= 4 5 3

generate_videos_since:
	@n=0; skipped=0; \
	for d in $(CURRENT_DIR)/results/*/$(SINCE_DAY)_*/; do \
		[ -d "$$d" ] || continue; \
		d=$${d%/}; b=$$(basename $$d); \
		hhmm=$$(echo "$$b" | cut -d_ -f2); \
		[ "$$(echo "$$hhmm $(SINCE)" | awk '{print ($$1 < $$2)}')" = 1 ] && continue; \
		vtype=$$(basename $$(dirname $$d)); \
		stem=$$(echo "$$b" | sed -E 's/^[0-9]{8}_[0-9]{4}_//; s/_t[0-9]+(_kf)?(_csv)?$$//'); \
		vid=$$(ls $(CURRENT_DIR)/videos/$$vtype/$$stem.* 2>/dev/null | head -n 1); \
		orig=$$d/method0_output_0.csv; \
		cust=; for m in $(CUST_METHODS); do \
			c=$$d/method$${m}_output_0.csv; \
			if [ -f "$$c" ]; then cust=$$c; break; fi; \
		done; \
		if [ -z "$$cust" ]; then echo "skip $$b: no custom CSV (tried methods $(CUST_METHODS))"; skipped=$$((skipped+1)); continue; fi; \
		if [ -z "$$vid" ]; then echo "skip $$b: no source video at videos/$$vtype/$$stem.*"; skipped=$$((skipped+1)); continue; fi; \
		echo "=== $$vtype/$$b  (custom=$$(basename $$cust)) ==="; \
		cargo run $(CARGO_TARGET_FLAG) --bin generate_motion_vectors_video $$cust $$d || exit 1; \
		if [ -f "$$orig" ]; then \
			cargo run $(CARGO_TARGET_FLAG) --bin combine_motion_vectors_with_video $$vid $$orig $$cust $$d || exit 1; \
		else \
			echo "  no method0 CSV, skipping side-by-side combine"; \
		fi; \
		n=$$((n+1)); \
	done; \
	echo "generate_videos_since: generated for $$n run(s), skipped $$skipped"

compare_mvs:
	LD_LIBRARY_PATH=$(REGULAR_PREFIX)/lib:$$LD_LIBRARY_PATH \
		'$(REGULAR_PREFIX)/bin/ffprobe$(EXE_EXT)' -v error -select_streams v:0 \
		-show_entries packet=pts_time -of csv=p=0 '$(VIDEO_FILE)' \
		> '$(LAST_RESULTS_DIR)/pkt_order.txt'
	cargo run --release $(CARGO_TARGET_FLAG) --bin mv_diff_driver -- \
		'$(CSV_FILE_PATH_ORIG)' '$(CSV_FILE_PATH_CUST)' \
		'$(LAST_RESULTS_DIR)/pkt_order.txt' '$(LAST_RESULTS_DIR)/mv_diff_neg1.txt'

		$(REGULAR_PREFIX)/bin/ffprobe -v error -select_streams v:0 -show_entries packet=pts_time \
		-of csv=p=0 $(VIDEO_FILE) > $(LAST_RESULTS_DIR)/pkt_order.txt
	cargo run --release --bin mv_diff_driver -- \
		$(CSV_FILE_PATH_ORIG) $(CSV_FILE_PATH_CUST) \
		$(LAST_RESULTS_DIR)/pkt_order.txt $(LAST_RESULTS_DIR)/mv_diff_neg1.txt

# =============================================================================
# INSTALLER DIFF GENERATION
# =============================================================================

FFMPEG_INSTALLER_DIR := $(CURRENT_DIR)/ffmpeg_installer
# MSYS2 maps /tmp to its own tmp dir; the same path works on both platforms.
FRESH_FFMPEG_DIR     ?= /tmp/ffmpeg-8.0-fresh
FFMPEG_BRANCH        ?= release/8.0

fetch_fresh_ffmpeg:
	@if [ ! -d "$(FRESH_FFMPEG_DIR)" ]; then \
		echo "Cloning fresh FFmpeg $(FFMPEG_BRANCH)..."; \
		git clone --depth 1 --branch $(FFMPEG_BRANCH) \
			https://github.com/FFmpeg/FFmpeg.git $(FRESH_FFMPEG_DIR); \
	else \
		echo "Fresh FFmpeg already at $(FRESH_FFMPEG_DIR)"; \
	fi

installer_diff: fetch_fresh_ffmpeg
	@echo "Generating diff: fresh $(FFMPEG_BRANCH) → custom..."
	diff -u -I '/tmp/ffconf\.' \
		-x '.git' \
		-x 'config.h' \
		-x 'config_components.h' \
		-x '*tests' \
		-x '*.pc' \
		-x 'ffmpeg_g' \
		-x 'ffprobe_g' \
		-x 'ffmpeg' \
		-x 'ffprobe' \
		-x '.version' \
		-x '*.so' \
		-x '*.so.*' \
		-x '*.ver.*' \
		-x '*.dll' \
		-x '*.dll.a' \
		-x '*.exe' \
		-x '*.a' \
		-x '*.o' \
		-x '*.d' \
		-x '*.S' \
		-x '*.asm' \
		-x 'doc' \
		-x 'ffversion.h' \
		-r $(FRESH_FFMPEG_DIR)/ $(CUSTOM_PREFIX)/FFmpeg/ \
		| sed 's|$(FRESH_FFMPEG_DIR)/|a/|g' \
		| sed 's|$(CUSTOM_PREFIX)/FFmpeg/|b/|g' \
		| sed '/Binary\ files\ /d' \
		| grep -v '^Only in b/' \
		> $(FFMPEG_INSTALLER_DIR)/custom_ffmpeg.diff \
		|| true
	@echo "Diff written to $(FFMPEG_INSTALLER_DIR)/custom_ffmpeg.diff"
	@echo "$$(grep -c '^diff ' $(FFMPEG_INSTALLER_DIR)/custom_ffmpeg.diff) file(s) changed"

# Convenience: generate diff + stage it in the submodule
installer_publish: installer_diff
	cd '$(FFMPEG_INSTALLER_DIR)' && git add ffmpeg_version.diff && git status
	@echo "Diff staged in ffmpeg-installer. Commit when ready."

# Nuke the cached fresh clone (forces re-download next time)
clean_fresh_ffmpeg:
	rm -rf '$(FRESH_FFMPEG_DIR)'
# =============================================================================
# HELP
# =============================================================================

help:
	@echo ""
	@echo "  motion-vector-extractors — platform: $(PLATFORM)"
	@echo ""
	@echo "    $(MAKE_HINT) install                 # toolchain + dependencies"
	@echo "    $(MAKE_HINT) setup_ffmpeg            # build all three FFmpeg trees (sys + custom + slim)"
	@echo "    $(MAKE_HINT) setup_ffmpeg_pgo        # PGO the custom fork (instrument -> train -> rebuild)"
	@echo "    $(MAKE_HINT) build                   # build all extractors (sys + custom + slim)"
	@echo "    $(MAKE_HINT) build_sys               # build extractors against the regular FFmpeg only"
	@echo "    $(MAKE_HINT) build_tools             # cargo build --workspace --release"
	@echo "    $(MAKE_HINT) test                    # cargo test --workspace"
	@echo ""
	@echo "    $(MAKE_HINT) benchmark               # single-video benchmark"
	@echo "    $(MAKE_HINT) benchmark_all           # iterate over VIDEO_TYPES"
	@echo "    $(MAKE_HINT) all                     # benchmark_all for sys + cust"
	@echo "    $(MAKE_HINT) benchmark_keyframes     # all videos, keyframes-only mode"
	@echo "    $(MAKE_HINT) benchmark_threads       # all videos, sweep thread counts"
	@echo ""
	@echo "    $(MAKE_HINT) publish                 # publish report"
	@echo "    $(MAKE_HINT) generate_video          # render MV overlay video"
	@echo "    $(MAKE_HINT) generate_videos_since   # same, for every recent results dir"
	@echo "    $(MAKE_HINT) compare_mvs             # method0 vs method9 MV diff"
	@echo "    $(MAKE_HINT) compare_runs            # reproducibility check across runs"
	@echo "    $(MAKE_HINT) decode_ffmpeg           # decode VIDEO_FILE via the custom FFmpeg"
	@echo "    $(MAKE_HINT) installer_diff          # regenerate custom_ffmpeg.diff"
	@echo ""
	@echo "  Vars: VIDEO_NAME=$(VIDEO_NAME)"
	@echo "        VIDEO_TYPE=$(VIDEO_TYPE)  STREAMS=$(STREAMS)  NRUNS=$(NRUNS)  THREAD_COUNT=$(THREAD_COUNT)"
	@echo "        KEYFRAMES_ONLY=$(KEYFRAMES_ONLY)  WRITE_CSV=$(WRITE_CSV)  L0_ONLY=$(L0_ONLY)"
	@echo "        MV_GRID=$(MV_GRID)  MV_MIN_SIZE=$(MV_MIN_SIZE)   # export filters (no decode saving)"
	@echo "        MV_SKIP_FRAME=$(MV_SKIP_FRAME)  MV_SKIP_EVERY_NTH=$(MV_SKIP_EVERY_NTH)   # temporal decimation (real speedup)"
	@echo "        COMPARE_FIRST=$(COMPARE_FIRST)  COMPARE_SECOND=$(COMPARE_SECOND)  PROFILER_EXTRACTOR=$(PROFILER_EXTRACTOR)"
	@echo ""
	@echo "  PGO vars: PGO_TRAIN_CLIPS=$(PGO_TRAIN_CLIPS)"
	@echo "            PGO_TRAIN_TYPES=$(PGO_TRAIN_TYPES)  PGO_TRAIN_THREADS=$(PGO_TRAIN_THREADS)"
	@echo "    e.g. $(MAKE_HINT) setup_ffmpeg_pgo PGO_TRAIN_TYPES=h264_cabac PGO_TRAIN_THREADS=1"
	@echo ""
ifneq ($(PLATFORM),linux)
	@echo "  Notes:"
	@echo "    - VTune (profiler) and perf (flamegraph) are Linux-only and skipped on Windows."
	@echo "    - DLL resolution uses PATH, not rpath: build copies the FFmpeg DLLs into"
	@echo "      $(EXECUTABLES_DIR)/{sys,cust,slim}/ so the .exe files run in place."
	@echo ""
endif

.PHONY: install platform_install \
        setup_ffmpeg setup_ffmpeg_pgo \
        build build_sys build_tools \
        all benchmark benchmark_all benchmark_keyframes benchmark_threads \
        test test_ffmpeg decode_ffmpeg \
        publish generate_video generate_videos_since compare_mvs compare_runs \
        fetch_fresh_ffmpeg installer_diff installer_publish clean_fresh_ffmpeg \
        help $(PLATFORM_PHONY)
