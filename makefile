# =============================================================================
# CONFIGURATION & GLOBAL VARIABLES
# =============================================================================

STREAMS = 15
NRUNS = 1

# VIDEO_NAME ?= bigbunny_walking.mp4
VIDEO_NAME ?= bigbunny.mp4
# VIDEO_NAME ?= stickman.mp4
# VIDEO_NAME ?= dashcam.mp4

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

EXTRACTOR_DIR = extractors
BENCHMARKING_DIR = benchmarking
EXECUTABLES_DIR = executables
WRITER_SRC = $(EXTRACTOR_DIR)/writer.cpp -Iextractors

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
get_cflags = $(shell $(call pkg_cmd,$(1)) --cflags $(FF_PKGS))
get_libs   = $(shell $(call pkg_cmd,$(1)) --libs $(FF_PKGS))
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

# FFmpeg build macro
FFMPEG_BUILD = \
	cd $1/FFmpeg && \
	chmod +x ./configure ./ffbuild/*.sh && \
	./configure --prefix=$(abspath $1) --enable-shared --enable-swresample --enable-debug --disable-stripping --pkg-config-flags="--static" && \
	make && make install

# =============================================================================
# INSTALLATION & DEPENDENCIES
# =============================================================================

install_vtune:
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

install: install_vtune
	apt install -y build-essential gcc g++ make pkg-config nasm xdg-utils libnss3 libnotify4 wkhtmltopdf
	cp -n .env_template .env
	mkdir -p $(VENV_FOLDER)
	mkdir -p $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)
	mkdir -p $(BENCHMARKING_DIR)/$(EXECUTABLES_DIR)
	python3 -m venv $(VENV_FOLDER)
	. $(VENV_FOLDER)/bin/activate && pip install -r requirements.txt

setup_ffmpeg:
	$(call FFMPEG_BUILD,$(CUSTOM_PREFIX))
	$(call FFMPEG_BUILD,$(REGULAR_PREFIX))

# =============================================================================
# BUILD TARGETS
# =============================================================================

build:
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor0 $(EXTRACTOR_DIR)/extractor0.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor1 $(EXTRACTOR_DIR)/extractor1.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor2 $(EXTRACTOR_DIR)/extractor2.cpp  $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor3 $(EXTRACTOR_DIR)/extractor3.cpp $(WRITER_SRC) $(CUST_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor4 $(EXTRACTOR_DIR)/extractor1.cpp $(WRITER_SRC) $(CUST_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor5 $(EXTRACTOR_DIR)/extractor5.cpp $(WRITER_SRC) $(CUST_FF)

build_sys:
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor0 $(EXTRACTOR_DIR)/extractor0.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor1 $(EXTRACTOR_DIR)/extractor1.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor2 $(EXTRACTOR_DIR)/extractor2.cpp  $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor3 $(EXTRACTOR_DIR)/extractor3.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor4 $(EXTRACTOR_DIR)/extractor1.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor5 $(EXTRACTOR_DIR)/extractor5.cpp $(WRITER_SRC) $(SYS_FF)

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
				$(PYTHON) -m benchmarking.full_benchmark $$filepath $(STREAMS) $$vtype cust $(NRUNS) 0; \
			else \
				$(PYTHON) -m benchmarking.full_benchmark $$filepath $(STREAMS) $$vtype $(TYPE) $(NRUNS) 0; \
			fi; \
		else \
			echo "SKIP: $$filepath not found"; \
		fi; \
	done

benchmark:
	$(PYTHON) -m benchmarking.full_benchmark $(VIDEO_FILE) $(STREAMS) $(VIDEO_TYPE) cust $(NRUNS)

# =============================================================================
# DEVELOPMENT & TESTING TOOLS
# =============================================================================

# note, the new h265 motion vector extraction function is not visible in this diff file
ffmpeg_diff:
	diff -u -I '/tmp/ffconf\.' \
		-x 'config.h' \
		-x 'ffbuild' \
		-x '*.pc' \
		-x 'ffversion.h' \
		-r  $(REGULAR_PREFIX)/FFmpeg/ $(CUSTOM_PREFIX)/FFmpeg/ \
		| sed '/Binary\ files\ /d' \
		| sed 's|$(REGULAR_PREFIX)/FFmpeg/|a/|' \
		| sed 's|$(CUSTOM_PREFIX)/FFmpeg/|b/|' \
		> ffmpeg/ffmpeg_version.diff

test_ffmpeg:
	$(call FFMPEG_BUILD,$(CUSTOM_PREFIX))
	$(PYTHON) -m benchmarking.full_benchmark $(VIDEO_FILE) $(STREAMS) $(VIDEO_TYPE) cust $(NRUNS) 1 2 5
#   chromium --no-sandbox $(shell ls -d $(CURRENT_DIR)/results/$(VIDEO_TYPE)/* | sort | tail -n 1)/vtune_results/call_tree.html

decode_ffmpeg:
	LD_LIBRARY_PATH=$(CUSTOM_PREFIX)/lib:$$LD_LIBRARY_PATH $(CUSTOM_PREFIX)/bin/ffmpeg -y -i $(VIDEO_FILE) -c copy -an $(LAST_RESULTS_DIR)/decoded_output.mp4

# =============================================================================
# PUBLISHING & VIDEO GENERATION
# =============================================================================

publish:
	$(PYTHON) -m publishing.publish_report 3 $(INITIAL_RUN_DATA) $(LAST_RESULTS_DIR) $(VIDEO_TYPE) test_git test_git 1

generate_video:
	$(PYTHON) -m video_generation.combine_motion_vectors_with_video $(VIDEO_FILE) $(CSV_FILE_PATH_ORIG) $(CSV_FILE_PATH_CUST) $(LAST_RESULTS_DIR)
	$(PYTHON) -m video_generation.generate_motion_vectors_video $(CSV_FILE_PATH_CUST) $(LAST_RESULTS_DIR)