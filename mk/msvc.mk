# =============================================================================
# Platform: Windows / native MSVC   ** BEST-EFFORT / UNTESTED **
# =============================================================================
# Included by the top-level makefile when PLATFORM=msvc. This is never
# auto-detected: MSVC and MinGW both run under MSYS2 and `uname -s` cannot tell
# which you want, so ask for it explicitly:
#
#     make PLATFORM=msvc build
#
# Targets the native MSVC toolchain instead of MinGW64:
#   - FFmpeg is built with `--toolchain=msvc` (cl.exe / link.exe), producing
#     MSVC import libraries (avcodec.lib, ...) + DLLs.
#   - Rust builds against the `x86_64-pc-windows-msvc` target (MSVC ABI).
#   - bindgen parses headers with an MSVC-targeted libclang.
#
# IMPORTANT — this path was written without a Windows+MSVC machine to test on,
# so expect to iterate. The spots most likely to need adjustment are flagged
# with `# VERIFY:` comments (import-lib location, LIB path, libclang path).
#
# Performance note: MSVC does NOT fix the main Windows slowness. FFmpeg's
# ./configure is a POSIX shell script that still runs under MSYS2/bash and
# spawns thousands of probe processes; that cost (plus Defender) dominates.
# The real wins are the parallel `-j` and slimmed `configure` below, and adding
# Windows Defender exclusions for this tree + the toolchain dirs.
#
# HOW TO RUN
#   1. Install: Visual Studio Build Tools (C++ workload -> cl.exe, link.exe),
#      NASM, LLVM-for-Windows (libclang), rustup with the
#      `stable-x86_64-pc-windows-msvc` toolchain, and an MSYS2 install for
#      bash/make/git/pkgconf.  (See `make PLATFORM=msvc help`.)
#   2. Open "x64 Native Tools Command Prompt for VS", then launch the MSYS2
#      bash from it so `cl`/`link`/`nasm` are inherited on PATH, e.g.:
#          C:\msys64\usr\bin\bash.exe -l
#   3. cd to the repo and run the targets below.
# =============================================================================

EXE_EXT := .exe

# Rust MSVC target triple and its release output subpath.
RUST_TARGET := x86_64-pc-windows-msvc
REL         := $(RUST_TARGET)/release

# Same DLL-segregation rationale as mk/mingw.mk.
EXECUTABLES_DIR_SYS  := $(EXECUTABLES_DIR)/sys
EXECUTABLES_DIR_CUST := $(EXECUTABLES_DIR)/cust

# Cargo target dir must be on a space-free path.
CARGO_TARGET_BASE := $(HOME)/cargo-target/motion-vector-extractors
VENV_FOLDER       ?= $(CURRENT_DIR)/venv-motion-vectors
VENV_WIN          := $(shell cygpath -m '$(VENV_FOLDER)' 2>/dev/null)

export CARGO_TARGET_DIR := $(CARGO_TARGET_BASE)/tools

# MSYS2 ships /usr/bin/link.exe (GNU coreutils `link`), and a login shell puts
# /usr/bin ahead of the inherited VS PATH — so a bare `link.exe`, like the one
# rustc invokes for the MSVC target, resolves to coreutils and dies with
# "extra operand ...rcgu.o". cl.exe exists only in the MSVC toolchain and lives
# in the same dir as the real link.exe, so derive that dir from cl and push it
# to the front of PATH for every recipe (FFmpeg + cargo alike).
MSVC_BIN := $(shell dirname "$$(command -v cl 2>/dev/null)" 2>/dev/null)
ifneq ($(MSVC_BIN),)
export PATH := $(MSVC_BIN):$(PATH)
endif

# Use the MinGW pkgconf (a native-Windows build), NOT MSYS2's cygwin
# /usr/bin/pkgconf. For an MSVC target the cygwin one is doubly wrong: it splits
# PKG_CONFIG_PATH on ':' (so a "D:/..." drive-letter colon shreds the path) AND
# it emits POSIX -I/-L paths (/d/a/...) that native cl.exe/link.exe cannot read
# (-> "Cannot open include file: 'libavutil/avutil.h'"). The MinGW pkgconf splits
# on ';' and emits Windows paths the MSVC toolchain consumes — the same pkgconf
# the MinGW path already relies on. This needs the mingw-w64-x86_64-pkgconf
# package; override PKG_CONFIG if it lives elsewhere.
PKG_CONFIG ?= /mingw64/bin/pkg-config
export PKG_CONFIG

