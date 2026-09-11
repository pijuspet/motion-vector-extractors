# =============================================================================
# Platform: Linux (GCC / glibc)
# =============================================================================
# Included by the top-level makefile when PLATFORM=linux (the default off a
# Linux `uname -s`). Defines the platform contract documented at the top of
# that file; everything else is shared.
# =============================================================================

EXE_EXT :=
REL     := release

# Binaries resolve their FFmpeg via rpath, so all three prefixes can deploy
# into one directory — no per-prefix DLL segregation is needed the way it is
# on Windows. Mirrors MethodInfo::exe_path()'s #[cfg(not(windows))] branch in
# crates/mv-bench/benchmark_extractors.rs, which builds a flat path.
EXECUTABLES_DIR_SYS  := $(EXECUTABLES_DIR)
EXECUTABLES_DIR_CUST := $(EXECUTABLES_DIR)

CARGO_TARGET_BASE := $(CURRENT_DIR)/target
VENV_FOLDER       ?= $(PARENT_DIR)/venv-motion-vectors

# No cross-target, no crate exclusions (OpenCV is available via apt, so
# mv-video builds), no interactive-console shim.
CARGO_TARGET_FLAG :=
CARGO_EXCLUDE     :=
BENCH_WRAPPER     :=

MAX_THREADS ?= 128

# Guard target run before the toolchain-sensitive targets. Nothing to check on
# Linux — the shared makefile tolerates this being empty.
PLATFORM_GUARD :=

# -----------------------------------------------------------------------------
# FFmpeg component sets
# -----------------------------------------------------------------------------
# Slimmed component set — only what the extractors actually use. Validated to
# produce byte-identical MV output to a full build across h264/hevc/mpeg4.
#   - mpeg4 decoder is REQUIRED: the "h264_avi" inputs are really MPEG-4 Part 2,
#     and h264's MV export is compile-gated behind CONFIG_MPEGVIDEODEC, which an
#     mpegvideo decoder (mpeg4) turns on — without it h264 exports zero MVs.
#   - rtsp/sdp are demuxers (RTSP rides on the rtp/tcp/udp protocols).
#   - mov muxer is kept for the optional `decode_ffmpeg` remux helper. (mk/msvc.mk
#     drops it — the mov muxer pulls in FFmpeg's cbs layer with no cbs types, and
#     that empty table fails to compile under MSVC. GCC/MinGW accept it fine.)
SLIM_FFMPEG := --disable-everything \
	--enable-decoder=h264,hevc,mpeg4 \
	--enable-parser=h264,hevc,mpeg4video \
	--enable-demuxer=mov,avi,h264,hevc,mpegts,rtsp,sdp \
	--enable-muxer=mov \
	--enable-protocol=file,rtp,tcp,udp \
	--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,extract_extradata

# Configure flags shared by the regular + custom trees (the shared makefile
# supplies --prefix). Kept as one variable so the PGO targets can reuse it
# verbatim and append only their -fprofile-* flags.
FF_CONFIGURE_FLAGS = --enable-shared --disable-static --enable-swresample \
	--enable-debug --disable-stripping --disable-doc \
	$(SLIM_FFMPEG) --pkg-config-flags="--static"

# -----------------------------------------------------------------------------
# Cargo invocation
# -----------------------------------------------------------------------------
# $(1) = FFmpeg prefix, $(2) = CARGO_TARGET_DIR, $(3) = extra cargo flags.
# Builds every extractor in mv-extract linked against the given FFmpeg prefix.
# $(3) enables the `custom_ffmpeg` Cargo feature when linking against the
# patched FFmpeg — that feature gates AVMotionVectorCompact /
# AV_FRAME_DATA_MOTION_VECTORS_COMPACT, which only exist in the custom build.
define build_extractors
	PKG_CONFIG_PATH=$(1)/lib/pkgconfig \
	RUSTFLAGS="-C link-arg=-Wl,-rpath,$(1)/lib -C link-arg=-Wl,--disable-new-dtags" \
	CARGO_TARGET_DIR=$(2) \
	cargo build --release -p mv-extract $(3)
endef

# $(1) = cargo subcommand + args. Points pkg-config at the built regular FFmpeg
# prefix (and bakes its rpath) so the workspace's ffmpeg-sys-next links against
# FFmpeg 8.0 rather than whatever incomplete/older FFmpeg is on the system path.
define cargo_sys_env
	PKG_CONFIG_PATH=$(REGULAR_PREFIX)/lib/pkgconfig \
	RUSTFLAGS="-C link-arg=-Wl,-rpath,$(REGULAR_PREFIX)/lib -C link-arg=-Wl,--disable-new-dtags" \
	cargo $(1)
endef

# No-op: rpath means nothing has to be copied next to the binaries.
# $(1) = FFmpeg prefix, $(2) = destination directory.
define copy_runtime_libs
endef
# -----------------------------------------------------------------------------
# extractor8 (edge264)
# -----------------------------------------------------------------------------
# edge264 (https://github.com/tvlabs/edge264) is a third-party from-scratch
# H.264 decoder, forked here (see EDGE264_PATCH in the top-level makefile) to add
# motion-vector-only decoding, a smaller macroblock layout, 4:2:2 residual
# parsing and a picture-completion fix. The extractor itself now lives in the
# submodule as edge264/extractor.c and is built by edge264's own Makefile
# ("make extractor"); we supply the FFmpeg prefix it demuxes with, an absolute
# rpath to the submodule (edge264 bakes $$ORIGIN, which stops resolving once the
# binary is copied out of its build directory), and deploy the result as
# executables/extractor8. The rm forces a relink: a plain
# "make -C edge264 extractor" produces an $$ORIGIN-only binary, and make would
# otherwise consider it up to date and let us deploy one that cannot start.
EDGE264_DIR := $(CURRENT_DIR)/edge264

