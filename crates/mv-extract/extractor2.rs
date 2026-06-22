use std::ffi::CString;
use std::ptr;

use ffmpeg_sys_next as ff;

use mv_extract::ffmpeg_common::{get_current_rss_kb, ExtractorArgs};

fn main() {
    let Some(args) = ExtractorArgs::from_env() else {
        std::process::exit(255);
    };

    unsafe {
        ff::avformat_network_init();

        let mut fmt_ctx: *mut ff::AVFormatContext = ptr::null_mut();
        let c_video = CString::new(args.video_file.as_str()).unwrap();
        if ff::avformat_open_input(
            &mut fmt_ctx,
            c_video.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
        ) < 0
        {
            eprintln!("Could not open input file.");
            std::process::exit(255);
        }

        if ff::avformat_find_stream_info(fmt_ctx, ptr::null_mut()) < 0 {
            eprintln!("Could not find stream info.");
            std::process::exit(255);
        }

        //region video stream
        let vsi = ff::av_find_best_stream(
            fmt_ctx,
            ff::AVMediaType::AVMEDIA_TYPE_VIDEO,
            -1,
            -1,
            ptr::null_mut(),
            0,
        );
        if vsi < 0 {
            eprintln!("Could not find video stream");
            std::process::exit(255);
        }

        let video_stream = *(*fmt_ctx).streams.add(vsi as usize);
        if args.keyframes_only {
            (*video_stream).discard = ff::AVDiscard::AVDISCARD_NONKEY;
        }
        //endregion

        let dec_ctx = ff::avcodec_alloc_context3(ptr::null());
        if dec_ctx.is_null() {
            eprintln!("Could not allocate codec context.");
            std::process::exit(255);
        }
        if ff::avcodec_parameters_to_context(dec_ctx, (*video_stream).codecpar) < 0 {
            eprintln!("Failed to copy codec parameters to codec context.");
            std::process::exit(255);
        }

        //region flag setting
        let mut opts: *mut ff::AVDictionary = ptr::null_mut();
        (*dec_ctx).thread_count = args.thread_count;
        // (*dec_ctx).thread_type = ff::FF_THREAD_SLICE as i32;
        if args.keyframes_only {
            (*dec_ctx).skip_frame = ff::AVDiscard::AVDISCARD_NONKEY;
        }
        //endregion

        //region codec
        let codec = ff::avcodec_find_decoder((*dec_ctx).codec_id);
        //endregion

        if ff::avcodec_open2(dec_ctx, codec, &mut opts) < 0 {
            eprintln!("Could not open codec.");
            std::process::exit(255);
        }

        let pkt = ff::av_packet_alloc();
        let frame = ff::av_frame_alloc();
        if pkt.is_null() || frame.is_null() {
            eprintln!("Could not allocate packet or frame.");
            std::process::exit(255);
        }

        let mut frame_num: i32 = 0;
        while ff::av_read_frame(fmt_ctx, pkt) >= 0 {
            if (*pkt).stream_index == vsi {
                let mut ret = ff::avcodec_send_packet(dec_ctx, pkt);
                if ret < 0 {
                    eprintln!("Error sending packet for decoding: {}", ret);
                    break;
                }
                while ret >= 0 {
                    ret = ff::avcodec_receive_frame(dec_ctx, frame);
                    if ret == ff::AVERROR(libc::EAGAIN) || ret == ff::AVERROR_EOF {
                        break;
                    } else if ret < 0 {
                        eprintln!("Error during decoding.");
                        break;
                    }
                    ff::av_frame_unref(frame);
                    frame_num += 1;
                }
            }
            ff::av_packet_unref(pkt);
        }

        let rss_kb = get_current_rss_kb();

        let mut dec_ctx_ptr = dec_ctx;
        ff::avcodec_free_context(&mut dec_ctx_ptr);
        ff::avformat_close_input(&mut fmt_ctx);
        let mut frame_ptr = frame;
        ff::av_frame_free(&mut frame_ptr);
        let mut pkt_ptr = pkt;
        ff::av_packet_free(&mut pkt_ptr);

        println!("{} {} {}", frame_num, 0, rss_kb);
    }
}
