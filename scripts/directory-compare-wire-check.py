#!/usr/bin/env python3
"""Owned comparison helper protocol checks; no GUI, clipboard or resource measurements."""
import argparse,hashlib,json,os,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--helper',type=Path,default=ROOT/'target/debug/files-helper');args=p.parse_args()
helper=args.helper.resolve();out=ROOT/'docs/evidence/directory-compare-wire';out.mkdir(parents=True,exist_ok=True)
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','scope':'Owned real helper CLI protocol only, no visible comparison tool or synchronization','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','cases':[],'helper_sha256':hashlib.sha256(helper.read_bytes()).hexdigest()}
def path_wire(path):
    value=os.fsencode(path)
    try:return value.decode('utf8')
    except UnicodeDecodeError:return list(value)
def raw(value):return value.encode('utf8')if isinstance(value,str)else bytes(value)
def invoke(label,request,command='compare_directories',exit_ok=True):
    data=(json.dumps(request,ensure_ascii=False)+'\n').encode('utf8')if not isinstance(request,bytes)else request
    child=subprocess.run([str(helper),command],input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=25)
    case={'label':label,'returncode':child.returncode,'stderr':child.stderr.decode('utf8',errors='replace'),'stdout_bytes':len(child.stdout)};report['cases'].append(case)
    assert (child.returncode==0)==exit_ok,(label,case)
    if not exit_ok:assert not child.stdout;return []
    assert child.stdout.endswith(b'\n')and len(child.stdout)<=21237760
    frames=[]
    for line in child.stdout.splitlines():
        assert len(line)<=65536
        frame=json.loads(line);assert frame['id']==request['id'];frames.append(frame)
    case['frames']=frames;return frames
def reconstruct(frames):
    rows=[];terminal=None;pages=0
    for frame in frames:
        e=frame['event'];kind=e['kind']
        assert terminal is None
        if kind=='page':
            assert e['offset']==len(rows)and 0<len(e['rows'])<=128
            assert sum(len(raw(r['path']))for r in e['rows'])<=4096
            rows.extend(e['rows']);pages+=1
        elif kind=='done':terminal=e
        else:assert kind=='progress'
    assert terminal is not None and terminal['progress']['compared']==len(rows)
    names=[raw(r['path'])for r in rows]
    assert len(names)==len(set(names))and names==sorted(names,key=lambda n:tuple(n.split(b'/')))
    for name in names:assert not name.startswith(b'/')and all(c not in [b'',b'.',b'..']for c in name.split(b'/'))
    return {raw(r['path']):r for r in rows},terminal,pages
try:
    with tempfile.TemporaryDirectory(prefix='directory-compare-wire-',dir=base)as tmp:
        root=os.fsencode(tmp);left=root+b'/left-\xff';right=root+b'/right';os.mkdir(left);os.mkdir(right)
        for folder in [left,right]:
            os.mkdir(folder+b'/nested');os.mkdir(folder+b'/empty')
            for i in range(130):
                with open(folder+('/file-%03d'%i).encode(),'wb')as f:f.write(b'x')
            with open(folder+b'/nested/raw\xff\n','wb')as f:f.write(b'raw')
            os.symlink(b'../outside\xff',folder+b'/literal-link')
        with open(left+b'/changed','wb')as f:f.write(b'left')
        with open(right+b'/changed','wb')as f:f.write(b'diff')
        with open(left+b'/only-left','wb')as f:f.write(b'left')
        with open(right+b'/raw\xef\xbf\xbd\n','wb')as f:f.write(b'shadow')
        expected=[{'device':os.stat(d).st_dev,'inode':os.stat(d).st_ino}for d in [left,right]]
        req={'command':'compare_directories','id':11,'left':path_wire(left),'right':path_wire(right),'expected':expected,'read_limit':1024*1024}
        rows,terminal,pages=reconstruct(invoke('raw-recursive-paged',req))
        assert pages>=2 and terminal['roots']==expected and terminal['progress']['reasons']==0
        assert rows[b'changed']['difference']=='Content'and rows[b'only-left']['difference']=='OnlyLeft'
        assert rows[b'nested/raw\xff\n']['difference']=='Equal'and rows[b'raw\xef\xbf\xbd\n']['difference']=='OnlyRight'
        assert rows[b'literal-link']['difference']=='Equal'and not any(n.startswith(b'literal-link/')for n in rows)
        limited={**req,'id':12,'read_limit':0};rows,terminal,_=reconstruct(invoke('explicit-read-limit',limited))
        assert terminal['progress']['bytes_read']==0 and terminal['progress']['reasons']&8 and rows[b'nested/raw\xff\n']['difference']=='Unknown'and rows[b'nested']['difference']=='Unknown'
        error=invoke('captured-root-replaced',{**req,'id':13,'expected':[{'device':0,'inode':0}]*2});assert len(error)==1 and error[0]['event']['kind']=='error'
        error=invoke('over-budget-before-open',{**req,'id':14,'left':'/does-not-exist','read_limit':1024**4+1});assert len(error)==1 and error[0]['event']['kind']=='error'
        invoke('mismatched-command',req,command='hash_files',exit_ok=False)
        invoke('nul-path-rejected',{**req,'left':[47,0,65]},exit_ok=False)
        invoke('oversize-request',b'x'*65537,exit_ok=False)
        with open(left+b'/changed','rb')as f:assert f.read()==b'left'
        with open(right+b'/changed','rb')as f:assert f.read()==b'diff'
        report['functional_gate']='PASS';report['scope']+='; raw root/name and Unicode shadow, full pages, captured root IDs, content/link/absence/read-limit, malformed input/command rejection and unchanged bytes, normal subprocess wait. No native input/clipboard/AT-SPI/2x/global lifetime claim.'
except BaseException as error:
    report.update(functional_gate='FAIL',error=repr(error));raise
finally:
    file=out/('helper-wire-'+str(time.time_ns())+'.json');file.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(file)
