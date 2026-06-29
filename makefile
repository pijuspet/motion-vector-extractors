# =============================================================================
# CONFIGURATION & GLOBAL VARIABLES
# =============================================================================

STREAMS = 15
NRUNS = 3
# Set KEYFRAMES_ONLY=1 to have every extractor decode I-frames only.
KEYFRAMES_ONLY ?= 0
# Set THREAD_COUNT=N to pin every extractor to N threads (0 = FFmpeg auto).
THREAD_COUNT ?= 28
# Set WRITE_CSV=0 to skip writing per-extractor MV output CSV files.
WRITE_CSV ?= 0
# Extractor number to profile with perf/VTune (step 5/6).
PROFILER_EXTRACTOR ?= 4

# VIDEO_NAME ?= bigbunny.mp4
# VIDEO_NAME ?= stickman.mp4
# VIDEO_NAME ?= dashcam.mp4
# VIDEO_NAME ?= 2018-03-05.09-50-15.09-55-01.school.G423.r13.mp4
# VIDEO_NAME ?= 2018-03-15.15-55-00.16-00-00.bus.G475.r13.mp4
# VIDEO_NAME ?= MCTTR0102b.mp4
VIDEO_NAME ?= bigbunny_walking.mp4

# All video names iterated by benchmark_keyframes and benchmark_threads.
# Files that don't exist for a given type are silently skipped.
# 2018-03-05.09-50-15.09-55-01.school.G423.r13.mp4 
VIDEO_NAMES ?= 2018-03-15.15-55-00.16-00-00.bus.G475.r13.mp4 \
               MCTTR0102b.mp4 
			#    bigbunny_walking.mp4 \
            #    stickman.mp4 \
            #    dashcam.mp4 

VIDEO_TYPES := h264_cabac h264_cavlc h264_avi h265
VIDEO_TYPE = h264_cabac
# VIDEO_TYPE = h264_cavlc
# VIDEO_TYPE = h264_avi
# VIDEO_TYPE = h265

CC = g++

# =============================================================================
# PATHS & ENVIRONMENTS
# =============================================================================

CURRENT_DIR := ${shell pwd}
PARENT_DIR  := $(shell dirname $(CURRENT_DIR))
VENV_FOLDER = $(PARENT_DIR)/venv-motion-vectors
PYTHON = $(VENV_FOLDER)/bin/python

EXECUTABLES_DIR = executables

VIDEO_FILE = $(CURRENT_DIR)/videos/$(VIDEO_TYPE)/$(VIDEO_NAME)

