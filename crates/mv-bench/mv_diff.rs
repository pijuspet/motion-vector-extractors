use mv_types::motion_vector::load_motion_vectors;
use mv_types::mv_compare::{compare_frames, is_zero_size, write_results};
use std::path::Path;

/// Result of [`compare_list0`]: row counts plus how many differences were found.
pub struct DiffSummary {
    pub method0_rows: usize,
    pub method9_rows: usize,
    pub diffs: usize,
}

/// Compare method0 (original ffmpeg) against a second extractor's CSV, list-0
/// (`source == -1`, backward-reference) motion vectors only.
///
/// `pkt_order_path` controls frame-number alignment and **must** match how the
/// second file numbers its pictures:
///
/// * `Some(path)` — the second file is in bitstream/decode order (method9, the
///   custom from-scratch parser). method0 numbers pictures in display order
///   because libavcodec reorders internally, so the file at `path` (one packet
///   PTS per line, in decode order) is used to remap the second file's frame
///   numbers to display order by PTS rank before the join.
/// * `None` — the second file is already in display order because it is itself
///   ffmpeg-based (method3/4/5). Remapping one of these scrambles the join key
///   and reports essentially every row as a diff (~725k instead of ~35).
///
/// extractor0's harness never flushes the decoder's reorder buffer at EOF, so
/// its last ~num_reorder_frames pictures are simply absent from method0; the
/// second file's trailing pictures beyond method0's max frame are dropped to
/// avoid spurious "missing in first file" diffs from that gap.
pub fn compare_list0(
    method0_csv: &str,
    method9_csv: &str,
    pkt_order_path: Option<&str>,
    output_file: &Path,
) -> Result<DiffSummary, String> {
    let rank_of: Vec<i32> = match pkt_order_path {
        None => Vec::new(),
        Some(p) => {
            let pts: Vec<f64> = std::fs::read_to_string(p)
                .map_err(|e| format!("read pkt order {p}: {e}"))?
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| {
                    l.trim()
                        .parse::<f64>()
                        .map_err(|e| format!("parse pts {l:?}: {e}"))
                })
                .collect::<Result<_, _>>()?;
            let mut order: Vec<usize> = (0..pts.len()).collect();
            order.sort_by(|&a, &b| pts[a].partial_cmp(&pts[b]).unwrap());
            let mut rank_of = vec![0i32; pts.len()];
            for (rank, &dec_idx) in order.iter().enumerate() {
                rank_of[dec_idx] = rank as i32;
            }
            rank_of
        }
    };

    let method0 =
        load_motion_vectors(method0_csv).map_err(|e| format!("load method0 csv: {e}"))?;
    let mut method9 =
        load_motion_vectors(method9_csv).map_err(|e| format!("load method9 csv: {e}"))?;

    let method0_neg1: Vec<_> = method0.into_iter().filter(|m| m.source == -1).collect();

    for m in method9.iter_mut() {
        if let Some(&r) = rank_of.get(m.frame as usize) {
            m.frame = r;
        }
    }
    let max_display_rank = method0_neg1.iter().map(|m| m.frame).max().unwrap_or(0);
    let method9_neg1: Vec<_> = method9
        .into_iter()
        .filter(|m| m.source == -1 && m.frame <= max_display_rank)
        .collect();

    // Count what compare_frames actually compares: it drops zero-size vectors
    // from both sides, and method0 (unpatched ffmpeg) still emits them while
    // method9 does not. Counting raw rows here made the summary print a large
    // count mismatch next to "differences: 0".
    let method0_rows = method0_neg1.iter().filter(|m| !is_zero_size(m)).count();
    let method9_rows = method9_neg1.iter().filter(|m| !is_zero_size(m)).count();

    let diffs = compare_frames(&method0_neg1, &method9_neg1);
    let diff_count = diffs.len();
    write_results(&diffs, output_file).map_err(|e| format!("write results: {e}"))?;

    Ok(DiffSummary {
        method0_rows,
        method9_rows,
        diffs: diff_count,
    })
}
