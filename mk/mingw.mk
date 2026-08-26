# =============================================================================
# Platform: Windows / MSYS2 MINGW64 (GCC toolchain, MinGW runtime)
# =============================================================================
# Included by the top-level makefile when PLATFORM=mingw (auto-detected from a
# MINGW/MSYS `uname -s`). Run from the "MSYS2 MinGW x64" shell — NOT plain
# MSYS, NOT cmd, NOT PowerShell.
#
# Linux-only paths (VTune, perf, /proc, LD_LIBRARY_PATH, apt) are absent here;
# the corresponding Rust targets (profiler, flamegraph) are gated behind
# cfg(unix) and print a notice on Windows instead of running.
# =============================================================================

EXE_EXT := .exe
REL     := release

# On Windows there is no rpath: DLLs are loaded from the executable's own
# directory. Separate sys/ cust/ slim/ subdirs carry the matching FFmpeg DLLs
# so each extractor loads the runtime it was linked against. Method 11 links
# the pruned slim tree, whose DLLs differ from the full fork's, so it needs its
# own directory rather than cust/. Mirrors MethodInfo::exe_path()'s
# #[cfg(windows)] branch in crates/mv-bench/benchmark_extractors.rs.
EXECUTABLES_DIR_SYS  := $(EXECUTABLES_DIR)/sys
EXECUTABLES_DIR_CUST := $(EXECUTABLES_DIR)/cust
EXECUTABLES_DIR_SLIM := $(EXECUTABLES_DIR)/slim

# Cargo target dir must be on a space-free path; dlltool/as split on spaces.
CARGO_TARGET_BASE := $(HOME)/cargo-target/motion-vector-extractors
VENV_FOLDER       ?= $(CURRENT_DIR)/venv-motion-vectors
VENV_WIN          := $(shell cygpath -m '$(VENV_FOLDER)' 2>/dev/null)

export CARGO_TARGET_DIR := $(CARGO_TARGET_BASE)/tools
# pkg-config must receive POSIX colon-separated paths; Rust build scripts emit
# Windows semicolon-separated paths that MSYS2 pkg-config mis-parses.
export PKG_CONFIG_PATH := /mingw64/lib/pkgconfig:/mingw64/share/pkgconfig
# bindgen (via ffmpeg-sys-next) must parse the MinGW system headers with MinGW's
# own libclang + the GNU target. Otherwise clang-sys can pick up an MSVC-targeted
# libclang (e.g. C:\Program Files\LLVM on CI runners), which parses the headers
# with the wrong predefined macros and fails ("expected ';'" in stdlib.h). Use a
# Windows-style path so the (native) clang-sys can resolve it.
export LIBCLANG_PATH := $(shell cygpath -m '$(MINGW_PREFIX)/bin')
export BINDGEN_EXTRA_CLANG_ARGS := --target=x86_64-w64-windows-gnu

CARGO_TARGET_FLAG :=
# mv-video is the only crate pulling in the `opencv` crate (MV-visualization
# video), and opencv's build script probes for a *system* OpenCV install.
# `install` does pacman mingw-w64-x86_64-opencv, but the probe still fails under
# CI: MSYS2 rewrites PKG_CONFIG_PATH into Windows form when it spawns the native
# cargo.exe, and the drive-letter colon in "D:\..." then shreds the path list,
# so opencv4.pc is never found:
#     PKG_CONFIG_PATH contains the following:
#         - D
#         - \a\_temp\msys64\mingw64\lib\pkgconfig;D
# Nothing in the workspace depends on mv-video (it is a leaf crate) and it is
# not part of the extractor deliverable, so excluding it costs nothing. To build
# it locally, run `cargo build -p mv-video` directly with OPENCV_INCLUDE_PATHS /
# OPENCV_LINK_PATHS / OPENCV_LINK_LIBS set, which bypasses the pkg-config probe.
CARGO_EXCLUDE := --exclude mv-video
# full_benchmark prompts on stdin when STEPS is empty; winpty gives it a real
# console. The shared makefile drops the wrapper when STEPS is set.
BENCH_WRAPPER := winpty

MAX_THREADS ?= $(shell nproc)

PLATFORM_GUARD := check-shell

check-shell:
	@if [ "$$MSYSTEM" != "MINGW64" ]; then \
		echo "[ERROR] Run this from the MSYS2 MINGW64 shell."; \
		echo "        Open 'MSYS2 MinGW x64' from the Start Menu."; \
		exit 1; \
	fi
	@echo "[OK]    Running in MINGW64."

# -----------------------------------------------------------------------------
# FFmpeg component sets — identical to mk/linux.mk (GCC accepts the mov muxer;
# only MSVC chokes on it). See mk/linux.mk for the per-flag rationale.
# -----------------------------------------------------------------------------
SLIM_FFMPEG := --disable-everything \
	--enable-decoder=h264,hevc,mpeg4 \
	--enable-parser=h264,hevc,mpeg4video \
	--enable-demuxer=mov,avi,h264,hevc,mpegts,rtsp,sdp \
	--enable-muxer=mov \
	--enable-protocol=file,rtp,tcp,udp \
	--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,extract_extradata

SLIMMEST_FFMPEG := \
	--disable-avdevice --disable-avfilter --disable-swscale --disable-swresample \
	--disable-network --disable-autodetect --disable-iconv \
	--disable-iamf --disable-faan \
	--disable-everything \
	--enable-decoder=h264,hevc,mpeg4 \
	--enable-parser=h264,hevc,mpeg4video \
	--enable-demuxer=mov,avi,h264,hevc \
	--enable-protocol=file \
	--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,extract_extradata

