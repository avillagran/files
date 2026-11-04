#!/usr/bin/env python3
"""Real release helper protocol smoke checks, fixture-only and no performance samples."""
import hashlib,json,os,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent;BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','method':'real release files-helper CLI and JSON contract on owned fixtures','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','cases':[]}
try:
    with tempfile.TemporaryDirectory(prefix='space-wire-',dir=BASE) as t:
        base=Path(t);root=base/'source';root.mkdir();(root/'a').write_bytes(b'a'*8);(root/'b').mkdir();(root/'b'/'inside').write_bytes(b'b'*4);(root/'c').write_bytes(b'cc');(root/'zero').touch();(root/'link').symlink_to(base/'outside');(base/'outside').write_bytes(b'ignored')
        def run(path,command='space_map'):
            result=subprocess.run([str(ROOT/'target/release/files-helper'),command],input=json.dumps({'command':'space_map','id':37,'path':str(path)}).encode()+b'\n',capture_output=True,timeout=10)
            if command!='space_map':assert result.returncode==2;return None
            assert result.returncode==0,result.stderr.decode();data=json.loads(result.stdout);assert data['id']==37;return data
        r=run(root);assert r['status']=='space_map';r=r['report'];assert r['complete'] and r['total_bytes']==14 and r['observed_bytes']==14
        assert {bytes(n['name']):n['bytes'] for n in r['nodes']}=={b'a':8,b'b':4,b'c':2,b'zero':0,b'link':0};report['cases'].append('real_cli_proportions_8_4_2_symlink_excluded')
        fd=os.open(os.fsencode(root)+b'/raw\xff',os.O_CREAT|os.O_WRONLY,0o600);os.close(fd);r=run(root)['report'];assert any(n['name']==list(b'raw\xff') for n in r['nodes']);report['cases'].append('lossless_child_name')
        (base/'alias').symlink_to(root,target_is_directory=True)
        for label,path in [('final_symlink',base/'alias'),('trailing_final_symlink',str(base/'alias')+'/'),('file_root',root/'a'),('relative_root','.'),('missing_root',base/'missing')]:
            r=run(path);assert r['status']=='error' and r['message'];report['cases'].append(label)
        run(root,'du');report['cases'].append('command_request_mismatch')
        empty=base/'empty';empty.mkdir();r=run(empty)['report'];assert r['complete'] and r['nodes']==[] and r['total_bytes']==0 and r['omitted_children']==0;report['cases'].append('empty_zero_without_fake_nodes')
        assert (base/'outside').read_bytes()==b'ignored' and (root/'b'/'inside').read_bytes()==b'bbbb'
        report['functional_gate']='PASS'
except BaseException as error:
    report.update(functional_gate='FAIL',error=repr(error));raise
finally:
    report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
    path=ROOT/'docs/evidence/space-map'/('helper-wire-'+str(time.time_ns())+'.json')
    with path.open('x') as out:json.dump(report,out,indent=2)
    print(path)
