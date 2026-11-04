#!/usr/bin/env python3
"""Launch own Cargo only after an idle Cargo snapshot; never controls competitors."""
import os,subprocess,sys
from pathlib import Path
scan=subprocess.run(['pgrep','-x','cargo'],text=True,capture_output=True)
if scan.returncode not in (0,1):raise SystemExit('Cargo inventory failed')
live=[]
for pid in scan.stdout.split():
    try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
    except FileNotFoundError:continue
    try:cwd=str(Path('/proc',pid,'cwd').resolve(strict=True))
    except OSError:cwd='<unavailable>'
    live.append((pid,parts[19],cwd))
if live:
    print('NOT_STARTED; Cargo observed:',live);raise SystemExit(75)
log=Path(sys.argv[1])
with log.open('w') as out:
    child=subprocess.Popen(sys.argv[2:],env={**os.environ,'CARGO_BUILD_JOBS':'1','CARGO_INCREMENTAL':'0'},stdout=out,stderr=subprocess.STDOUT)
    print('Own Cargo admitted after idle snapshot; PID',child.pid,flush=True)
    raise SystemExit(child.wait())
