#!/usr/bin/env python3
"""Functional native adapter FD contract, no resource/startup/throughput sampling."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT=Path(__file__).resolve().parent.parent
binary=ROOT/'target/release/files-html-helper'
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','scope':'native adapter inherited-FD reading; parent expected revision/publication covered separately by Rust tests and UI scene','checks':[],'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}
def invoke(args,fds=()):
    return subprocess.run([str(binary),*args],pass_fds=fds,capture_output=True,timeout=5)
def text(result):
    assert result.returncode==0,result.stderr.decode(errors='replace')
    return ' '.join(c.get('text','') for c in json.loads(result.stdout)['commands'])
try:
    with tempfile.TemporaryDirectory(prefix='html-fd-',dir=base) as tmp:
        root=Path(tmp);path=root/'source.html';path.write_text('<p>FD_ORIGINAL_A</p>')
        with path.open('rb') as source:
            replacement=root/'replacement';replacement.write_text('<p>PATH_REPLACEMENT_B</p>');os.replace(replacement,path)
            # Descriptor reads use pread: shared seek offset is preserved too.
            source.seek(2)
            body=text(invoke(['--source-fd',str(source.fileno()),'640','480','FiraCode Nerd Font'],(source.fileno(),)))
            assert 'FD_ORIGINAL_A' in body and 'PATH_REPLACEMENT_B' not in body
            assert source.tell()==2
        report['checks'].append('FD inherits original inode after name replacement and leaves shared offset unchanged')
        assert 'PATH_REPLACEMENT_B' in text(invoke([str(path),'640','480','FiraCode Nerd Font']))
        report['checks'].append('legacy path mode remains compatible and reads current named inode')
        for fd in ['0','1','2','256','-1','3suffix','+3','9999']:
            result=invoke(['--source-fd',fd,'640','480','FiraCode Nerd Font'])
            assert result.returncode!=0 and not result.stdout,fd
        report['checks'].append('invalid inherited FD mode rejects without partial output')
        fifo=root/'fifo';os.mkfifo(fifo);fd=os.open(fifo,os.O_RDONLY|os.O_NONBLOCK)
        try:result=invoke(['--source-fd',str(fd),'640','480','FiraCode Nerd Font'],(fd,))
        finally:os.close(fd)
        assert result.returncode!=0 and not result.stdout
        path.write_bytes(b' '*32769)
        with path.open('rb') as source:result=invoke(['--source-fd',str(source.fileno()),'640','480','FiraCode Nerd Font'],(source.fileno(),))
        assert result.returncode!=0 and not result.stdout
        report['checks'].append('FIFO/nonregular and source above32KiB rejected before layout, no partial output')
        report['functional_gate']='PASS'
except BaseException as error:
    report['error']=repr(error)
    raise
finally:
    output=ROOT/'docs/evidence/html-engine'/('fd-contract-'+str(time.time_ns())+'.json')
    with output.open('x') as handle:json.dump(report,handle,indent=2)
    print(output)
