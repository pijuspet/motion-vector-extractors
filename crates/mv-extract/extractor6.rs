use std::ffi::{c_void, CString};
use std::ptr;

use ffmpeg_sys_next as ff;

use mv_extract::ffmpeg_common::{
    get_current_rss_kb, open_mv_any, print_ffmpeg_version, write_frame_mvs, ExtractorArgs, set_av_flags, unset_av_flags
};

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

        //region setting up motion_vectors_only for av_find_stream_info
        let mut stream_opts: Vec<*mut ff::AVDictionary> = set_av_flags(&mut *fmt_ctx);
        //endregion

        if ff::avformat_find_stream_info(fmt_ctx, stream_opts.as_mut_ptr()) < 0 {
            eprintln!("Could not find stream info.");
            std::process::exit(255);
        }

        // region, unsetting flags
        unset_av_flags(stream_opts);
        //endregion

        //region video stream
        let mut vsi: i32 = -1;
        let nb_streams = (*fmt_ctx).nb_streams as usize;
        for i in 0..nb_streams {
            let s = *(*fmt_ctx).streams.add(i);
            if (*(*s).codecpar).codec_type == ff::AVMediaType::AVMEDIA_TYPE_VIDEO {
                vsi = i as i32;
                break;
            }
        }
        if vsi < 0 {
            eprintln!("Could not find video stream");
            std::process::exit(255);
        }

        let video_stream = *(*fmt_ctx).streams.add(vsi as usize);
        //endregion

        //region codec
        let codec = ff::avcodec_find_decoder((*(*video_stream).codecpar).codec_id);
        if codec.is_null() {
            eprintln!("Codec not found.");
            std::process::exit(255);
        }
        //endregion

        let dec_ctx = ff::avcodec_alloc_context3(codec);
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
        (*dec_ctx).thread_count = if args.is_single_threaded { 1 } else { 0 };
        // (*dec_ctx).thread_type = ff::FF_THREAD_SLICE as i32;
        let flags2_key = CString::new("flags2").unwrap();
        let flags2_val = CString::new("+export_mvs").unwrap();
        ff::av_dict_set(&mut opts, flags2_key.as_ptr(), flags2_val.as_ptr(), 0);
        let mv_only_key = CString::new("motion_vectors_only").unwrap();
        ff::av_opt_set_int(dec_ctx as *mut c_void, mv_only_key.as_ptr(), 1, 0);
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

        let mut writer = if args.do_print {
            match open_mv_any(&args.output_file) {
                Ok(w) => Some(w),
                Err(e) => {
                    eprintln!("Failed to open output file: {}", e);
                    std::process::exit(255);
                }
            }
        } else {
            None
        };

        if args.is_verbose {
            print_ffmpeg_version();
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
                    if let Some(w) = writer.as_mut() {
                        let wrote = write_frame_mvs(w, frame_num, frame);
                        if !wrote && args.is_verbose {
                            eprintln!("Frame {}: no motion vectors", frame_num);
                        }
                    }
                    ff::av_frame_unref(frame);
                    frame_num += 1;
                }
            }
            ff::av_packet_unref(pkt);
        }

        let total_mvs = writer.as_ref().map(|w| w.total()).unwrap_or(0);
        if let Some(mut w) = writer {
            let _ = w.flush();
        }

        let rss_kb = get_current_rss_kb();

        let mut dec_ctx_ptr = dec_ctx;
        ff::avcodec_free_context(&mut dec_ctx_ptr);
        ff::avformat_close_input(&mut fmt_ctx);
        let mut frame_ptr = frame;
        ff::av_frame_free(&mut frame_ptr);
        let mut pkt_ptr = pkt;
        ff::av_packet_free(&mut pkt_ptr);

        println!("{} {} {}", frame_num, total_mvs, rss_kb);
    }
}
