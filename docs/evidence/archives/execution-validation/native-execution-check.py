#!/usr/bin/env python3
"""Owned core-example filesystem contracts; no UI/native input/resource sampling."""
import argparse,hashlib,io,json,os,stat,subprocess,tempfile,warnings,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
PROBE=ROOT/'target/release/examples/archive-extract-probe'
parser=argparse.ArgumentParser();parser.add_argument('--report',type=Path,default=Path(__file__).with_name('native-execution.json'));REPORT=parser.parse_args().report
R={'functional_gate':'IN_PROGRESS','product_ui_gate':'NOT_IMPLEMENTED','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','method':'independent Python zipfile sources and explicit core example child processes, private repo fixture paths only','cases':[]}
def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
class Sequential(io.BytesIO):
    def seekable(self):return False
    def seek(self,*args):raise io.UnsupportedOperation('owned nonseekable fixture')
def package(entries,codec=zipfile.ZIP_DEFLATED,descriptor=False):
    buffer=Sequential() if descriptor else io.BytesIO()
    with warnings.catch_warnings():
        warnings.simplefilter('ignore',UserWarning)
        with zipfile.ZipFile(buffer,'w',compression=codec)as z:
            for name,data,mode in entries:
                info=zipfile.ZipInfo(name);info.create_system=3;info.external_attr=mode<<16
                z.writestr(info,data,compress_type=codec)
    return buffer.getvalue()
def invoke(mode,source,destination,name,*flags):
    run=subprocess.run([os.fsencode(PROBE),mode,source,destination,name,*flags],capture_output=True,timeout=25)
    result=json.loads(run.stdout);return run,result
def tree(root):
    rows=[]
    for parent,dirs,files in os.walk(root,followlinks=False):
        for name in sorted(dirs+files):
            path=os.path.join(parent,name);s=os.lstat(path);rel=os.path.relpath(path,root)
            rows.append({'path_raw':list(rel),'mode':stat.S_IMODE(s.st_mode),'kind':'directory'if stat.S_ISDIR(s.st_mode)else'file','bytes':s.st_size if stat.S_ISREG(s.st_mode)else None,'sha256':hashlib.sha256(open(path,'rb').read()).hexdigest()if stat.S_ISREG(s.st_mode)else None})
    return sorted(rows,key=lambda x:x['path_raw'])
