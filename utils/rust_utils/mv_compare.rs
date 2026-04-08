use shared::{load_motion_vectors, MotionVector};
use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::Path;
use std::process;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    frame: i32,
    src_x: i64,
    src_y: i64,
    dst_x: i64,
    dst_y: i64,
    source: i32,
}

impl Key {
    fn from_mv(mv: &MotionVector) -> Self {
        Key {
            frame: mv.frame,
            src_x: mv.src_x as i64,
            src_y: mv.src_y as i64,
            dst_x: mv.dst_x as i64,
            dst_y: mv.dst_y as i64,
            source: mv.source,
        }
    }

    fn display(&self) -> String {
        format!(
            "Frame {} src=({},{}) dst=({},{}) source={}",
            self.frame, self.src_x, self.src_y, self.dst_x, self.dst_y, self.source
        )
    }
}

struct ValueColumns {
    w: i32,
    h: i32,
    motion_x: f64,
    motion_y: f64,
    motion_scale: f64,
}

impl ValueColumns {
    fn from_mv(mv: &MotionVector) -> Self {
        ValueColumns {
            w: mv.w,
            h: mv.h,
            motion_x: mv.motion_x,
            motion_y: mv.motion_y,
            motion_scale: mv.motion_scale,
        }
    }

    fn fields(&self) -> Vec<(&str, f64)> {
        vec![
            ("w", self.w as f64),
            ("h", self.h as f64),
            ("motion_x", self.motion_x),
            ("motion_y", self.motion_y),
            ("motion_scale", self.motion_scale),
        ]
    }
}

fn compare_frames(first: &[MotionVector], second: &[MotionVector]) -> Vec<(Key, String)> {
    let mut first_map: BTreeMap<Key, Vec<&MotionVector>> = BTreeMap::new();
    for mv in first {
        first_map.entry(Key::from_mv(mv)).or_default().push(mv);
    }

    let mut second_map: BTreeMap<Key, Vec<&MotionVector>> = BTreeMap::new();
    for mv in second {
        second_map.entry(Key::from_mv(mv)).or_default().push(mv);
    }

    let mut diffs: Vec<(Key, String)> = Vec::new();

    let all_keys: BTreeMap<&Key, ()> = first_map
        .keys()
        .chain(second_map.keys())
        .map(|k| (k, ()))
        .collect();

    for key in all_keys.keys() {
        let first_rows = first_map.get(key);
        let second_rows = second_map.get(key);

        match (first_rows, second_rows) {
            (Some(_), None) => {
                diffs.push((
                    (*key).clone(),
                    format!("{}: missing in second file", key.display()),
                ));
            }
            (None, Some(_)) => {
                diffs.push((
                    (*key).clone(),
                    format!("{}: missing in first file", key.display()),
                ));
            }
            (Some(f_rows), Some(s_rows)) => {
                let count = f_rows.len().max(s_rows.len());
                for i in 0..count {
                    if i >= f_rows.len() {
                        diffs.push((
                            (*key).clone(),
                            format!("{}: extra row in second file (index {})", key.display(), i),
                        ));
                        continue;
                    }
                    if i >= s_rows.len() {
                        diffs.push((
                            (*key).clone(),
                            format!("{}: extra row in first file (index {})", key.display(), i),
                        ));
                        continue;
                    }
                    let f_vals = ValueColumns::from_mv(f_rows[i]);
                    let s_vals = ValueColumns::from_mv(s_rows[i]);
                    for ((col, v1), (_, v2)) in f_vals.fields().iter().zip(s_vals.fields().iter()) {
                        if v1 != v2 {
                            diffs.push((
                                (*key).clone(),
                                format!(
                                    "{}: '{}' differs (first={}, second={})",
                                    key.display(),
                                    col,
                                    v1,
                                    v2
                                ),
                            ));
                        }
                    }
                }
            }
            (None, None) => unreachable!(),
        }
    }

    diffs.sort_by(|a, b| a.0.cmp(&b.0));
    diffs
}

fn write_results(differences: &[(Key, String)], output_path: &Path) -> Result<(), String> {
    let mut content = String::new();
    if differences.is_empty() {
        writeln!(content, "No differences found in frames in all frames.").unwrap();
    } else {
        for (_, msg) in differences {
            writeln!(content, "{}", msg).unwrap();
        }
    }
    fs::write(output_path, content).map_err(|e| format!("Failed to write output: {}", e))
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        eprintln!("Usage: mv_compare <first_csv> <second_csv> <output_file>");
        process::exit(1);
    }

    let first = match load_motion_vectors(&args[1]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let second = match load_motion_vectors(&args[2]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let differences = compare_frames(&first, &second);
    let output_path = Path::new(&args[3]);

    if let Err(e) = write_results(&differences, output_path) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }

    println!(
        "Comparison complete. Results written to {}",
        output_path.display()
    );
}