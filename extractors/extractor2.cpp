extern "C" {
    #include <libavutil/motion_vector.h>
    #include <libavutil/opt.h>
    #include <libavformat/avformat.h>
    #include <libavcodec/avcodec.h>
}
#include <stdio.h>
#include <stdlib.h>
#include <inttypes.h> // for PRIx64
#include "writer.h"

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s rtsp://host:port/stream\n", argv[0]);
        return 1;
    }
    
    std::string file_name = "";
    int do_print = 1;
    if (argc >= 3) do_print = atoi(argv[2]);
    if (argc >= 4) file_name = argv[3];

    AVFormatContext *fmt_ctx = NULL;
    AVCodecContext *dec_ctx = NULL;
    int video_stream_idx = -1, frame_count = 0;
    AVPacket *pkt = av_packet_alloc();
    AVFrame *frame = av_frame_alloc();

    // Open RTSP input with options
    AVDictionary *opts = NULL;
    av_dict_set(&opts, "rtsp_transport", "udp", 0);
    av_dict_set(&opts, "stimeout", "2500000", 0);
    av_dict_set(&opts, "buffer_size", "32768", 0);

    if (avformat_open_input(&fmt_ctx, argv[1], NULL, &opts) < 0) {
        fprintf(stderr, "Could not open input\n");
        return 1;
    }
    av_dict_free(&opts);

    if (avformat_find_stream_info(fmt_ctx, NULL) < 0) {
        fprintf(stderr, "Could not find stream info\n");
        return 1;
    }

    // Find video stream index
    video_stream_idx = av_find_best_stream(fmt_ctx, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (video_stream_idx < 0) {
        fprintf(stderr, "Could not find video stream\n");
        return 1;
    }

    AVStream *video_stream = fmt_ctx->streams[video_stream_idx];
    const AVCodec *dec = avcodec_find_decoder(video_stream->codecpar->codec_id);
    if (!dec) {
        fprintf(stderr, "Decoder not found\n");
        return 1;
    }

    dec_ctx = avcodec_alloc_context3(dec);
    avcodec_parameters_to_context(dec_ctx, video_stream->codecpar);

    // ✅ Use patched FFmpeg: motion_vectors_only + side data export
    dec_ctx->export_side_data = AV_CODEC_EXPORT_DATA_MVS;
    av_opt_set_int(dec_ctx, "motion_vectors_only", 1, 0);

    // Optional: use single-threaded mode for more predictable latency
    dec_ctx->thread_count = 1;

    if (avcodec_open2(dec_ctx, dec, NULL) < 0) {
        fprintf(stderr, "Could not open codec\n");
        return 1;
    }

    MotionVectorWriter writer;
    if (do_print){
        if (!writer.Open(file_name)) {
            fprintf(stderr, "Failed to open output file\n");
            return 1;
        }
    }


    while (av_read_frame(fmt_ctx, pkt) >= 0) {
        if (pkt->stream_index == video_stream_idx) {
            if (avcodec_send_packet(dec_ctx, pkt) < 0) {
                av_packet_unref(pkt);
                continue;
            }

            while (avcodec_receive_frame(dec_ctx, frame) >= 0) {
                AVFrameSideData *sd = av_frame_get_side_data(frame, AV_FRAME_DATA_MOTION_VECTORS);
                if (sd) {
                    // const AVMotionVector *mvs = (const AVMotionVector *)sd->data;
                    // int n = sd->size / sizeof(*mvs);
                    // for (int i = 0; i < n; i++) {
                    //     const AVMotionVector *mv = &mvs[i];
                    //     /*printf("%d,2,%d,%d,%d,%d,%d,%d,%d,0x%" PRIx64 ",%d,%d,%d\n",
                    //         frame_count, mv->source, mv->w, mv->h,
                    //         mv->src_x, mv->src_y,
                    //         mv->dst_x, mv->dst_y,
                    //         (uint64_t)mv->flags,
                    //         mv->motion_x, mv->motion_y, mv->motion_scale);*/
                    // }
                    if(do_print)
                        writer.Write(frame_count, (const AVMotionVector*)sd->data, 2, sd->size);
                }
                frame_count++;
                av_frame_unref(frame);
            }
        }
        av_packet_unref(pkt);
    }

    avcodec_free_context(&dec_ctx);
    avformat_close_input(&fmt_ctx);
    av_frame_free(&frame);
    av_packet_free(&pkt);

    return 0;
}