INITIAL_RUN_DATA = $(CURRENT_DIR)/published/$(VIDEO_TYPE)/initial_results_$(VIDEO_TYPE)
LAST_RESULTS_DIR = $(shell ls -d $(CURRENT_DIR)/results/$(VIDEO_TYPE)/* | sort | tail -n 1)

CSV_FILE_PATH_ORIG = $(LAST_RESULTS_DIR)/method0_output_0.csv # original ffmpeg
CSV_FILE_PATH_CUST = $(LAST_RESULTS_DIR)/method4_output_0.csv # custom ffmpeg

# =============================================================================
# FFMPEG & PKG-CONFIG SETUP
# =============================================================================

FF_PKGS := libavformat libavcodec libavutil libswresample
pkg_cmd = PKG_CONFIG_PATH=$(1)/lib/pkgconfig pkg-config

# Functions to extract flags, libs, and rpath
get_cflags = $(shell $(call pkg_cmd,$(1)) --cflags $(FF_PKGS) 2>/dev/null)
get_libs   = $(shell $(call pkg_cmd,$(1)) --libs $(FF_PKGS) 2>/dev/null)
get_rpath  = -Wl,-rpath,$(1)/lib -Wl,--disable-new-dtags

define def_ff_flags
$(2)_CFLAGS := $$(call get_cflags,$(1))
$(2)_LIBS   := $$(call get_libs,$(1))
$(2)_RPATH  := $$(call get_rpath,$(1))
$(2)        := $$($(2)_CFLAGS) $$($(2)_LIBS) $$($(2)_RPATH)
endef

CUSTOM_PREFIX   := $(abspath $(CURRENT_DIR)/ffmpeg/FFmpeg-8.0-custom)
REGULAR_PREFIX  := $(abspath $(CURRENT_DIR)/ffmpeg/FFmpeg-8.0)
$(eval $(call def_ff_flags,$(CUSTOM_PREFIX),CUST_FF))
$(eval $(call def_ff_flags,$(REGULAR_PREFIX),SYS_FF))

# Slimmed component set — only what the extractors actually use. Validated to
# produce byte-identical MV output to a full build across h264/hevc/mpeg4.
#   - mpeg4 decoder is REQUIRED: the "h264_avi" inputs are really MPEG-4 Part 2,
#     and h264's MV export is compile-gated behind CONFIG_MPEGVIDEODEC, which an
#     mpegvideo decoder (mpeg4) turns on — without it h264 exports zero MVs.
#   - rtsp/sdp are demuxers (RTSP rides on the rtp/tcp/udp protocols).
SLIM_FFMPEG := --disable-everything \
	--enable-decoder=h264,hevc,mpeg4 \
	--enable-parser=h264,hevc,mpeg4video \
	--enable-demuxer=mov,avi,h264,hevc,mpegts,rtsp,sdp \
	--enable-muxer=mov \
	--enable-protocol=file,rtp,tcp,udp \
	--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,extract_extradata

# FFmpeg build macro
FFMPEG_BUILD = \
	cd $1/FFmpeg && \
	chmod +x ./configure ./ffbuild/*.sh && \
	./configure --prefix=$(abspath $1) --enable-shared --enable-swresample --enable-debug --disable-stripping --disable-doc $(SLIM_FFMPEG) --pkg-config-flags="--static" && \
	make -j"$$(nproc)" && make install

# =============================================================================
# INSTALLATION & DEPENDENCIES
# =============================================================================

# CI (GitHub Actions sets CI=true) skips profiler/report-only tooling so a build
# can be verified without VTune/perf. SUDO is empty for local root runs and set
# to `sudo` by CI.
SUDO ?=

# Packages required to build + benchmark. Rust is bootstrapped separately via
# rustup (see the install recipe) — the apt `cargo`/`rustup` packages conflict
# with each other on recent Ubuntu, so they're intentionally not listed here.
APT_CORE  := build-essential gcc g++ make pkg-config nasm libclang-dev libopencv-dev clang
# Profiler / report-generation extras (perf, notifications, pdf/plot rendering).
APT_EXTRA := xdg-utils libnss3 libnotify4 wkhtmltopdf linux-tools-common linux-tools-realtime

install_vtune:
ifndef CI
	@echo "Adding Intel oneAPI repository..."
	wget -O- https://apt.repos.intel.com/intel-gpg-keys/GPG-PUB-KEY-INTEL-SW-PRODUCTS.PUB \
		| gpg --dearmor \
		| sudo tee /usr/share/keyrings/oneapi-archive-keyring.gpg > /dev/null
	echo "deb [signed-by=/usr/share/keyrings/oneapi-archive-keyring.gpg] \
		https://apt.repos.intel.com/oneapi all main" \
		| sudo tee /etc/apt/sources.list.d/oneAPI.list
	apt update
	apt install -y intel-oneapi-vtune
	@echo "Enabling ptrace for VTune..."
	sysctl -w kernel.yama.ptrace_scope=0
	@if grep -q "kernel.yama.ptrace_scope" /etc/sysctl.d/10-ptrace.conf 2>/dev/null; then \
		sed -i 's/kernel.yama.ptrace_scope = .*/kernel.yama.ptrace_scope = 0/' /etc/sysctl.d/10-ptrace.conf; \
	else \
		echo "kernel.yama.ptrace_scope = 0" >> /etc/sysctl.d/10-ptrace.conf; \
	fi
	sysctl -p /etc/sysctl.d/10-ptrace.conf
	@echo "VTune installation complete."
else
	@echo "[CI] Skipping VTune / oneAPI install."
endif

install: install_vtune
	command -v cargo >/dev/null 2>&1 || curl https://sh.rustup.rs -sSf | sh -s -- -y
	$(SUDO) apt install -y $(APT_CORE)
	cp -n .env_template .env
	mkdir -p $(EXECUTABLES_DIR)
ifndef CI
	$(SUDO) apt install -y $(APT_EXTRA)
	mkdir -p $(VENV_FOLDER)
	python3 -m venv $(VENV_FOLDER)
	. $(VENV_FOLDER)/bin/activate && pip install -r requirements.txt
endif

setup_ffmpeg:
	$(call FFMPEG_BUILD,$(CUSTOM_PREFIX))
	$(call FFMPEG_BUILD,$(REGULAR_PREFIX))

# =============================================================================
# BUILD TARGETS
# =============================================================================
# Point pkg-config at the built regular FFmpeg prefix (and bake its rpath) so
# the workspace's ffmpeg-sys-next links against FFmpeg 8.0 instead of whatever
# incomplete/older FFmpeg happens to be on the system pkg-config path.
build_tools:
	PKG_CONFIG_PATH=$(REGULAR_PREFIX)/lib/pkgconfig \
	RUSTFLAGS="-C link-arg=-Wl,-rpath,$(REGULAR_PREFIX)/lib -C link-arg=-Wl,--disable-new-dtags" \
	cargo build --workspace --release

TARGET_SYS  := $(CURRENT_DIR)/target/extractor-sys
TARGET_CUST := $(CURRENT_DIR)/target/extractor-cust

# $(1) = FFmpeg prefix, $(2) = CARGO_TARGET_DIR to use, $(3) = extra cargo flags
# Builds every extractor in the mv-extract crate linked against the given
# FFmpeg prefix. $(3) is used to enable the `custom_ffmpeg` Cargo feature when
# linking against the patched FFmpeg — that feature gates access to
# AVMotionVectorCompact / AV_FRAME_DATA_MOTION_VECTORS_COMPACT, which only
# exist in the custom build.
define build_extractors
	PKG_CONFIG_PATH=$(1)/lib/pkgconfig \
	RUSTFLAGS="-C link-arg=-Wl,-rpath,$(1)/lib -C link-arg=-Wl,--disable-new-dtags" \
	CARGO_TARGET_DIR=$(2) \
	cargo build --release -p mv-extract $(3)
endef

# Build every extractor twice: once against the regular FFmpeg and once
# against the custom patched FFmpeg. Extractors 0/1/2 are deployed from the
# system build; 3/5 from the custom build; extractor1 from the custom build
# is renamed to extractor4 (custom-FFmpeg flush-decoder variant).
build:
	$(call build_extractors,$(REGULAR_PREFIX),$(TARGET_SYS),)
	$(call build_extractors,$(CUSTOM_PREFIX),$(TARGET_CUST),--features=custom_ffmpeg)
	cp $(TARGET_SYS)/release/extractor0  $(EXECUTABLES_DIR)/extractor0
	cp $(TARGET_SYS)/release/extractor1  $(EXECUTABLES_DIR)/extractor1
	cp $(TARGET_SYS)/release/extractor2  $(EXECUTABLES_DIR)/extractor2
	cp $(TARGET_CUST)/release/extractor3 $(EXECUTABLES_DIR)/extractor3
	cp $(TARGET_CUST)/release/extractor1 $(EXECUTABLES_DIR)/extractor4
	cp $(TARGET_CUST)/release/extractor5 $(EXECUTABLES_DIR)/extractor5
	cp $(TARGET_CUST)/release/extractor6 $(EXECUTABLES_DIR)/extractor6

# System-only build: every extractor links against the regular system FFmpeg.
# Useful for isolating whether a regression comes from the custom patch or
# from the extractor code itself. extractor4 here is just extractor1 under a
# different name — identical binary to method 1.
build_sys:
	$(call build_extractors,$(REGULAR_PREFIX),$(TARGET_SYS),)
	cp $(TARGET_SYS)/release/extractor0 $(EXECUTABLES_DIR)/extractor0
	cp $(TARGET_SYS)/release/extractor1 $(EXECUTABLES_DIR)/extractor1
	cp $(TARGET_SYS)/release/extractor2 $(EXECUTABLES_DIR)/extractor2
	cp $(TARGET_SYS)/release/extractor3 $(EXECUTABLES_DIR)/extractor3
	cp $(TARGET_SYS)/release/extractor1 $(EXECUTABLES_DIR)/extractor4
	cp $(TARGET_SYS)/release/extractor5 $(EXECUTABLES_DIR)/extractor5

# =============================================================================
# BENCHMARKING
# =============================================================================

all:
	$(MAKE) benchmark_all TYPE=sys
	$(MAKE) benchmark_all TYPE=cust

# make benchmark_all VIDEO_NAME=bigbunny.mp4
# make benchmark_all VIDEO_NAME=bigbunny.mp4 TYPE=sys
benchmark_all:
	@vname="$(VIDEO_NAME)"; \
	for vtype in $(VIDEO_TYPES); do \
		if [ "$$vtype" = "h264_avi" ]; then \
			filepath=$(CURRENT_DIR)/videos/$$vtype/$${vname%.*}.avi; \
		else \
			filepath=$(CURRENT_DIR)/videos/$$vtype/$$vname; \
		fi; \
		if [ -f "$$filepath" ]; then \
			echo "\n========== $$vtype / $$filepath =========="; \
			if [ -z "$(TYPE)" ]; then \
				cargo run --bin full_benchmark $$filepath $(STREAMS) $$vtype cust $(NRUNS) $(THREAD_COUNT) $(KEYFRAMES_ONLY) $(WRITE_CSV) $(PROFILER_EXTRACTOR) 0; \
			else \
				cargo run --bin full_benchmark $$filepath $(STREAMS) $$vtype $(TYPE) $(NRUNS) $(THREAD_COUNT) $(KEYFRAMES_ONLY) $(WRITE_CSV) $(PROFILER_EXTRACTOR) 0; \
			fi; \
		else \
			echo "SKIP: $$filepath not found"; \
		fi; \
	done

# STEPS (optional) selects benchmark steps non-interactively (e.g. STEPS=2 to
# run only the extraction pass). Left empty, full_benchmark prompts on stdin.
benchmark:
	cargo run --bin full_benchmark $(VIDEO_FILE) $(STREAMS) $(VIDEO_TYPE) cust $(NRUNS) $(THREAD_COUNT) $(KEYFRAMES_ONLY) $(WRITE_CSV) $(PROFILER_EXTRACTOR) $(STEPS)

# Run the benchmark with keyframe-only decoding across every video in
# VIDEO_NAMES × VIDEO_TYPES. KEYFRAMES_ONLY=1 is inherited by every extractor
# child process via fork+exec environment inheritance.
benchmark_keyframes:
	@for vname in $(VIDEO_NAMES); do \
		for vtype in $(VIDEO_TYPES); do \
			if [ "$$vtype" = "h264_avi" ]; then \
				filepath=$(CURRENT_DIR)/videos/$$vtype/$${vname%.*}.avi; \
			else \
				filepath=$(CURRENT_DIR)/videos/$$vtype/$$vname; \
			fi; \
			if [ -f "$$filepath" ]; then \
				echo "\n========== $$vname / $$vtype (keyframes only) =========="; \
				cargo run --bin full_benchmark \
					$$filepath $(STREAMS) $$vtype cust $(NRUNS) $(THREAD_COUNT) 1 $(WRITE_CSV) $(PROFILER_EXTRACTOR) 4; \
			fi; \
		done; \
	done

# Sweep thread counts 1→2→4→…→MAX_THREADS across every video in
# VIDEO_NAMES × VIDEO_TYPES. Useful for understanding multi-thread scaling.
# Override: make benchmark_threads MAX_THREADS=16
MAX_THREADS ?= $(shell nproc)
benchmark_threads:
	@t=1; while [ $$t -le $(MAX_THREADS) ]; do \
		echo "\n========================================"; \
		echo "  THREAD COUNT = $$t"; \
		echo "========================================"; \
		for vname in $(VIDEO_NAMES); do \
			for vtype in $(VIDEO_TYPES); do \
				if [ "$$vtype" = "h264_avi" ]; then \
					filepath=$(CURRENT_DIR)/videos/$$vtype/$${vname%.*}.avi; \
				else \
					filepath=$(CURRENT_DIR)/videos/$$vtype/$$vname; \
				fi; \
				if [ -f "$$filepath" ]; then \
					echo "\n--- $$vname / $$vtype ---"; \
					cargo run --bin full_benchmark \
						$$filepath $(STREAMS) $$vtype cust $(NRUNS) $$t $(KEYFRAMES_ONLY) $(WRITE_CSV) $(PROFILER_EXTRACTOR) 4 6; \
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
	PKG_CONFIG_PATH=$(REGULAR_PREFIX)/lib/pkgconfig \
	RUSTFLAGS="-C link-arg=-Wl,-rpath,$(REGULAR_PREFIX)/lib -C link-arg=-Wl,--disable-new-dtags" \
	cargo test --workspace

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
	cargo run --bin publish_report 3 $(INITIAL_RUN_DATA) $(LAST_RESULTS_DIR) $(VIDEO_TYPE) test_git test_git 1

generate_video:
	cargo run --bin generate_motion_vectors_video $(CSV_FILE_PATH_CUST) $(LAST_RESULTS_DIR)
	cargo run --bin combine_motion_vectors_with_video $(VIDEO_FILE) $(CSV_FILE_PATH_ORIG) $(CSV_FILE_PATH_CUST) $(LAST_RESULTS_DIR)

# =============================================================================
# INSTALLER DIFF GENERATION
# =============================================================================

FFMPEG_INSTALLER_DIR = $(CURRENT_DIR)/ffmpeg_installer
FRESH_FFMPEG_DIR     = /tmp/ffmpeg-8.0-fresh
FFMPEG_BRANCH        = release/8.0

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
	cd $(FFMPEG_INSTALLER_DIR) && git add ffmpeg_version.diff && \
		git status
	@echo "Diff staged in ffmpeg-installer. Commit when ready."

# Nuke the cached fresh clone (forces re-download next time)
clean_fresh_ffmpeg:
	rm -rf $(FRESH_FFMPEG_DIR)