# pkg-config wants POSIX colon-separated paths.
export PKG_CONFIG_PATH := /mingw64/lib/pkgconfig

# bindgen (via ffmpeg-sys-next) must parse the MSVC system headers with the
# MSVC target triple, otherwise the predefined macros are wrong.
export BINDGEN_EXTRA_CLANG_ARGS := --target=x86_64-pc-windows-msvc
# VERIFY: point this at an MSVC-built libclang (LLVM for Windows). Override on
# the command line if LLVM is installed elsewhere:
#     make PLATFORM=msvc build_tools LIBCLANG_PATH='/c/Program Files/LLVM/bin'
LIBCLANG_PATH ?= /c/Program Files/LLVM/bin
export LIBCLANG_PATH

CARGO_TARGET_FLAG := --target $(RUST_TARGET)
# mv-video is EXCLUDED (NOT mv-bench — mv-bench depends only on mv-types).
# mv-video is the sole crate pulling in the `opencv` crate, whose build script
# probes for a *system* OpenCV install (pkg-config opencv4 / vcpkg / cmake
# OpenCVConfig). None of the MSVC prereqs install OpenCV, so a bare
# `--workspace` fails in opencv's build script:
#     Failed to find installed OpenCV package using probes:
#     environment, pkg_config, vcpkg_cmake, vcpkg, cmake
# A vcpkg build of it would add ~30-60 min to CI and it is not part of the
# extractor deliverable, so exclude just that crate; everything else
# (mv-types, mv-extract, mv-bench, mv-publish) still builds. To include it,
# install OpenCV (e.g. `vcpkg install opencv4:x64-windows`) and expose it to the
# opencv crate — put opencv4.pc on PKG_CONFIG_PATH or set OPENCV_INCLUDE_PATHS /
# OPENCV_LINK_PATHS / OPENCV_LINK_LIBS (or OpenCV_DIR for the cmake probe).
CARGO_EXCLUDE := --exclude mv-video
BENCH_WRAPPER :=

MAX_THREADS ?= $(shell nproc)

PLATFORM_GUARD := check-env

check-env:
	@command -v cl   >/dev/null 2>&1 || { echo "[ERROR] cl.exe not on PATH. Launch MSYS2 bash from the 'x64 Native Tools Command Prompt for VS'."; exit 1; }
	@[ -n "$(MSVC_BIN)" ] && [ -x "$(MSVC_BIN)/link.exe" ] || { echo "[ERROR] MSVC link.exe not found next to cl.exe (got MSVC_BIN='$(MSVC_BIN)'). NB: 'command -v link' finds MSYS2 coreutils 'link', not the linker."; exit 1; }
	@command -v nasm >/dev/null 2>&1 || { echo "[ERROR] nasm not on PATH (needed for FFmpeg asm)."; exit 1; }
	@[ -x "$(PKG_CONFIG)" ] || command -v "$(PKG_CONFIG)" >/dev/null 2>&1 || { echo "[ERROR] MinGW pkg-config not found at '$(PKG_CONFIG)'. Install it: pacman -S mingw-w64-x86_64-pkgconf (the MSYS cygwin pkgconf emits POSIX paths cl/link can't read)."; exit 1; }
	@command -v cargo >/dev/null 2>&1 || { echo "[ERROR] cargo not found (install rustup + the $(RUST_TARGET) toolchain)."; exit 1; }
	@rustup target list --installed 2>/dev/null | grep -qx '$(RUST_TARGET)' || echo "[WARN]  rustup target '$(RUST_TARGET)' not installed: rustup target add $(RUST_TARGET)"
	@echo "[OK]    MSVC toolchain visible (cl, link, nasm, cargo)."

# -----------------------------------------------------------------------------
# FFmpeg component sets
# -----------------------------------------------------------------------------
# Same reasoning as mk/linux.mk, with one MSVC-only subtraction:
#   - No mov muxer (unlike linux/mingw, which keep it): the mov muxer drags in
#     FFmpeg's coded-bitstream (cbs) layer with zero cbs types enabled, and MSVC
#     rejects the resulting empty table (error C7757). The extractors only
#     demux/decode, so this only disables the optional `decode_ffmpeg` remux
#     helper; re-add `--enable-muxer=mov --enable-cbs_av1` if you need it.
SLIM_FFMPEG := --disable-everything \
	--enable-decoder=h264,hevc,mpeg4 \
	--enable-parser=h264,hevc,mpeg4video \
	--enable-demuxer=mov,avi,h264,hevc,mpegts,rtsp,sdp \
	--enable-protocol=file,rtp,tcp,udp \
	--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,extract_extradata

