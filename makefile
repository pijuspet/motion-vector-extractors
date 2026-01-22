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

VIDEO_FILE = $(CURRENT_DIR)/videos/vid_h264.mp4
LAST_RESULTS_DIR = $(shell ls -d $(CURRENT_DIR)/results/* | sort | tail -n 1)
CSV_FILE_PATH_ORIG = $(LAST_RESULTS_DIR)/method0_output_0.csv # original ffmpeg
CSV_FILE_PATH_CUST = $(LAST_RESULTS_DIR)/method4_output_0.csv # custom ffmpeg

install:
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
	./configure --prefix=$(abspath $1) --enable-shared --enable-swresample --pkg-config-flags="--static" && \
	make && make install

setup_ffmpeg:
	$(call FFMPEG_BUILD,$(CUSTOM_PREFIX))
	$(call FFMPEG_BUILD,$(REGULAR_PREFIX))

benchmark:
	$(PYTHON) -m benchmarking.full_benchmark $(VIDEO_FILE) 15

publish:
	$(PYTHON) -m publishing.publish_report 2 $(CURRENT_DIR)/results/20251231_1312 $(CURRENT_DIR)/results/20260105_1115 test_git test_git
	
generate_video:
	$(PYTHON) -m video_generation.combine_motion_vectors_with_video $(VIDEO_FILE) $(CSV_FILE_PATH_ORIG) $(CSV_FILE_PATH_CUST) $(LAST_RESULTS_DIR)
	$(PYTHON) -m video_generation.generate_motion_vectors_video $(CSV_FILE_PATH_CUST) $(LAST_RESULTS_DIR)
