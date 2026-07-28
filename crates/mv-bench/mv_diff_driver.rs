use mv_bench::mv_diff::compare_list0;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!(
            "usage: mv_diff_driver <first.csv> <second.csv> <pkt_order.txt|-> <out.txt>\n\
             \n\
             pkt_order.txt  second.csv is in decode order (method9, from-scratch parser);\n\
             \x20              remap its frame numbers to display order by PTS rank.\n\
             -              second.csv is already in display order (ffmpeg-based:\n\
             \x20              method3/4/5); compare as-is. Passing a pkt-order file here\n\
             \x20              scrambles the join and reports nearly every row as a diff."
        );
        std::process::exit(2);
    }
    let first_csv = &args[1];
    let second_csv = &args[2];
    let pkt_order_path = &args[3];
    let output_file = &args[4];

    let pkt_order = if pkt_order_path == "-" { None } else { Some(pkt_order_path.as_str()) };

    let summary = compare_list0(first_csv, second_csv, pkt_order, Path::new(output_file))
        .unwrap_or_else(|e| {
            eprintln!("mv_diff_driver: {e}");
            std::process::exit(1);
        });

    println!("first  (source == -1): {} rows", summary.method0_rows);
    println!(
        "second (source == -1{}): {} rows",
        if pkt_order.is_some() { ", remapped to display order" } else { "" },
        summary.method9_rows
    );
    println!("differences: {}", summary.diffs);
    println!("Full diff list written to {}", output_file);
}
