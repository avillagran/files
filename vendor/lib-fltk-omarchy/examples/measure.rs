//! Launches a GUI command as a real Wayland client on the hidden Hyprland
//! workspace special:omarchy-fltk-bench (floating at map, never focused) and
//! measures it the same way for every toolkit:
//!   startup_ms  dispatch -> window mapped (Hyprland client list, 5 ms polling)
//!   pss_kb      Pss + SwapPss of the whole process tree after 2 s
//!   idle_cpu_s  CPU seconds (utime + stime, tree) over 5 s idle
//! Usage: cargo run --release --example measure -- <label> <runs> <command...>
//! (the command receives a unique window title as its last argument)
use fltk_omarchy::hyprland;
use std::time::{Duration, Instant};

fn children(pid: i64) -> Vec<i64> {
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(p) = e.file_name().to_string_lossy().parse::<i64>() else { continue };
        if let Ok(stat) = std::fs::read_to_string(format!("/proc/{p}/stat")) {
            let f: Vec<&str> = stat.rsplit(") ").next().unwrap_or("").split_whitespace().collect();
            if f.get(1).and_then(|x| x.parse::<i64>().ok()) == Some(pid) { out.push(p); }
        }
    }
    out
}
fn tree(pid: i64) -> Vec<i64> {
    let mut v = vec![pid];
    for c in children(pid) { v.extend(tree(c)); }
    v
}
fn pss_kb(pids: &[i64]) -> u64 {
    pids.iter().map(|p| std::fs::read_to_string(format!("/proc/{p}/smaps_rollup")).unwrap_or_default().lines()
        .filter(|l| l.starts_with("Pss:") || l.starts_with("SwapPss:"))
        .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok()).sum::<u64>()).sum()
}
fn cpu_ticks(pids: &[i64]) -> u64 {
    pids.iter().map(|p| {
        let s = std::fs::read_to_string(format!("/proc/{p}/stat")).unwrap_or_default();
        let f: Vec<&str> = s.rsplit(") ").next().unwrap_or("").split_whitespace().collect();
        f.get(11).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0) + f.get(12).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0)
    }).sum()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (label, runs, cmd) = (&args[0], args[1].parse::<usize>().unwrap(), &args[2..]);
    let mut rows = Vec::new();
    for i in 0..runs {
        let title = format!("omarchy-fltk-bench-{label}-{}-{i}", std::process::id());
        let argv = cmd.iter().chain(std::iter::once(&title)).map(|a| a.replace('"', "\\\"")).collect::<Vec<_>>().join(" ");
        let t0 = Instant::now();
        hyprland::dispatch(&format!(r#"hl.dsp.exec_cmd("{argv}", {{ workspace = "special:omarchy-fltk-bench silent", float = true }})"#), None);
        let client = loop {
            let found = hyprland::json("clients").and_then(|v| v.as_array().cloned()).unwrap_or_default().into_iter()
                .find(|c| c["title"].as_str() == Some(&title) && c["mapped"].as_bool() == Some(true));
            if let Some(c) = found { break c; }
            assert!(t0.elapsed() < Duration::from_secs(20), "timeout starting {label}");
            std::thread::sleep(Duration::from_millis(5));
        };
        let startup = t0.elapsed().as_millis();
        let pid = client["pid"].as_i64().unwrap();
        std::thread::sleep(Duration::from_secs(2));
        let pids = tree(pid);
        let pss = pss_kb(&pids);
        let c0 = cpu_ticks(&pids);
        std::thread::sleep(Duration::from_secs(5));
        let idle = (cpu_ticks(&tree(pid)) - c0) as f64 / 100.0;
        unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        for _ in 0..50 { if !std::path::Path::new(&format!("/proc/{pid}")).exists() { break; } std::thread::sleep(Duration::from_millis(50)); }
        eprintln!("{label} run {i}: startup {startup} ms, pss {pss} kB, idle cpu {idle:.2} s, size {}", client["size"]);
        rows.push((startup, pss, idle));
    }
    let med = |mut v: Vec<f64>| { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[v.len() / 2] };
    println!("{{\"label\":\"{label}\",\"runs\":{runs},\"median_startup_ms\":{},\"median_pss_kb\":{},\"max_pss_kb\":{},\"median_idle_cpu_s\":{}}}",
        med(rows.iter().map(|r| r.0 as f64).collect()), med(rows.iter().map(|r| r.1 as f64).collect()),
        rows.iter().map(|r| r.1).max().unwrap(), med(rows.iter().map(|r| r.2).collect()));
}
