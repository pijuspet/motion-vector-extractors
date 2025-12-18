#include <stdio.h>
#include <stdlib.h>
#include <inttypes.h>
#include "writer.h"

extern "C" {
    #include <libavcodec/avcodec.h>
    #include <libavformat/avformat.h>
    #include <libavutil/motion_vector.h>
    #include <libavutil/opt.h>
}

int main(int argc, char** argv) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <input>\n", argv[0]);
        return -1;
    }

    std::string file_name = "";
    int do_print = 1;
    if (argc >= 3) do_print = atoi(argv[2]);
    if (argc >= 4) file_name = argv[3];

    avformat_network_init();

    AVFormatContext* fmt_ctx = NULL;
    if (avformat_open_input(&fmt_ctx, argv[1], NULL, NULL) < 0) {
        fprintf(stderr, "Could not open input file.\n");
        return -1;
    }

    avformat_find_stream_info(fmt_ctx, NULL);

    int video_stream_index = -1;
    for (unsigned i = 0; i < fmt_ctx->nb_streams; i++) {
        if (fmt_ctx->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            video_stream_index = i;
            break;
        }
    }

    MotionVectorWriter writer;
    if (do_print) {
        if (!writer.Open(file_name)) {
            fprintf(stderr, "Failed to open output file\n");
            return 1;
        }
    }

    const AVCodec* codec = avcodec_find_decoder(fmt_ctx->streams[video_stream_index]->codecpar->codec_id);
    AVCodecContext* codec_ctx = avcodec_alloc_context3(codec);
    avcodec_parameters_to_context(codec_ctx, fmt_ctx->streams[video_stream_index]->codecpar);

    codec_ctx->export_side_data = AV_CODEC_EXPORT_DATA_MVS;
    av_opt_set_int(codec_ctx, "motion_vectors_only", 1, 0);  // CUSTOM PATCHED FLAG

    avcodec_open2(codec_ctx, codec, NULL);

    AVPacket* pkt = av_packet_alloc();
    AVFrame* frame = av_frame_alloc();

    int frame_idx = 0;

    while (av_read_frame(fmt_ctx, pkt) >= 0) {
        if (pkt->stream_index == video_stream_index) {
            avcodec_send_packet(codec_ctx, pkt);
            while (avcodec_receive_frame(codec_ctx, frame) == 0) {
                AVFrameSideData* sd = av_frame_get_side_data(frame, AV_FRAME_DATA_MOTION_VECTORS);
                if (sd) {
                    if (do_print)
                        writer.Write(frame_idx, (const AVMotionVector*)sd->data, 8, sd->size);
                }
                av_frame_unref(frame);
                frame_idx++;
            }
        }
        av_packet_unref(pkt);
    }

    av_frame_free(&frame);
    av_packet_free(&pkt);
    avcodec_free_context(&codec_ctx);
    avformat_close_input(&fmt_ctx);
    return 0;
}

