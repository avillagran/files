#!/usr/bin/env python3
"""Bounded helper protocol checks; no UI or resource-budget claim."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary=Path(sys.argv[1] if len(sys.argv)>1 else 'target/debug/files-helper').resolve()
with tempfile.TemporaryDirectory(prefix='files-text-preview-') as tmp:
    root=Path(tmp)
    def request(path, command='preview_text'):
        result=subprocess.run([str(binary), command],input=json.dumps({'command':'preview_text','id':9,'path':str(path)}).encode()+b'\n',capture_output=True,timeout=3)
        return result, json.loads(result.stdout) if result.stdout else None
    text=root/'note.txt'; text.write_text('Hola 🐜\n')
    result,data=request(text)
    assert result.returncode==0 and data=={'status':'text','id':9,'text':'Hola 🐜\n','encoding':'UTF-8','truncated':False}
    wide=root/'wide.txt'; wide.write_bytes('Hola 🐜'.encode('utf-16'))
    assert request(wide)[1]['text']=='Hola 🐜'
    huge=root/'huge.txt'
    with huge.open('wb') as output:
        output.write(b'a'*(256*1024+1))
        output.truncate(4*1024*1024*1024)
    data=request(huge)[1]
    assert data['truncated'] and len(data['text'])==256*1024
    many=root/'many.txt';many.write_text('line\n'*3000)
    data=request(many)[1]
    assert data['truncated'] and data['text']=='line\n'*2000
    binary_file=root/'binary';binary_file.write_bytes(b'abc\x00def')
    assert request(binary_file)[1]['status']=='error'
    fifo=root/'fifo';os.mkfifo(fifo)
    assert request(fifo)[1]['status']=='error'
    alias=root/'link';alias.symlink_to(text)
    assert request(alias)[1]['status']=='error'
    result,_=request(text,'du')
    assert result.returncode==2
print(json.dumps({'protocol':'PASS','cases':8,'ui_integrated':False,'resource_benchmark':False}))
