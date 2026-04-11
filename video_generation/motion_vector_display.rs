use opencv::core::{Mat, Point, Scalar};
use opencv::imgproc;

use utils::motion_vector::MotionVector;

pub fn draw_motion_vectors(
    img: &mut Mat,
    frame_data: &[MotionVector],
) -> Result<(), Box<dyn std::error::Error>> {
    for v in frame_data {
        let mag = (v.motion_x * v.motion_x + v.motion_y * v.motion_y).sqrt();
        if mag < 2.0 {
            continue;
        }

        let color = if mag > 20.0 {
            Scalar::new(0.0, 0.0, 255.0, 0.0) // Red (BGR)
        } else if mag > 10.0 {
            Scalar::new(0.0, 255.0, 255.0, 0.0) // Yellow (BGR)
        } else {
            Scalar::new(255.0, 255.0, 255.0, 0.0) // White (BGR)
        };

        let src = Point::new(v.src_x as i32, v.src_y as i32);
        let dst = Point::new(v.dst_x as i32, v.dst_y as i32);

        imgproc::arrowed_line(img, src, dst, color, 1, imgproc::LINE_8, 0, 0.3)?;
        imgproc::circle(
            img,
            src,
            1,
            Scalar::new(255.0, 255.0, 255.0, 0.0),
            -1,
            imgproc::LINE_8,
            0,
        )?;
    }

    Ok(())
}
