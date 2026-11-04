#!/usr/bin/env python3
"""Build the two-FD FLTK wait probe from existing debug libraries (no Cargo).
Run only outside resource measurements. --run executes the compiled fixture.
"""
import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    args = parser.parse_args()
    logs = [p for p in (ROOT/'target/debug/build').glob('fltk-sys-*/output')
            if (p.parent/'out/include/FL/Fl.H').exists()]
    if not logs:
        parser.error('Build files-ui in debug mode first; no existing FLTK libraries found')
    log = max(logs, key=lambda p: p.stat().st_mtime_ns)
    flags = []
    for line in log.read_text().splitlines():
        if line.startswith('cargo:rustc-link-search=native='):
            flags.append('-L'+line.split('=', 2)[2])
        if line.startswith('cargo:rustc-link-lib='):
            flags.append('-l'+line.split('=')[-1])
    binary = ROOT/'target/wait-contract'
    subprocess.run(['c++', '-std=c++17', '-I'+str(log.parent/'out/include'),
                    str(ROOT/'scripts/wait-contract.cpp'), *flags, '-o', str(binary)], check=True)
    print('Built '+str(binary), flush=True)
    if args.run:
        subprocess.run([str(binary)], check=True, timeout=5)

if __name__ == '__main__':
    main()