# edge264 derives -march=native -O3 itself; -g mirrors the FFmpeg trees'
# --enable-debug --disable-stripping so perf/VTune can see inside it.
EDGE264_EXTRA_CFLAGS := -g

# VARIANTS= drops edge264's `logs` object - a second copy of the header parser
# compiled with -DLOGS, for a log callback the extractor never installs.
# BUILDTEST=no skips edge264_test, which dlopen()s SDL2 to display frames.
# $(1) = extra CFLAGS, $(2) = extra LDFLAGS.
define edge264_build
	$(MAKE) -C '$(EDGE264_DIR)' VARIANTS= BUILDTEST=no \
		CFLAGS='$(EDGE264_EXTRA_CFLAGS) $(1)' LDFLAGS='$(2)'
endef

# Builds the library + extractor and deploys it. Skips with a hint rather than
# failing when the submodule has not been checked out, so `make build` still
# works on a clone that only wants the FFmpeg-based methods.
define build_edge264
	@if [ -f '$(EDGE264_DIR)/Makefile' ]; then \
		$(MAKE) --no-print-directory -C '$(EDGE264_DIR)' VARIANTS= BUILDTEST=no \
			CFLAGS='$(EDGE264_EXTRA_CFLAGS)' && \
		rm -f '$(EDGE264_DIR)/extractor$(EXE_EXT)' && \
		$(MAKE) --no-print-directory -C '$(EDGE264_DIR)' extractor$(EXE_EXT) \
			VARIANTS= BUILDTEST=no CFLAGS='$(EDGE264_EXTRA_CFLAGS)' \
			FFMPEG_PREFIX='$(REGULAR_PREFIX)' \
			LDFLAGS='-Wl,-rpath,$(EDGE264_DIR) -Wl,--disable-new-dtags' && \
		cp '$(EDGE264_DIR)/extractor$(EXE_EXT)' \
			'$(EXECUTABLES_DIR_SYS)/extractor8$(EXE_EXT)'; \
	else \
		echo "[SKIP]  edge264 submodule not checked out - method 8 not built"; \
		echo "        run: git submodule update --init edge264"; \
	fi
endef

# -----------------------------------------------------------------------------
# PGO (GCC -fprofile-generate / -fprofile-use)
# -----------------------------------------------------------------------------
# The profile is kept OUT of the source tree: FFmpeg's CLEANSUFFIXES
# (ffbuild/common.mak) lists *.gcda, so an in-tree profile would be deleted by
# the very `make clean` these phases need — silently, because
# -Wno-missing-profile hides the resulting "no profile data" warnings.
#
# -fprofile-correction tolerates residual counter skew from the threaded
# training runs; the two -Wno- flags keep the log readable, since most objects
# (demuxers, option tables) are never touched by a decode-only training run and
# would otherwise warn on every file.
PGO_PROFILE_GLOB := *.gcda

# $(1) = profile directory.
define pgo_configure_cust_gen
	./configure --prefix=$(CUSTOM_PREFIX) $(FF_CONFIGURE_FLAGS) \
		--extra-cflags="-fprofile-generate=$(1) -fprofile-update=atomic" \
		--extra-ldflags="-fprofile-generate=$(1)"
endef

define pgo_configure_cust_use
	./configure --prefix=$(CUSTOM_PREFIX) $(FF_CONFIGURE_FLAGS) \
		--extra-cflags="-fprofile-use=$(1) -fprofile-correction \
			-Wno-missing-profile -Wno-coverage-mismatch"
endef

# edge264 uses the same GCC mechanism, driven through its Makefile's CFLAGS /
# LDFLAGS rather than a configure script. $(1) = profile directory.
define pgo_edge264_gen
	$(call edge264_build,-fprofile-generate=$(1) -fprofile-update=atomic,-fprofile-generate=$(1))
endef

define pgo_edge264_use
	$(call edge264_build,-fprofile-use=$(1) -fprofile-correction -Wno-missing-profile -Wno-coverage-mismatch,)
endef



# -----------------------------------------------------------------------------
# FFmpeg CLI (decode_ffmpeg) — resolved via LD_LIBRARY_PATH
# -----------------------------------------------------------------------------
define ffmpeg_cli
	LD_LIBRARY_PATH=$(CUSTOM_PREFIX)/lib:$$LD_LIBRARY_PATH $(CUSTOM_PREFIX)/bin/ffmpeg $(1)
endef

# -----------------------------------------------------------------------------
# Dependency installation
# -----------------------------------------------------------------------------
# CI (GitHub Actions sets CI=true) skips profiler/report-only tooling so a build
# can be verified without VTune/perf. SUDO is empty for local root runs and set
# to `sudo` by CI.
SUDO ?=

# Packages required to build + benchmark. Rust is bootstrapped separately via
# rustup (see the recipe) — the apt `cargo`/`rustup` packages conflict with
# each other on recent Ubuntu, so they're intentionally not listed here.
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

platform_install: install_vtune
	command -v cargo >/dev/null 2>&1 || curl https://sh.rustup.rs -sSf | sh -s -- -y
	$(SUDO) apt install -y $(APT_CORE)
ifndef CI
	$(SUDO) apt install -y $(APT_EXTRA)
	mkdir -p $(VENV_FOLDER)
	python3 -m venv $(VENV_FOLDER)
	. $(VENV_FOLDER)/bin/activate && pip install -r requirements.txt
endif

PLATFORM_PHONY := install_vtune
