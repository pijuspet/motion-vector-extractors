#include <stdio.h>
#include "writer.h"

#include <inttypes.h>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/motion_vector.h>
#include <libavutil/opt.h>
}

int main(int argc, char** argv) {
    AVFormatContext* fmt_ctx = NULL;
    AVCodecContext* dec_ctx = NULL;
    AVPacket* pkt = NULL;
    AVFrame* frame = NULL;
    int video_stream_index = -1;
    int frame_num = 0;
    int do_print = 1;
    std::string file_name = "";

    if (argc < 2) {
        fprintf(stderr, "Usage: %s <input>\n", argv[0]);
        return -1;
    }
    if (argc >= 3)
        do_print = atoi(argv[2]);
    if (argc >= 4)
        file_name = argv[3];

    avformat_network_init();

    if (avformat_open_input(&fmt_ctx, argv[1], NULL, NULL) < 0) {
        fprintf(stderr, "Could not open input file.\n");
        return -1;
    }

    if (avformat_find_stream_info(fmt_ctx, NULL) < 0) {
        fprintf(stderr, "Could not find stream info.\n");
        return -1;
    }

    //region video stream 
    for (unsigned i = 0; i < fmt_ctx->nb_streams; i++) {
        if (fmt_ctx->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            video_stream_index = i;
            break;
        }
    }

    if (video_stream_index < 0) {
        fprintf(stderr, "Could not find video stream\n");
        return -1;
    }

    AVStream* video_stream = fmt_ctx->streams[video_stream_index];
    //endregion

    //region codec
    const AVCodec* codec = avcodec_find_decoder(video_stream->codecpar->codec_id);
    
    if (!codec) {
        fprintf(stderr, "Codec not found.\n");
        return -1;
    }
    //endregion

    dec_ctx = avcodec_alloc_context3(codec);
    if (!dec_ctx) {
        fprintf(stderr, "Could not allocate codec context.\n");
        return -1;
    }

    if (avcodec_parameters_to_context(dec_ctx, video_stream->codecpar) < 0) {
        fprintf(stderr, "Failed to copy codec parameters to codec context.\n");
        return -1;
    }

    //region flag setting
    AVDictionary* opts = NULL;
    dec_ctx->thread_count = 0; // 0 lets ffmpeg decide based on CPU cores
    // dec_ctx->thread_count = 1; // set in c version
    dec_ctx->export_side_data |= AV_CODEC_EXPORT_DATA_MVS;
    av_opt_set_int(dec_ctx, "motion_vectors_only", 1, 0); // CUSTOM PATCHED FLAG
    //endregion

    if (avcodec_open2(dec_ctx, codec, &opts) < 0) {
        fprintf(stderr, "Could not open codec.\n");
        return -1;
    }

    pkt = av_packet_alloc();
    frame = av_frame_alloc();

    if (!pkt || !frame) {
        fprintf(stderr, "Could not allocate packet or frame.\n");
        return -1;
    }

    MotionVectorWriter writer;
    if (do_print) {
        if (!writer.Open(file_name)) {
            fprintf(stderr, "Failed to open output file\n");
            return -1;
        }
    }

    while (av_read_frame(fmt_ctx, pkt) >= 0) {
        if (pkt->stream_index == video_stream_index) {
            int ret = avcodec_send_packet(dec_ctx, pkt);
            if (ret < 0) {
                fprintf(stderr, "Error sending packet for decoding: %d\n", ret);
                break;
            }

            while (ret >= 0) {
                ret = avcodec_receive_frame(dec_ctx, frame);
                if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF)
                    break;
                else if (ret < 0) {
                    fprintf(stderr, "Error during decoding.\n");
                    break;
                }

                AVFrameSideData* sd = av_frame_get_side_data(frame, AV_FRAME_DATA_MOTION_VECTORS);
                if (do_print) {
                    if (sd && sd->data && sd->size > 0) {
                        writer.Write(frame_num, (const AVMotionVector*)sd->data, 1, sd->size);
                    }
                    else {
                        fprintf(stderr, "Frame %d: no motion vectors\n", frame_num);
                    }
                }

                av_frame_unref(frame);
                frame_num++;
            }
        }
        av_packet_unref(pkt);
    }

    // Flush decoder
    avcodec_send_packet(dec_ctx, NULL);
    while (avcodec_receive_frame(dec_ctx, frame) == 0) {
        AVFrameSideData* sd = av_frame_get_side_data(frame, AV_FRAME_DATA_MOTION_VECTORS);
        if (sd) {
            if (do_print)
                writer.Write(frame_num, (const AVMotionVector*)sd->data, 1, sd->size);
        }
        av_frame_unref(frame);
        frame_num++;
    }

    avcodec_free_context(&dec_ctx);
    avformat_close_input(&fmt_ctx);
    av_frame_free(&frame);
    av_packet_free(&pkt);

    fprintf(stdout, "%d %d\n", frame_num, writer.GetTotalMVs());
    fflush(stdout);
    writer.Close();
    return 0;
}
