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

    int do_print = 1;
    if (argc >= 3)
        do_print = atoi(argv[2]);


    std::string file_name = "";
    if (argc >= 4) 
        file_name = argv[3];

    avformat_network_init();

    AVFormatContext* fmt_ctx = NULL;
    if (avformat_open_input(&fmt_ctx, argv[1], NULL, NULL) < 0) {
        fprintf(stderr, "Could not open input file.\n");
        return -1;
    }

    if (avformat_find_stream_info(fmt_ctx, NULL) < 0) {
        fprintf(stderr, "Could not find stream info.\n");
        return -1;
    }

    int video_stream_index = -1;
    for (unsigned i = 0; i < fmt_ctx->nb_streams; i++) {
        if (fmt_ctx->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            video_stream_index = i;
            break;
        }
    }

    if(video_stream_index < 0) {
        fprintf(stderr, "No video stream found.\n");
        return -1;
    }

    const AVCodec* codec = avcodec_find_decoder(fmt_ctx->streams[video_stream_index]->codecpar->codec_id);
    if(!codec){
        fprintf(stderr, "Codec not found.\n");
        return -1;
    }

    AVCodecContext* codec_ctx = avcodec_alloc_context3(codec);
    if(!codec_ctx){
        fprintf(stderr, "Could not allocate codec context.\n");
        return -1;
    }

    if (avcodec_parameters_to_context(codec_ctx, fmt_ctx->streams[video_stream_index]->codecpar) < 0) {
        fprintf(stderr, "Failed to copy codec parameters to codec context.\n");
        return -1;
    }

    // Enable multi-threaded decoding

    // codec_ctx->thread_count = 1; // set in c version
    codec_ctx->thread_count = 0; // 0 lets ffmpeg decide based on CPU cores
    codec_ctx->export_side_data |= AV_CODEC_EXPORT_DATA_MVS;
    av_opt_set_int(codec_ctx, "motion_vectors_only", 1, 0);  // CUSTOM PATCHED FLAG

    if (avcodec_open2(codec_ctx, codec, NULL) < 0) {
        fprintf(stderr, "Could not open codec.\n");
        return -1;
    }

    AVPacket* pkt = av_packet_alloc();
    AVFrame* frame = av_frame_alloc();
    if(!pkt || !frame) {
        fprintf(stderr, "Could not allocate packet or frame.\n");
        return -1;
    }

    int frame_idx = 0;

    MotionVectorWriter writer;
    if (do_print) {
        if (!writer.Open(file_name)) {
            fprintf(stderr, "Failed to open output file\n");
            return 1;
        }
    }

    while (av_read_frame(fmt_ctx, pkt) >= 0) {
        if (pkt->stream_index == video_stream_index) {
            int ret = avcodec_send_packet(codec_ctx, pkt);
            if (ret < 0) {
                fprintf(stderr, "Error sending packet for decoding: %d\n", ret);
                break;
            }

            // Receive all available frames
            while (ret >= 0) {
                ret = avcodec_receive_frame(codec_ctx, frame);
                if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF)
                    break;
                else if (ret < 0) {
                    fprintf(stderr, "Error during decoding.\n");
                    break;
                }

                AVFrameSideData* sd = av_frame_get_side_data(frame, AV_FRAME_DATA_MOTION_VECTORS);
                if (sd) {
                    if(do_print)
                        writer.Write(frame_idx, (const AVMotionVector*)sd->data, 1, sd->size);
                }

                av_frame_unref(frame);
                frame_idx++;
            }
        }
        av_packet_unref(pkt);
    }

    // Flush decoder
    avcodec_send_packet(codec_ctx, NULL);
    while (avcodec_receive_frame(codec_ctx, frame) == 0) {
        AVFrameSideData* sd = av_frame_get_side_data(frame, AV_FRAME_DATA_MOTION_VECTORS);
        if (sd) {
            if(do_print)
                writer.Write(frame_idx, (const AVMotionVector*)sd->data, 1, sd->size);
        }
        av_frame_unref(frame);
        frame_idx++;
    }

    av_frame_free(&frame);
    av_packet_free(&pkt);
    avcodec_free_context(&codec_ctx);
    avformat_close_input(&fmt_ctx);

    return 0;
}

