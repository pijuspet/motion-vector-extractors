#include <stdio.h>
#include "writer.h"

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/motion_vector.h>
}

int main(int argc, char** argv) {
    AVFormatContext* fmt_ctx = NULL;
    AVCodecContext* dec_ctx = NULL;
    AVPacket* pkt = NULL;
    AVFrame* frame = NULL;
    int video_stream_index = -1;
    int frame_num = 0;
    bool do_print = 1;
    bool is_single_threaded = 0;
    bool is_verbose = 1;
    std::string file_name = "";
    std::string video_file = "";

    if (argc < 6) {
        fprintf(stderr, "Usage: %s <input file> <print mv> <output file>, <extractor index> <is verbose> <is single threaded> \n", argv[0]);
        return -1;
    }

    video_file = argv[1];
    do_print = atoi(argv[2]);
    file_name = argv[3];
    is_verbose = atoi(argv[4]);
    is_single_threaded = atoi(argv[5]);

    avformat_network_init();

    if (avformat_open_input(&fmt_ctx, video_file.c_str(), NULL, NULL) < 0) {
        fprintf(stderr, "Could not open input file.\n");
        return -1;
    }

    if (avformat_find_stream_info(fmt_ctx, NULL) < 0) {
        fprintf(stderr, "Could not find stream info.\n");
        return -1;
    }

    //region video stream 
    video_stream_index = av_find_best_stream(fmt_ctx, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);

    if (video_stream_index < 0) {
        fprintf(stderr, "Could not find video stream\n");
        return -1;
    }

    AVStream* video_stream = fmt_ctx->streams[video_stream_index];
    //endregion

    //region codec
    const AVCodec* codec = NULL;
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
    dec_ctx->thread_count = is_single_threaded; // 0 lets ffmpeg decide based on CPU cores
    av_dict_set(&opts, "flags2", "+export_mvs", 0);
    //endregion

    if (avcodec_open2(dec_ctx, avcodec_find_decoder(dec_ctx->codec_id), &opts) < 0) {
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

    if (is_verbose)
        fprintf(stderr, "FFmpeg version: %s\n", av_version_info());

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
                        writer.Write(frame_num, (const AVMotionVector*)sd->data, sd->size);
                    }
                    else {
                        if (is_verbose)
                            fprintf(stderr, "Frame %d: no motion vectors\n", frame_num);
                    }
                }

                av_frame_unref(frame);
                frame_num++;
            }
        }
        av_packet_unref(pkt);
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
