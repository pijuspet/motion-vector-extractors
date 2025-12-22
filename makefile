CURRENT_DIR=${shell pwd}

CC = g++

# Custom FFmpeg install path
CUSTOM_PREFIX = $(CURRENT_DIR)/ffmpeg/FFmpeg-8.0-custom
CUSTOM_PKG_CONFIG = PKG_CONFIG_PATH=$(CUSTOM_PREFIX)/lib/pkgconfig
CUST_FF_CFLAGS = $(shell $(CUSTOM_PKG_CONFIG) pkg-config --cflags libavformat libavcodec libavutil)
CUST_FF_LIBS   = $(shell $(CUSTOM_PKG_CONFIG) pkg-config --libs libavformat libavcodec libavutil)
CUST_RPATH = -Wl,-rpath,$(CUSTOM_PREFIX)/lib
CUST_FF = $(CUST_FF_CFLAGS) $(CUST_FF_LIBS) $(CUST_RPATH)

# Original FFmpeg install path
REGULAR_PREFIX = $(CURRENT_DIR)/ffmpeg/FFmpeg-8.0
REGULAR_PKG_CONFIG = PKG_CONFIG_PATH=$(REGULAR_PREFIX)/lib/pkgconfig
SYS_FF_CFLAGS = $(shell $(REGULAR_PKG_CONFIG) pkg-config --cflags libavformat libavcodec libavutil)
SYS_FF_LIBS   = $(shell $(REGULAR_PKG_CONFIG) pkg-config --libs libavformat libavcodec libavutil)
SYS_RPATH = -Wl,-rpath,$(REGULAR_PREFIX)/lib
SYS_FF = $(SYS_FF_CFLAGS) $(SYS_FF_LIBS) $(SYS_RPATH)	

EXTRACTOR_DIR = extractors
EXTRACTOR_EXECUTABLES_DIR = executables
WRITER_SRC = $(EXTRACTOR_DIR)/writer.cpp -Iextractors

VIDEO_FILE = $(CURRENT_DIR)/videos/vid_h264.mp4
LAST_RESULTS_DIR = $(shell ls -d $(CURRENT_DIR)/results/* | sort | tail -n 1)
CSV_FILE_PATH = $(LAST_RESULTS_DIR)/all_motion_vectors.csv

PARENT_DIR  := $(shell dirname $(CURRENT_DIR))
VENV_FOLDER = $(PARENT_DIR)/venv-motion-vectors

install:
	mkdir -p $(VENV_FOLDER)
	. $(VENV_FOLDER)/bin/activate && pip install -r requirements.txt

all:
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXTRACTOR_EXECUTABLES_DIR)/extractor0 $(EXTRACTOR_DIR)/extractor0.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXTRACTOR_EXECUTABLES_DIR)/extractor1 $(EXTRACTOR_DIR)/extractor1.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXTRACTOR_EXECUTABLES_DIR)/extractor2 $(EXTRACTOR_DIR)/extractor2.cpp $(WRITER_SRC) $(CUST_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXTRACTOR_EXECUTABLES_DIR)/extractor3 $(EXTRACTOR_DIR)/extractor3.cpp $(WRITER_SRC) $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXTRACTOR_EXECUTABLES_DIR)/extractor4 $(EXTRACTOR_DIR)/extractor4.cpp  $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXTRACTOR_EXECUTABLES_DIR)/extractor5 $(EXTRACTOR_DIR)/extractor5.cpp  $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXTRACTOR_EXECUTABLES_DIR)/extractor6 $(EXTRACTOR_DIR)/extractor6.cpp  $(SYS_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXTRACTOR_EXECUTABLES_DIR)/extractor7 $(EXTRACTOR_DIR)/extractor7.cpp $(WRITER_SRC) $(CUST_FF)
	$(CC) -O2 -o $(EXTRACTOR_DIR)/$(EXTRACTOR_EXECUTABLES_DIR)/extractor8 $(EXTRACTOR_DIR)/extractor8.cpp $(WRITER_SRC) $(CUST_FF)

FFMPEG_BUILD = \
	cd $1 && \
	chmod +x ./configure ./ffbuild/*.sh && \
	./configure --prefix=../ --enable-shared --pkg-config-flags="--static" && \
	make && make install

setup_ffmpeg:
	$(call FFMPEG_BUILD,$(CUSTOM_PREFIX)/FFmpeg)
	$(call FFMPEG_BUILD,$(REGULAR_PREFIX)/FFmpeg)

benchmark:
	./run_full_benchmark.sh $(VIDEO_FILE) 5

generate_video:
	python ./video_generation/combine_motion_vectors_with_video.py $(VIDEO_FILE) $(CSV_FILE_PATH) $(LAST_RESULTS_DIR)
	python ./video_generation/generate_motion_vectors_video.py $(CSV_FILE_PATH) $(LAST_RESULTS_DIR)