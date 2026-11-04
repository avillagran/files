#!/usr/bin/env python3
"""Benchmark aislado del planificador; no mide UI ni ejecución de renombrados."""
import json
from pathlib import Path
import select
import subprocess
import tempfile
import time
from process_metrics import TreeSampler

ROOT = Path(__file__).resolve().parent.parent


def main():
    base = Path.home() / '.cache/files-test'
    base.mkdir(parents=True, exist_ok=True)
    results = []
    with tempfile.TemporaryDirectory(prefix='rename-bench-', dir=base) as temp:
        folder = Path(temp)
        source = '''
use files_core::rename_plan;
use std::io::Write;
fn main() {
    let count: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    let names: Vec<String> = (0..count).map(|i| format!("archivo-{i:06}.jpg")).collect();
    let options = rename_plan::RenameOptions {
        pattern: "imagen-{n:06}-{name}{dotext}".into(),
        ..Default::default()
    };
    let start = std::time::Instant::now();
    let rows = rename_plan::plan(&names, &names, &options).unwrap();
    println!("{}", start.elapsed().as_micros());
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    std::hint::black_box(rows);
    std::hint::black_box(names);
}
'''
        (folder / 'src').mkdir()
        (folder / 'src/main.rs').write_text(source)
        # Use the actual crate/features and its regex dependency, rather than
        # importing a source file into rustc without Cargo's dependency graph.
        (folder / 'Cargo.toml').write_text(
            '[package]\nname = "files-rename-plan-fixture"\nversion = "0.0.4"\nedition = "2024"\n'
            '[workspace]\n[dependencies]\nfiles-core = { path = '+json.dumps(str(ROOT/'crates/core'))+' }\n'
            '[profile.release]\nlto = "thin"\ncodegen-units = 1\nstrip = true\n')
        target = ROOT / 'target'
        subprocess.run(['cargo', 'build', '--offline', '--release', '--manifest-path', str(folder/'Cargo.toml'),
                        '--target-dir', str(target)], cwd=ROOT, check=True, timeout=180)
        binary = target / 'release/files-rename-plan-fixture'

        for count in (10_000, 100_000):
            process = subprocess.Popen([str(binary), str(count)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
            try:
                sampler = TreeSampler(process.pid)
                samples = []
                deadline = time.monotonic() + 15
                while True:
                    samples.append(sampler.sample())
                    if select.select([process.stdout], [], [], .005)[0]:
                        elapsed_us = int(process.stdout.readline().strip())
                        samples.append(sampler.sample())
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError('El planificador no terminó en 15 segundos')
                if not all(sample['complete'] for sample in samples):
                    raise RuntimeError('Muestreo incompleto')
                results.append({'entries': count, 'plan_ms': elapsed_us / 1000,
                                'sampled_peak_pss_plus_swap_kib': max(s['memory_kib'] for s in samples),
                                'retained_pss_plus_swap_kib': samples[-1]['memory_kib'],
                                'samples': len(samples)})
                process.communicate('exit\n', timeout=3)
                if process.returncode != 0:
                    raise RuntimeError(f'Proceso terminó con código {process.returncode}')
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate(timeout=3)
    report = {'scope': 'Isolated Cargo release rename planner with actual core dependency/features, fixture names and retained plan; no UI or filesystem mutation',
              'limitations': 'Sampled peak is a lower bound; one run per size; no total application budget claim',
              'results': results}
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