# `cl`/`link`/`nasm` must be on PATH (launch MSYS2 bash from the VS x64 Native
# Tools prompt). Produces DLLs + MSVC import libs. Flags mirror a known-good
# MSVC build:
#   --extra-cflags=-MD                   : dynamic CRT, matching Rust's MSVC
#                                          default (a /MT vs /MD mismatch breaks
#                                          the link).
#   --extra-ldflags=/NODEFAULTLIB:libcmt : keep the static CRT out of the link.
#   --disable-programs                   : no ffmpeg/ffprobe/ffplay CLIs
#                                          (this also disables decode_ffmpeg).
#   --disable-{bzlib,libopenjpeg,iconv,zlib} : external libs awkward on MSVC.
#   --enable-asm                         : keep nasm/external asm (NOT GCC
#                                          inline asm, which MSVC lacks —
#                                          HAVE_INLINE_ASM stays 0).
FF_CONFIGURE_FLAGS = --toolchain=msvc --arch=amd64 \
	--enable-asm --enable-shared --disable-static --enable-swresample \
	--enable-debug --disable-stripping --disable-doc --disable-programs \
	--disable-bzlib --disable-libopenjpeg --disable-iconv --disable-zlib \
	--extra-cflags="-MD" --extra-ldflags="/NODEFAULTLIB:libcmt" \
	--pkg-config-flags="--static" $(SLIM_FFMPEG)

# -----------------------------------------------------------------------------
# Cargo invocation
# -----------------------------------------------------------------------------
# $(1) = FFmpeg prefix, $(2) = CARGO_TARGET_DIR, $(3) = extra cargo flags.
# DLLs live in <prefix>/bin (PATH-resolved at runtime, same as MinGW). The MSVC
# linker finds import libraries via the LIB env var.
# VERIFY: FFmpeg+MSVC may install the import .libs in <prefix>/bin rather than
# <prefix>/lib — both are added to LIB so either layout links.
#
# PKG_CONFIG_PATH stays a plain Windows path ($(CURDIR) form, e.g. D:/a/...):
# the MinGW pkgconf selected above splits on ';' and reads drive-letter paths
# directly, then emits Windows -I/-L flags the MSVC toolchain understands.
define build_extractors
	PATH='$(1)/bin':$$PATH \
	PKG_CONFIG_PATH='$(1)/lib/pkgconfig' \
	LIB="$$(cygpath -w '$(1)/lib');$$(cygpath -w '$(1)/bin');$$LIB" \
	CARGO_TARGET_DIR='$(2)' \
	cargo build --release $(CARGO_TARGET_FLAG) -p mv-extract $(3)
endef

# $(1) = cargo subcommand + args. The workspace pulls in ffmpeg-sys-next (via
# mv-extract), so it needs an FFmpeg prefix on PKG_CONFIG_PATH / LIB just like
# build_extractors — the global /mingw64 pkgconfig has no FFmpeg. Point it at
# the regular (sys) build.
define cargo_sys_env
	PKG_CONFIG_PATH='$(REGULAR_PREFIX)/lib/pkgconfig' \
	PATH='$(REGULAR_PREFIX)/bin':$$PATH \
	LIB="$$(cygpath -w '$(REGULAR_PREFIX)/lib');$$(cygpath -w '$(REGULAR_PREFIX)/bin');$$LIB" \
	cargo $(1)
endef

# $(1) = FFmpeg prefix, $(2) = destination directory.
define copy_runtime_libs
	@cp -u '$(1)/bin/'*.dll '$(2)/' 2>/dev/null || true
endef

