#!/usr/bin/env python3

import sys

try:
    from mvextractor.videocap import VideoCap
except ImportError:
    print("[ERROR] mvextractor module not found. Did you install it?", file=sys.stderr)
    sys.exit(1)

if len(sys.argv) < 2:
    print("Usage: python3 mv_python_extractor5.py <input.mp4 or rtsp> <output.csv>", file=sys.stderr)
    sys.exit(1)

input_path = sys.argv[1]
output_file = sys.argv[2]

print(f"[INFO] Opening input: {input_path}", file=sys.stderr)

cap = VideoCap()
opened = cap.open(input_path)
if not opened:
    print(f"[ERROR] Failed to open input: {input_path}", file=sys.stderr)
    sys.exit(1)

frame = 0
f = open(output_file, "w")

f.write("frame,method_id,source,w,h,src_x,src_y,dst_x,dst_y,flags,motion_x,motion_y,motion_scale")

while True:
    try:
        result = cap.read()
        if not result or not isinstance(result, tuple):
            print(f"[ERROR] cap.read() returned non-tuple or empty at frame {frame}: {result}", file=sys.stderr)
            break

        if len(result) < 3:
            print(f"[ERROR] cap.read() tuple too short at frame {frame}: {result}", file=sys.stderr)
            break

        ret, img, mvs = result[:3]  # Unpack first 3 values safely

        if not ret:
            print(f"[INFO] Stream ended at frame {frame}", file=sys.stderr)
            break

       #print(f"[DEBUG] Frame {frame} - {len(mvs)} motion vectors", file=sys.stderr)

        for v in mvs:
            try:
                # Expecting: [src, w, h, src_x, src_y, dst_x, dst_y, mv_x, mv_y]
                if len(v) >= 9:
                    f.write(f"{frame},5,{v[0]},{v[1]},{v[2]},{v[3]},{v[4]},{v[5]},{v[6]},0x0,{v[7]},{v[8]},1")
                    v = v
                else:
                    print(f"[WARN] Skipping malformed MV at frame {frame}: {v}", file=sys.stderr)
            except Exception as mv_err:
                print(f"[ERROR] Exception printing MV at frame {frame}: {mv_err}", file=sys.stderr)

        frame += 1

    except Exception as e:
        print(f"[EXCEPTION] Fatal error at frame {frame}: {e}", file=sys.stderr)
        break

cap.release()
f.close()
print("[INFO] Extraction complete and video closed", file=sys.stderr)
