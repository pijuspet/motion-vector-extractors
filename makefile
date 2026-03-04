CC = g++

CURRENT_DIR := ${shell pwd}
PARENT_DIR  := $(shell dirname $(CURRENT_DIR))
VENV_FOLDER = $(PARENT_DIR)/venv-motion-vectors
PYTHON = $(VENV_FOLDER)/bin/python

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

CUSTOM_PREFIX    := $(abspath $(CURRENT_DIR)/ffmpeg/FFmpeg-8.0-custom)
REGULAR_PREFIX   := $(abspath $(CURRENT_DIR)/ffmpeg/FFmpeg-8.0)
$(eval $(call def_ff_flags,$(CUSTOM_PREFIX),CUST_FF))
$(eval $(call def_ff_flags,$(REGULAR_PREFIX),SYS_FF))

EXTRACTOR_DIR = extractors
BENCHMARKING_DIR = benchmarking
EXECUTABLES_DIR = executables
WRITER_SRC = $(EXTRACTOR_DIR)/writer.cpp -Iextractors

VIDEO_TYPE = h264_cabac
# VIDEO_TYPE = h264_cavlc
# VIDEO_TYPE = h264_avi
# VIDEO_TYPE = h265

VIDEO_FILE = $(CURRENT_DIR)/videos/$(VIDEO_TYPE)/bigbunny_walking.mp4
# VIDEO_FILE = $(CURRENT_DIR)/videos/$(VIDEO_TYPE)/bigbunny.mp4
# VIDEO_FILE = $(CURRENT_DIR)/videos/$(VIDEO_TYPE)/stickman.mp4
# VIDEO_FILE = $(CURRENT_DIR)/videos/$(VIDEO_TYPE)/dashcam.mp4

INITIAL_RUN_DATA = $(CURRENT_DIR)/published/$(VIDEO_TYPE)/initial_results_$(VIDEO_TYPE)
LAST_RESULTS_DIR = $(shell ls -d $(CURRENT_DIR)/results/$(VIDEO_TYPE)/* | sort | tail -n 1)

CSV_FILE_PATH_ORIG = $(LAST_RESULTS_DIR)/method0_output_0.csv # original ffmpeg
CSV_FILE_PATH_CUST = $(LAST_RESULTS_DIR)/method4_output_0.csv # custom ffmpeg


install:
	apt install -y build-essential gcc g++ make pkg-config nasm
	cp -n .env_template .env
	mkdir -p $(VENV_FOLDER)
	mkdir -p $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)
	mkdir -p $(BENCHMARKING_DIR)/$(EXECUTABLES_DIR)
	python3 -m venv $(VENV_FOLDER)
	. $(VENV_FOLDER)/bin/activate && pip install -r requirements.txt

all:
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor0 $(EXTRACTOR_DIR)/extractor0.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor1 $(EXTRACTOR_DIR)/extractor1.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor2 $(EXTRACTOR_DIR)/extractor2.cpp  $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor3 $(EXTRACTOR_DIR)/extractor3.cpp $(WRITER_SRC) $(CUST_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor4 $(EXTRACTOR_DIR)/extractor1.cpp $(WRITER_SRC) $(CUST_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXECUTABLES_DIR)/extractor5 $(EXTRACTOR_DIR)/extractor5.cpp $(WRITER_SRC) $(CUST_FF)

FFMPEG_BUILD = \
	cd $1/FFmpeg && \
	chmod +x ./configure ./ffbuild/*.sh && \
	./configure --prefix=$(abspath $1) --enable-shared --enable-swresample --enable-debug --disable-stripping --pkg-config-flags="--static" && \
	make && make install

setup_ffmpeg:
	$(call FFMPEG_BUILD,$(CUSTOM_PREFIX))
	$(call FFMPEG_BUILD,$(REGULAR_PREFIX))

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

benchmark:
	$(PYTHON) -m benchmarking.full_benchmark $(VIDEO_FILE) 15 $(VIDEO_TYPE)

publish:
	$(PYTHON) -m publishing.publish_report 3 $(INITIAL_RUN_DATA) $(LAST_RESULTS_DIR) $(VIDEO_TYPE) test_git test_git 1
	
generate_video:
	$(PYTHON) -m video_generation.combine_motion_vectors_with_video $(VIDEO_FILE) $(CSV_FILE_PATH_ORIG) $(CSV_FILE_PATH_CUST) $(LAST_RESULTS_DIR)
	$(PYTHON) -m video_generation.generate_motion_vectors_video $(CSV_FILE_PATH_CUST) $(LAST_RESULTS_DIR)

decode_ffmpeg:
	LD_LIBRARY_PATH=$(CUSTOM_PREFIX)/lib:$$LD_LIBRARY_PATH $(CUSTOM_PREFIX)/bin/ffmpeg -y -i $(VIDEO_FILE) -c copy -an $(LAST_RESULTS_DIR)/decoded_output.mp4

test_ffmpeg:
	$(call FFMPEG_BUILD,$(CUSTOM_PREFIX))
	$(PYTHON) -m benchmarking.full_benchmark $(VIDEO_FILE) 1 $(VIDEO_TYPE) 1 2 5
# 	chromium --no-sandbox $(shell ls -d $(CURRENT_DIR)/results/$(VIDEO_TYPE)/* | sort | tail -n 1)/vtune_results/call_tree.html