# -----------------------------------------------------------------------------
# PGO (MSVC /GENPROFILE -> train -> /USEPROFILE)   ** UNTESTED **
# -----------------------------------------------------------------------------
# MSVC PGO is a different mechanism from GCC's, so this is NOT a port of the
# -fprofile-generate/-fprofile-use recipe in mk/linux.mk and mk/mingw.mk:
#
#   instrument : cl /GL           + link /GENPROFILE:PGD=<pgd>
#   train      : run the .exe     -> writes <pgd-dir>/*.pgc next to the .pgd
#   optimize   : cl /GL           + link /USEPROFILE:PGD=<pgd>
#
# /GENPROFILE + /USEPROFILE supersede the older /LTCG:PGINSTRUMENT +
# pgomgr /merge + /LTCG:PGOPTIMIZE dance; /USEPROFILE merges the .pgc files
# itself, so no pgomgr step is needed.
#
# Differences from the GCC targets worth knowing:
#   - `make clean` is still required between phases (same reason: configure
#     regenerates config.mak but leaves the objects), but the profile lives in
#     its own directory outside the tree anyway, so clean cannot eat it.
#   - The training run needs the INSTRUMENTED DLLs on PATH — hence the copy into
#     the executables dir before training, same as the GCC targets.
#   - /GL is whole-program optimization; it makes the compile phase noticeably
#     slower. On Windows the configure probes usually still dominate.
#
# VERIFY: the exact .pgd path handling. link.exe wants a Windows path, so it is
# passed through cygpath -w. If link reports it cannot create/find the .pgd,
# check that PGO_*_DIR exists and is writable from the native (non-MSYS) side.
PGO_PROFILE_GLOB := *.pgc

define pgo_configure_cust_gen
	./configure --prefix='$(CUSTOM_PREFIX)' --toolchain=msvc --arch=amd64 \
		--enable-asm --enable-shared --disable-static --enable-swresample \
		--enable-debug --disable-stripping --disable-doc --disable-programs \
		--disable-bzlib --disable-libopenjpeg --disable-iconv --disable-zlib \
		--extra-cflags="-MD -GL" \
		--extra-ldflags="/NODEFAULTLIB:libcmt /GENPROFILE:PGD=$$(cygpath -w '$(1)')\\mv.pgd" \
		--pkg-config-flags="--static" $(SLIM_FFMPEG)
endef

define pgo_configure_cust_use
	./configure --prefix='$(CUSTOM_PREFIX)' --toolchain=msvc --arch=amd64 \
		--enable-asm --enable-shared --disable-static --enable-swresample \
		--enable-debug --disable-stripping --disable-doc --disable-programs \
		--disable-bzlib --disable-libopenjpeg --disable-iconv --disable-zlib \
		--extra-cflags="-MD -GL" \
		--extra-ldflags="/NODEFAULTLIB:libcmt /USEPROFILE:PGD=$$(cygpath -w '$(1)')\\mv.pgd" \
		--pkg-config-flags="--static" $(SLIM_FFMPEG)
endef



# -----------------------------------------------------------------------------
# FFmpeg CLI (decode_ffmpeg) — DLLs resolved via PATH.
# NB: FF_CONFIGURE_FLAGS passes --disable-programs, so ffmpeg.exe is not built
# by default on this platform; decode_ffmpeg only works against a tree
# configured without that flag.
# -----------------------------------------------------------------------------
define ffmpeg_cli
	PATH='$(CUSTOM_PREFIX)/bin':$$PATH '$(CUSTOM_PREFIX)/bin/ffmpeg$(EXE_EXT)' $(1)
endef

# -----------------------------------------------------------------------------
# Dependency installation
# -----------------------------------------------------------------------------
# MSVC deps can't be installed via pacman; they come from external installers.
# This only prints guidance (winget commands are behind GUI/admin prompts).
# It does not auto-install.
platform_install:
	@echo "Install the MSVC build prerequisites (admin shell):"
	@echo "  winget install --id Microsoft.VisualStudio.2022.BuildTools  # add 'Desktop development with C++'"
	@echo "  winget install --id LLVM.LLVM            # provides libclang for bindgen"
	@echo "  winget install --id NASM.NASM            # FFmpeg assembler"
	@echo "  winget install --id Rustlang.Rustup      # then: rustup default stable-$(RUST_TARGET)"
	@echo "  (MSYS2 from msys2.org for bash/make/git; 'pacman -S mingw-w64-x86_64-pkgconf'"
	@echo "   for a native-Windows pkg-config — the MSYS cygwin pkgconf emits POSIX paths cl/link reject)"
	@echo ""
	@echo "  Optional (only for mv-video, which pulls the opencv crate):"
	@echo "  vcpkg install opencv4:x64-windows        # then export OpenCV_DIR / OPENCV_* so the opencv crate's probe succeeds"
	@echo ""
	@echo "Then add Windows Defender exclusions (big build speedup, elevated PowerShell):"
	@echo "  Add-MpPreference -ExclusionPath '$(CURRENT_DIR)'"
	@echo "  Add-MpPreference -ExclusionProcess 'cl.exe','link.exe','nasm.exe','cargo.exe','rustc.exe'"

PLATFORM_PHONY := check-env
