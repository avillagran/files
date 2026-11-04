#!/usr/bin/env python3
"""Functional descriptor-binding fixtures. No UI input or resource measurements."""
import hashlib,json,os,struct,subprocess,tempfile,time,zlib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
HELPER=ROOT/'target/release/files-helper'
EVIDENCE=ROOT/'docs/evidence/relative-source'
EVIDENCE.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report={'method':'real release helper protocol, own filesystem fixtures and deterministic replacement between probe/decode','functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','cases':[]}
def png(rgb):
    def chunk(k,d):return struct.pack('>I',len(d))+k+d+struct.pack('>I',zlib.crc32(k+d)&0xffffffff)
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1,1,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0'+bytes(rgb)))+chunk(b'IEND',b'')
def request(command,**fields):
    p=subprocess.run([str(HELPER),command],input=json.dumps({'command':command,'id':1,**fields}).encode()+b'\n',stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=8,check=True)
    header,_,payload=p.stdout.partition(b'\n')
    value=json.loads(header);assert value['id']==1
    return value,payload
def text(root):
    value,payload=request('preview_relative_text',root=str(root),relative='document.md')
    assert value['status']=='text' and not payload
    assert value['relative_source']['root']=={'device':root.stat().st_dev,'inode':root.stat().st_ino}
    return value['relative_source']
def probe(root,identity,relative='image.png'):
    value,payload=request('probe_relative_file',root=str(root),relative=relative,expected_root=identity)
    assert not payload
    return value
def checked(root,proof,relative='image.png'):
    return request('preview_relative_image_checked',root=str(root),relative=relative,expected=proof)
def rejected(value,payload):assert value['status']=='error' and not payload,value
try:
    with tempfile.TemporaryDirectory(prefix='relative-bound-',dir=BASE) as d:
        folder=Path(d);root=folder/'root';root.mkdir()
        (root/'document.md').write_text('# FD root\n\n![image](image.png)\n')
        (root/'image.png').write_bytes(png((224,96,80)))
        doc=text(root);identity=doc['root']
        a=probe(root,identity)['proof']
        image,payload=checked(root,a)
        assert image['status']=='image' and image['relative_source']==a and image['source']==a['file'] and payload==bytes((224,96,80))
        report['cases'].append('actual document FD root and checked image FD match returned proofs/RGB')
        modified=(root/'image.png').stat().st_mtime_ns
        (root/'next').write_bytes(png((56,112,176)))
        assert (root/'next').stat().st_size==(root/'image.png').stat().st_size
        os.utime(root/'next',ns=(modified,modified));os.replace(root/'next',root/'image.png')
        rejected(*checked(root,a))
        b=probe(root,identity)['proof'];assert b!=a
        value,payload=checked(root,b);assert payload==bytes((56,112,176)) and value['relative_source']==b
        report['cases'].append('probe A -> same path/size/mtime replacement B -> checked A rejected; fresh B accepted')
        os.rename(root,folder/'old');root.mkdir()
        (root/'document.md').write_text('# replacement root\n')
        (root/'image.png').write_bytes(png((224,96,80)))
        rejected(*checked(root,b));rejected(probe(root,identity),b'')
        assert text(root)['root']!=identity
        report['cases'].append('document root renamed/replaced -> both old probe authorization and checked image rejected')
        identity=text(root)['root'];(root/'sub').mkdir();(root/'sub'/'image.png').write_bytes(png((1,2,3)))
        sub=probe(root,identity,'sub/image.png')['proof']
        os.rename(root/'sub',root/'old-sub');(root/'sub').mkdir();(root/'sub'/'image.png').write_bytes(png((3,2,1)))
        rejected(*checked(root,sub,'sub/image.png'))
        report['cases'].append('intermediate directory replacement cannot satisfy previous file proof')
        (root/'link').symlink_to('image.png');(root/'alias').symlink_to('sub')
        for relative in ['link','alias/image.png','../root/image.png','/etc/passwd']:
            rejected(probe(root,identity,relative),b'')
        report['cases'].append('final/intermediate symlinks, parent traversal and absolute resources rejected')
        raw=b'raw-\xff.png';os.rename(os.fsencode(root/'image.png'),os.fsencode(root)+b'/'+raw)
        proof=probe(root,identity,list(raw))['proof'];value,payload=checked(root,proof,list(raw))
        assert value['relative_source']==proof and payload==bytes((224,96,80))
        report['cases'].append('opaque Unix component roundtrip remains lossless in probe and checked decoder')
    report['functional_gate']='PASS'
except BaseException as e:
    report['error']=repr(e)
    if isinstance(e,subprocess.CalledProcessError):report['stderr']=e.stderr[:2048].decode(errors='replace')
finally:
    report['helper_sha256']=hashlib.sha256(HELPER.read_bytes()).hexdigest()
    path=EVIDENCE/('relative-source-wire-'+str(time.time_ns())+'.json')
    with path.open('x') as out:json.dump(report,out,indent=2)
    print(path)
raise SystemExit(0 if report['functional_gate']=='PASS' else 1)
