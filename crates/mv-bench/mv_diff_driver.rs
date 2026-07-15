use mv_bench::mv_diff::compare_list0;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let method0_csv = &args[1];
    let method9_csv = &args[2];
    let pkt_order_path = &args[3];
    let output_file = &args[4];

    let summary = compare_list0(method0_csv, method9_csv, pkt_order_path, Path::new(output_file))
        .unwrap_or_else(|e| {
            eprintln!("mv_diff_driver: {e}");
            std::process::exit(1);
        });

    println!("method0 (source == -1): {} rows", summary.method0_rows);
    println!(
        "method9 (source == -1, remapped to display order): {} rows",
        summary.method9_rows
    );
    println!("differences: {}", summary.diffs);
    println!("Full diff list written to {}", output_file);
}