try:
    assert PROBE.is_file()
    inert=subprocess.run([str(PROBE)],capture_output=True,timeout=5);assert inert.returncode==0 and b'Usage:'in inert.stdout
    R['inert_default']=True;R['probe_sha256']=digest(PROBE)
    with tempfile.TemporaryDirectory(prefix='archive-exec-contract-',dir=ROOT/'.cargo/tmp')as folder:
        base=os.fsencode(folder)
        entries=[('a/run.sh',b'#!/bin/sh\necho owned\n',0o100755),('a/raw-X',b'raw bytes',0o100600),('empty/',b'',0o040755)]
        def fixture(label,data,raw=False):
            here=os.path.join(base,label.encode());os.mkdir(here)
            source=os.path.join(here,b'input-\xfe.zip'if raw else b'input.zip');destination=os.path.join(here,b'dest-\xfe'if raw else b'dest');os.mkdir(destination)
            with open(source,'wb')as out:out.write(data)
            return source,destination
        for label,codec,descriptor,restrictive,raw in [
            ('stored',zipfile.ZIP_STORED,False,False,False),('deflate',zipfile.ZIP_DEFLATED,False,False,False),('descriptor',zipfile.ZIP_DEFLATED,True,False,False),('restrictive-umask',zipfile.ZIP_DEFLATED,True,True,False),('opaque-paths',zipfile.ZIP_DEFLATED,False,False,True)]:
            data=package(entries,codec,descriptor).replace(b'raw-X',b'raw-\xff')
            source,destination=fixture(label,data,raw);name=b'out-\xfd'if raw else b'out';before=digest(os.fsdecode(source));s0=os.stat(source)
            run,prepared=invoke(b'--prepare',source,destination,name);assert run.returncode==0 and prepared['phase']=='prepared_readonly' and os.listdir(destination)==[]
            assert prepared['files']==2 and prepared['directories']==2 and prepared['retained_fds']==3
            run,result=invoke(b'--extract',source,destination,name,*([b'--umask-0777']if restrictive else[]));assert run.returncode==0 and result['phase']=='published_synced'
            root=os.path.join(destination,name);rows=tree(root);assert os.listdir(destination)==[name]
            wanted={b'a/run.sh':entries[0][1],b'a/raw-\xff':entries[1][1]}
            for rel,data in wanted.items():
                with open(os.path.join(root,rel),'rb')as f:assert f.read()==data
            assert os.path.isdir(os.path.join(root,b'empty'))
            assert stat.S_IMODE(os.stat(root).st_mode)==0o700
            for row in rows:assert row['mode']==(0o700 if row['kind']=='directory'or bytes(row['path_raw'])==b'a/run.sh'else 0o600)
            assert result['bytes']==sum(map(len,wanted.values())) and result['files']==2 and result['directories']==2
            assert digest(os.fsdecode(source))==before and(os.stat(source).st_mtime_ns,os.stat(source).st_ctime_ns)==(s0.st_mtime_ns,s0.st_ctime_ns)
            run,occupied=invoke(b'--extract',source,destination,name);assert run.returncode==1 and occupied['phase']=='prepare' and tree(root)==rows
            R['cases'].append({'name':label,'passed':True,'readonly_prepare':prepared,'outcome':result,'source_unchanged':True,'tree':rows,'second_attempt_preserved_occupant':True})
        for label,entries_bad in [('traversal',[('../escape',b'x',0o100600)]),('symlink',[('link',b'../../outside',0o120777)]),('duplicate',[('same',b'one',0o100600),('same',b'two',0o100600)]),('file-prefix',[('a',b'one',0o100600),('a/b',b'two',0o100600)])]:
            source,destination=fixture(label,package(entries_bad));before=digest(os.fsdecode(source));run,result=invoke(b'--extract',source,destination,b'out');assert run.returncode==1 and result['phase']=='prepare' and os.listdir(destination)==[] and digest(os.fsdecode(source))==before
            R['cases'].append({'name':label,'passed':True,'outcome':result,'destination_unchanged':True})
        corrupt=bytearray(package([('one',b'payload',0o100600)],zipfile.ZIP_STORED));central=corrupt.find(b'PK\x01\x02');crc=int.from_bytes(corrupt[14:18],'little')^1;corrupt[14:18]=crc.to_bytes(4,'little');corrupt[central+16:central+20]=crc.to_bytes(4,'little')
        source,destination=fixture('bad-crc',corrupt);run,result=invoke(b'--extract',source,destination,b'out');assert run.returncode==1 and result['phase']=='execute' and result['publication']=='Unpublished' and result['cleanup']=='Complete' and os.listdir(destination)==[]
        R['cases'].append({'name':'bad-crc','passed':True,'outcome':result})
        source,destination=fixture('cancel-progress',package([('large',b'x'*250000,0o100600)]));before=digest(os.fsdecode(source));run,result=invoke(b'--extract',source,destination,b'out',b'--cancel-after-progress');assert run.returncode==1 and result['cleanup']=='Complete' and 0<result['provisional_bytes']<250000 and os.listdir(destination)==[] and digest(os.fsdecode(source))==before
        R['cases'].append({'name':'cancel-after-progress','passed':True,'outcome':result,'source_unchanged':True})
        source,destination=fixture('empty',package([]));run,result=invoke(b'--extract',source,destination,b'out');assert run.returncode==0 and result['bytes']==0 and os.listdir(os.path.join(destination,b'out'))==[]
        R['cases'].append({'name':'empty-archive','passed':True,'outcome':result})
    R['functional_gate']='PASS'
except BaseException as error:
    R['functional_gate']='FAIL';R['error']=repr(error);raise
finally:
    with REPORT.open('x')as out:json.dump(R,out,ensure_ascii=False,indent=2);out.write('\n')
    print(R['functional_gate'],REPORT)
