//! Isolated copy benchmark driver, not the app or a resource-budget gate.
use files_core::copy_engine::{Control, CopyOptions, CopyRequest, CopyStrategy, copy_paths};
use std::{path::PathBuf, time::Instant};
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 || !matches!(args[0].to_str(), Some("auto" | "stream")) {
        eprintln!("Uso: copy_bench auto|stream origen destino");
        std::process::exit(2);
    }
    let options = CopyOptions {
        strategy: if args[0] == "stream" {
            CopyStrategy::Stream
        } else {
            CopyStrategy::Auto
        },
        ..CopyOptions::default()
    };
    let started = Instant::now();
    match copy_paths(
        &[CopyRequest {
            source: PathBuf::from(&args[1]),
            destination: PathBuf::from(&args[2]),
        }],
        &options,
        &Control::default(),
        |_| {},
    ) {
        Ok(report) => {
            println!(
                "{{\"elapsed_ms\":{},\"logical_bytes\":{},\"files\":{},\"directories\":{},\"reflink_files\":{},\"kernel_files\":{},\"stream_files\":{},\"auxiliary_accounted_peak_bytes\":{},\"cleanup_paths\":{},\"cleanup_errors\":{}}}",
                started.elapsed().as_secs_f64() * 1000.0,
                report.bytes_copied,
                report.files_copied,
                report.directories_copied,
                report.reflink_files,
                report.kernel_files,
                report.stream_files,
                report.auxiliary_accounted_peak_bytes,
                report.cleanup_paths.len(),
                report.cleanup_errors.len()
            );
            if !report.cleanup_paths.is_empty() || !report.cleanup_errors.is_empty() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error:?}");
            std::process::exit(1);
        }
    }
}
