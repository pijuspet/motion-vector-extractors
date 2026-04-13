use plotters::style::{register_font, FontStyle};
use std::fs;

const FONT_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

pub fn init_fonts() -> Result<(), String> {
    let font_data =
        fs::read(FONT_PATH).map_err(|e| format!("Cannot read font {}: {}", FONT_PATH, e))?;
    let font_data: &'static [u8] = Box::leak(font_data.into_boxed_slice());
    register_font("sans-serif", FontStyle::Normal, font_data)
        .map_err(|_| "Failed to register normal font".to_string())?;
    register_font("sans-serif", FontStyle::Bold, font_data)
        .map_err(|_| "Failed to register bold font".to_string())?;
    Ok(())
}
