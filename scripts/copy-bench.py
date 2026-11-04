#!/usr/bin/env python3
"""Serialized warm-cache copy comparison; own fixtures, no disk-durability claim."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

parser=argparse.ArgumentParser()
parser.add_argument('--mib', type=int, choices=[16,64,256], default=64)
parser.add_argument('--small-files', type=int, choices=[1000,10000], default=1000)
parser.add_argument('--repetitions', type=int, choices=[1,3,5], default=3)
args=parser.parse_args()
root=Path(__file__).resolve().parent.parent
binary=root/'target/release/examples/copy_bench'
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
rows=[]
report={'method':'serialized warm-cache fixtures; external process time includes startup; hashes checked after timing; no fsync/durability or physical-throughput claim','scope':'isolated core engine versus cp, not integrated UI PSS','bulk_mib':args.mib,'small_files':args.small_files,'repetitions':args.repetitions,'rows':rows,'reflink_note':'logical clones do not transfer all logical bytes; cp-no-reflink may still use kernel copy_file_range','resource_gate':'NOT_MEASURED'}
report['functional_gate']='FAIL'
try:
    if not binary.is_file():
        raise RuntimeError('Primero: cargo build --release -p files-core --example copy_bench')
    with tempfile.TemporaryDirectory(prefix='copy-bench-',dir=base) as tmp:
        fixture=Path(tmp)
        bulk=fixture/'bulk.bin'
        block=bytes(range(256))*4096
        with bulk.open('wb') as stream:
            for _ in range(args.mib): stream.write(block)
        small=fixture/'small';small.mkdir()
        for i in range(args.small_files):
            path=small/f'file-{i:05}.txt';path.write_bytes((f'entry {i}\n'.encode())*100)
        stamp=1_700_000_000_123456789
        for source in [bulk,*small.iterdir(),small]:
            source.chmod(0o750 if source.is_dir() else 0o640)
            os.utime(source, ns=(stamp,stamp))
        def fingerprint(path):
            entries=sorted(path.rglob('*')) if path.is_dir() else [path]
            out={}
            for entry in entries:
                st=entry.stat()
                if entry.is_file():
                    with entry.open('rb') as stream:
                        digest=hashlib.file_digest(stream,'sha256').hexdigest()
                    out[str(entry.relative_to(path) if path.is_dir() else '.')]=(st.st_size,st.st_mode & 0o777,st.st_mtime_ns,digest)
                else:
                    out[str(entry.relative_to(path))]=('directory',st.st_mode & 0o777,st.st_mtime_ns)
            return out
        scenarios=[('bulk',bulk),('small-files',small)]
        for scenario,source in scenarios:
            expected=fingerprint(source)
            for repetition in range(args.repetitions):
                methods=['engine-auto','cp-auto','engine-stream','cp-no-reflink']
                # Rotate ordering; input remains warm-cache, not a cold-storage claim.
                methods=methods[repetition%4:]+methods[:repetition%4]
                for method in methods:
                    destination=fixture/f'destination-{scenario}-{repetition}-{method}'
                    if method.startswith('engine'):
                        command=[str(binary),method.removeprefix('engine-'),str(source),str(destination)]
                    else:
                        command=['cp','--recursive','--preserve=mode,timestamps','--no-clobber', '--reflink=auto' if method=='cp-auto' else '--reflink=never']
                        if method=='cp-no-reflink':command+=['--sparse=never']
                        command+=['--',str(source),str(destination)]
                    row={'scenario':scenario,'method':method,'repetition':repetition,
                         'verified_bytes_mode_mtime':False,'status':'RUNNING'}
                    rows.append(row)
                    start=time.perf_counter()
                    result=subprocess.run(command,capture_output=True,text=True,timeout=120)
                    elapsed=(time.perf_counter()-start)*1000
                    row.update(external_elapsed_ms=elapsed, returncode=result.returncode, stderr=result.stderr[:8192])
                    assert result.returncode==0,(method,result.stderr[:8192])
                    assert fingerprint(destination)==expected,method
                    assert fingerprint(source)==expected,'source changed'
                    if source.is_dir():
                        assert (destination.stat().st_mode & 0o777,destination.stat().st_mtime_ns)==(source.stat().st_mode & 0o777,source.stat().st_mtime_ns)
                    row.update(verified_bytes_mode_mtime=True, status='PASS', engine=json.loads(result.stdout) if method.startswith('engine') else None)
                    if destination.is_dir():shutil.rmtree(destination)
                    else:destination.unlink()
    report['functional_gate']='PASS'
except Exception as exc:
    report['error']={'type':type(exc).__name__,'message':str(exc)[:8192]}
    if rows and rows[-1]['status']=='RUNNING':
        rows[-1].update(status='FAIL', external_elapsed_ms=(time.perf_counter()-start)*1000)
        if isinstance(exc, subprocess.TimeoutExpired):
            rows[-1]['timed_out']=True
finally:
    output=root/'docs/evidence/copy';output.mkdir(parents=True,exist_ok=True)
    path=output/'throughput.json'
    if path.exists():
        path=output/f'throughput-{time.time_ns()}.json'
    with path.open('x') as stream:
        json.dump(report,stream,indent=2)
        stream.write('\n')
    print(json.dumps({'report':str(path),**report},indent=2))
if report['functional_gate']!='PASS':
    raise SystemExit(1)
