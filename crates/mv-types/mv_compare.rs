use crate::motion_vector::{MotionVector, MvCsvReader};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub frame: i32,
    pub src_x: i64,
    pub src_y: i64,
    pub dst_x: i64,
    pub dst_y: i64,
    pub source: i32,
}

impl Key {
    pub fn from_mv(mv: &MotionVector) -> Self {
        Key {
            frame: mv.frame,
            src_x: mv.src_x as i64,
            src_y: mv.src_y as i64,
            dst_x: mv.dst_x as i64,
            dst_y: mv.dst_y as i64,
            source: mv.source,
        }
    }

    pub fn display(&self) -> String {
        format!(
            "Frame {} src=({},{}) dst=({},{}) source={}",
            self.frame, self.src_x, self.src_y, self.dst_x, self.dst_y, self.source
        )
    }
}

struct ValueColumns {
    src_x: f64,
    src_y: f64,
    dst_x: f64,
    dst_y: f64,
}

impl ValueColumns {
    fn from_mv(mv: &MotionVector) -> Self {
        ValueColumns {
            src_x: mv.src_x,
            src_y: mv.src_y,
            dst_x: mv.dst_x,
            dst_y: mv.dst_y,
        }
    }

    fn fields(&self) -> Vec<(&str, f64)> {
        vec![
            ("src_x", self.src_x),
            ("src_y", self.src_y),
            ("dst_x", self.dst_x),
            ("dst_y", self.dst_y),
        ]
    }
}

pub fn compare_frames(first: &[MotionVector], second: &[MotionVector]) -> Vec<(Key, String)> {
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

/// Pulls the next group of motion vectors that share the same frame number.
/// Relies on extractor CSVs being written in monotonically non-decreasing
/// frame order — extractors emit MVs frame-by-frame in decode order, so a
/// frame's rows are always contiguous.
fn next_frame_batch<I>(
    iter: &mut I,
    peeked: &mut Option<MotionVector>,
) -> std::io::Result<Option<(i32, Vec<MotionVector>)>>
where
    I: Iterator<Item = std::io::Result<MotionVector>>,
{
    let first = match peeked.take() {
        Some(mv) => mv,
        None => match iter.next() {
            Some(Ok(mv)) => mv,
            Some(Err(e)) => return Err(e),
            None => return Ok(None),
        },
    };
    let frame = first.frame;
    let mut batch = vec![first];
    loop {
        match iter.next() {
            Some(Ok(mv)) if mv.frame == frame => batch.push(mv),
            Some(Ok(mv)) => {
                *peeked = Some(mv);
                break;
            }
            Some(Err(e)) => return Err(e),
            None => break,
        }
    }
    Ok(Some((frame, batch)))
}

fn emit_only<W: Write>(
    batch: &[MotionVector],
    other_side: &str,
    out: &mut W,
    diff_count: &mut usize,
) -> std::io::Result<()> {
    for mv in batch {
        let key = Key::from_mv(mv);
        writeln!(out, "{}: missing in {} file", key.display(), other_side)?;
        *diff_count += 1;
    }
    Ok(())
}

pub fn compare(
    first_csv: &str,
    second_csv: &str,
    output_file: &str,
) -> Result<(), String> {
    let f1 = File::open(first_csv).map_err(|e| format!("Error opening first CSV: {}", e))?;
    let f2 = File::open(second_csv).map_err(|e| format!("Error opening second CSV: {}", e))?;
    let mut r1 = MvCsvReader::new(BufReader::new(f1))
        .map_err(|e| format!("Error reading first CSV header: {}", e))?;
    let mut r2 = MvCsvReader::new(BufReader::new(f2))
        .map_err(|e| format!("Error reading second CSV header: {}", e))?;

    let out_file = File::create(output_file)
        .map_err(|e| format!("Failed to create output: {}", e))?;
    let mut out = BufWriter::new(out_file);

    let mut peek1: Option<MotionVector> = None;
    let mut peek2: Option<MotionVector> = None;
    let mut next1 = next_frame_batch(&mut r1, &mut peek1)
        .map_err(|e| format!("Error reading first CSV: {}", e))?;
    let mut next2 = next_frame_batch(&mut r2, &mut peek2)
        .map_err(|e| format!("Error reading second CSV: {}", e))?;

    let mut diff_count: usize = 0;

    loop {
        use std::cmp::Ordering;
        let order = match (&next1, &next2) {
            (None, None) => break,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some((f1n, _)), Some((f2n, _))) => f1n.cmp(f2n),
        };

        match order {
            Ordering::Less => {
                let (_, batch) = next1.take().unwrap();
                emit_only(&batch, "second", &mut out, &mut diff_count)
                    .map_err(|e| format!("Failed to write output: {}", e))?;
                next1 = next_frame_batch(&mut r1, &mut peek1)
                    .map_err(|e| format!("Error reading first CSV: {}", e))?;
            }
            Ordering::Greater => {
                let (_, batch) = next2.take().unwrap();
                emit_only(&batch, "first", &mut out, &mut diff_count)
                    .map_err(|e| format!("Failed to write output: {}", e))?;
                next2 = next_frame_batch(&mut r2, &mut peek2)
                    .map_err(|e| format!("Error reading second CSV: {}", e))?;
            }
            Ordering::Equal => {
                let (_, b1) = next1.take().unwrap();
                let (_, b2) = next2.take().unwrap();
                let diffs = compare_frames(&b1, &b2);
                for (_, msg) in &diffs {
                    writeln!(out, "{}", msg)
                        .map_err(|e| format!("Failed to write output: {}", e))?;
                }
                diff_count += diffs.len();
                next1 = next_frame_batch(&mut r1, &mut peek1)
                    .map_err(|e| format!("Error reading first CSV: {}", e))?;
                next2 = next_frame_batch(&mut r2, &mut peek2)
                    .map_err(|e| format!("Error reading second CSV: {}", e))?;
            }
        }
    }

    if diff_count == 0 {
        writeln!(out, "No differences found in frames in all frames.")
            .map_err(|e| format!("Failed to write output: {}", e))?;
    }
    out.flush().map_err(|e| format!("Failed to flush output: {}", e))?;

    println!("Comparison complete. Results written to {}", output_file);
    Ok(())
}