# --target-os/--arch pin the MinGW64 cross-shape. Kept in step with the
# ffmpeg_installer submodule's own makefile.windows (a different file from the
# repo-root ones this replaced -- that one still exists and is unaffected).
FF_CONFIGURE_FLAGS = --enable-shared --disable-static --enable-swresample \
	--target-os=mingw32 --arch=x86_64 \
	--enable-debug --disable-stripping --disable-doc \
	$(SLIM_FFMPEG) --pkg-config-flags="--static"

SLIM_CONFIGURE = ./configure --prefix='$(SLIM_PREFIX)' --enable-shared --disable-static \
		--target-os=mingw32 --arch=x86_64 \
		--disable-programs --disable-doc --disable-debug \
		$(SLIMMEST_FFMPEG) --pkg-config-flags="--static"

# -----------------------------------------------------------------------------
# Cargo invocation
# -----------------------------------------------------------------------------
# $(1) = FFmpeg prefix, $(2) = CARGO_TARGET_DIR, $(3) = extra cargo flags.
# No RUSTFLAGS rpath — runtime DLL resolution is by PATH instead. The FFmpeg
# bin dir goes on PATH at build time so the linker finds the import libraries.
define build_extractors
	PATH='$(1)/bin':$$PATH \
	PKG_CONFIG_PATH='$(1)/lib/pkgconfig' \
	CARGO_TARGET_DIR='$(2)' \
	cargo build --release -p mv-extract $(3)
endef

# $(1) = cargo subcommand + args. The exported /mingw64 PKG_CONFIG_PATH above
# already covers the workspace build, so no per-invocation env is needed.
define cargo_sys_env
	cargo $(1)
endef

# $(1) = FFmpeg prefix, $(2) = destination directory. Puts the matching DLLs
# next to the executables so they run in place.
define copy_runtime_libs
	@cp -u '$(1)/bin/'*.dll '$(2)/' 2>/dev/null || true
endef

# -----------------------------------------------------------------------------
# PGO (GCC -fprofile-generate / -fprofile-use) — same mechanism as Linux
# -----------------------------------------------------------------------------
PGO_PROFILE_GLOB := *.gcda

define pgo_configure_cust_gen
	./configure --prefix='$(CUSTOM_PREFIX)' $(FF_CONFIGURE_FLAGS) \
		--extra-cflags="-fprofile-generate='$(1)' -fprofile-update=atomic" \
		--extra-ldflags="-fprofile-generate='$(1)'"
endef

define pgo_configure_cust_use
	./configure --prefix='$(CUSTOM_PREFIX)' $(FF_CONFIGURE_FLAGS) \
		--extra-cflags="-fprofile-use='$(1)' -fprofile-correction \
			-Wno-missing-profile -Wno-coverage-mismatch"
endef

define pgo_configure_slim_gen
	$(SLIM_CONFIGURE) \
		--extra-cflags="-fprofile-generate='$(1)' -fprofile-update=atomic" \
		--extra-ldflags="-fprofile-generate='$(1)'"
endef

define pgo_configure_slim_use
	$(SLIM_CONFIGURE) \
		--extra-cflags="-fprofile-use='$(1)' -fprofile-correction \
			-Wno-missing-profile -Wno-coverage-mismatch"
endef

# -----------------------------------------------------------------------------
# FFmpeg CLI (decode_ffmpeg) — DLLs resolved via PATH
# -----------------------------------------------------------------------------
define ffmpeg_cli
	PATH='$(CUSTOM_PREFIX)/bin':$$PATH '$(CUSTOM_PREFIX)/bin/ffmpeg$(EXE_EXT)' $(1)
endef

# -----------------------------------------------------------------------------
# Dependency installation
# -----------------------------------------------------------------------------
# pacman packages mirror the Linux apt list as closely as MSYS2 allows.
# Skipped on Windows: vtune (Linux profiler), linux-tools (perf).
# wkhtmltopdf: install the native Windows binary from wkhtmltopdf.org, then
# add C:\Program Files\wkhtmltopdf\bin to PATH (needed by imgkit/plots.py).
platform_install: check-shell
	@echo "[INFO]  Installing MSYS2/MinGW64 build dependencies..."
	pacman -S --needed --noconfirm \
		mingw-w64-x86_64-toolchain \
		mingw-w64-x86_64-rust \
		mingw-w64-x86_64-nasm \
		mingw-w64-x86_64-pkg-config \
		mingw-w64-x86_64-clang \
		mingw-w64-x86_64-python \
		mingw-w64-x86_64-python-pip \
		mingw-w64-x86_64-python-matplotlib \
		mingw-w64-x86_64-python-pandas \
		mingw-w64-x86_64-python-seaborn \
		mingw-w64-x86_64-python-lxml \
		mingw-w64-x86_64-opencv \
		make git patch diffutils \
		winpty
	@mkdir -p '$(VENV_FOLDER)'
# CI (GitHub Actions sets CI=true) skips the report-only Python venv/pip layer;
# the pacman packages above are enough to build + benchmark.
ifndef CI
	python -m venv --upgrade-deps --system-site-packages '$(VENV_WIN)'
	grep -vE '^\s*(matplotlib|pandas|seaborn)' requirements.txt > /tmp/pip-reqs.txt && \
	'$(PYTHON)' -m pip install -r /tmp/pip-reqs.txt
endif

PLATFORM_PHONY := check-shell
