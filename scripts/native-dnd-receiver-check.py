#!/usr/bin/env python3
"""Private C++ receiver regression using existing debug FLTK; no Wayland I/O."""
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parent.parent
headers = sorted((root / 'target/debug/build').glob('files-ui-*/out/files_fltk_dnd_build.h'),
                 key=lambda p: p.stat().st_mtime_ns, reverse=True)
logs = sorted((root / 'target/debug/build').glob('fltk-sys-*/output'),
              key=lambda p: p.stat().st_mtime_ns, reverse=True)
sources = list((Path.home() / '.cargo/registry/src').glob('*/fltk-sys-1.5.23/cfltk/fltk'))
if not headers or len(sources) != 1:
    raise SystemExit('Build files-ui debug and provide exactly one pinned fltk-sys source')
header = headers[0]
symbol = header.read_text().split()[-1]
for log in logs:
    archive = log.parent / 'out/lib/libfltk.a'
    if archive.exists():
        nm = subprocess.run(['nm', '--defined-only', str(archive)], capture_output=True, text=True, check=True)
        if ' T ' + symbol in nm.stdout:
            break
else:
    raise SystemExit('No debug libfltk contains the UI input-guard symbol')
flags = []
for line in log.read_text().splitlines():
    if line.startswith('cargo:rustc-link-search=native='):
        flags.append('-L' + line.split('=', 2)[2])
    if line.startswith('cargo:rustc-link-lib='):
        flags.append('-l' + line.split('=')[-1])
source = sources[0]
binary = root / 'target/native-dnd-receiver-check'
command = ['c++', '-std=c++17', '-I' + str(header.parent), '-I' + str(source),
           '-I' + str(source / 'src/drivers/Wayland'), '-I' + str(log.parent / 'out/build/fltk'),
           '-I' + str(log.parent / 'out/build/fltk/src'),
           str(root / 'crates/ui/native/tests/file_transfer_dnd.cxx'),
           '-Wl,--wrap=wl_proxy_get_version', '-Wl,--wrap=wl_proxy_marshal_flags',
           '-Wl,--wrap=wl_proxy_add_listener',
           *flags, '-o', str(binary)]
subprocess.run(command, check=True)
subprocess.run([str(binary)], check=True, timeout=5)
