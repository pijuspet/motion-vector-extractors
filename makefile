CC = gcc

# Custom FFmpeg install path
CUSTOM_PREFIX = /home/loab/Documents/motion-vector-extractors-/ffmpeg-8.1/hacked
CUSTOM_PKG_CONFIG = PKG_CONFIG_PATH=$(CUSTOM_PREFIX)/lib/pkgconfig
CUST_FF_CFLAGS = $(shell $(CUSTOM_PKG_CONFIG) pkg-config --cflags libavformat libavcodec libavutil)
CUST_FF_LIBS   = $(shell $(CUSTOM_PKG_CONFIG) pkg-config --libs libavformat libavcodec libavutil)
CUST_RPATH = -Wl,-rpath,$(CUSTOM_PREFIX)/lib
CUST_FF = $(CUST_FF_CFLAGS) $(CUST_FF_LIBS) $(CUST_RPATH)

# Original FFmpeg install path
ORIG_PREFIX = /home/loab/Documents/motion-vector-extractors-/ffmpeg-8.1/clean-version
ORIG_PKG_CONFIG = PKG_CONFIG_PATH=$(ORIG_PREFIX)/lib/pkgconfig
ORIG_FF_CFLAGS = $(shell $(ORIG_PKG_CONFIG) pkg-config --cflags libavformat libavcodec libavutil)
ORIG_FF_LIBS   = $(shell $(ORIG_PKG_CONFIG) pkg-config --libs libavformat libavcodec libavutil)
ORIG_RPATH = -Wl,-rpath,$(ORIG_PREFIX)/lib
ORIG_FF = $(ORIG_FF_CFLAGS) $(ORIG_FF_LIBS) $(ORIG_RPATH)

all:
	$(CC) -O2 -o extractor0 extractors/extractor0.c $(ORIG_FF)
	$(CC) -O2 -o extractor1 extractors/extractor1.c $(ORIG_FF)
	$(CC) -O2 -o extractor2 extractors/extractor2.c $(ORIG_FF)
	$(CC) -O2 -o extractor3 extractors/extractor3.c $(ORIG_FF)
	$(CC) -O2 -o extractor4 extractors/extractor4.c $(ORIG_FF)
	$(CC) -O2 -o extractor5 extractors/extractor5.c $(ORIG_FF)
	$(CC) -O2 -o extractor6 extractors/extractor6.c $(ORIG_FF)
	$(CC) -O2 -o extractor7 extractors/extractor7.c $(CUST_FF)
	$(CC) -O2 -o extractor8 extractors/extractor8.c $(CUST_FF)
	echo $(CUSTOM_PREFIX) > custom
	
clean:
	rm -f extractor*

