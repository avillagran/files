fn main() {
    let start = std::time::Instant::now();
    if !files_ui::run_since(start) {
        std::process::exit(1);
    }
}
