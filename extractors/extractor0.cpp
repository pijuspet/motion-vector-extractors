#include <stdio.h>
#include "writer.h"
#include <iostream>

extern "C" {
    #include <libavutil/motion_vector.h>
    #include <libavformat/avformat.h>
    #include <libavcodec/avcodec.h>
}

int main(int argc, char **argv) {
    AVFormatContext *fmt_ctx = NULL;
    AVCodecContext *dec_ctx = NULL;
    AVPacket *pkt = NULL;
    AVFrame *frame = NULL;
    int video_idx, frame_num = 0;

    std::string file_name = "";
    int do_print = 1;
    if (argc < 2) { printf("Usage: %s <input> [print]\n", argv[0]); return 1; }
    if (argc >= 3) do_print = atoi(argv[2]);
    if (argc >= 4) file_name = argv[3];


    avformat_network_init();
    avformat_open_input(&fmt_ctx, argv[1], NULL, NULL);
    avformat_find_stream_info(fmt_ctx, NULL);
    video_idx = av_find_best_stream(fmt_ctx, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    dec_ctx = avcodec_alloc_context3(NULL);
    // dec_ctx->thread_count = 1; in c
    avcodec_parameters_to_context(dec_ctx, fmt_ctx->streams[video_idx]->codecpar);
    AVDictionary *opts = NULL; av_dict_set(&opts, "flags2", "+export_mvs", 0);
    avcodec_open2(dec_ctx, avcodec_find_decoder(dec_ctx->codec_id), &opts);
    pkt = av_packet_alloc();
    frame = av_frame_alloc();

    MotionVectorWriter writer;
    if (do_print) {
        if (!writer.Open(file_name)) {
            fprintf(stderr, "Failed to open output file\n");
            return 1;
        }
    }
    while (av_read_frame(fmt_ctx, pkt) >= 0) {
        if (pkt->stream_index == video_idx) {
            avcodec_send_packet(dec_ctx, pkt);
            while (avcodec_receive_frame(dec_ctx, frame) >= 0) {
                AVFrameSideData *sd = av_frame_get_side_data(frame, AV_FRAME_DATA_MOTION_VECTORS);
                if (do_print){
                    if (sd && sd->data && sd->size > 0) {
                        writer.Write(frame_num, (const AVMotionVector*)sd->data, 0, sd->size);
                    } else {
                        std::cerr << "frame " << frame_num << ": no motion vectors\n";
                    }
                }
                    
                frame_num++;
                av_frame_unref(frame);
            }
        }
        av_packet_unref(pkt);
    }

    avcodec_free_context(&dec_ctx); avformat_close_input(&fmt_ctx);
    av_frame_free(&frame); av_packet_free(&pkt);
    return 0;
}
