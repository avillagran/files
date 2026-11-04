#!/usr/bin/env python3
"""Install the validated local checkpoint only; never builds/publishes or edits global config."""
import argparse,hashlib,json,os,shutil,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--report',required=True,type=Path);args=p.parse_args()
report=json.loads(args.report.read_text())
if report.get('functional_gate')!='PASS':raise SystemExit('Refusing checkpoint without scoped functional PASS')
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
release=ROOT/'target/release'
if report.get('binary_sha256')!=sha(release/'files'):raise SystemExit('App differs from the validated scene')
if report.get('helper_sha256')!=sha(release/'files-helper'):raise SystemExit('Helper differs from the validated scene')
# Validates native adapter recipe/source fingerprint and binary stamp, no compilation.
subprocess.run(['python3',str(ROOT/'scripts/stage-html-helper.py')],check=True)
stamp=json.loads((release/'files-html-helper.build.json').read_text())
if sha(release/'files-html-helper')!=stamp['binary_sha256']:raise SystemExit('HTML helper build identity mismatch')
if report.get('html_helper_sha256') and report['html_helper_sha256']!=stamp['binary_sha256']:raise SystemExit('HTML helper differs from the validated scene')
names=['files','files-helper','files-html-helper','files-html-helper.LICENSES','files-html-helper.build.json']
bin=Path.home()/'.local/bin';bin.mkdir(parents=True,exist_ok=True)
backup=Path.home()/'.cache/files-test/installed-backups'/str(time.time_ns());backup.mkdir(parents=True)
records=[];staged=[];replaced=[]
try:
    for name in names:
        source=release/name;destination=bin/name
        old=sha(destination) if destination.is_file() else None
        if destination.exists() or destination.is_symlink():shutil.copy2(destination,backup/name,follow_symlinks=False)
        fd,tmp=tempfile.mkstemp(prefix='.'+name+'-',dir=bin)
        with os.fdopen(fd,'wb') as out,source.open('rb') as inp:
            shutil.copyfileobj(inp,out);out.flush();os.fsync(out.fileno());os.fchmod(out.fileno(),0o755 if name in names[:3] else 0o644)
        if sha(Path(tmp))!=sha(source):raise RuntimeError('Staged hash mismatch: '+name)
        staged.append((name,Path(tmp),old));records.append({'name':name,'old_sha256':old,'release_sha256':sha(source)})
    for name,tmp,old in staged:
        destination=bin/name
        if (sha(destination) if destination.is_file() else None)!=old:raise RuntimeError('Concurrent local install detected: '+name)
        os.replace(tmp,destination);replaced.append(name)
    for record in records:
        record['installed_sha256']=sha(bin/record['name'])
        assert record['installed_sha256']==record['release_sha256']
except BaseException:
    for name in reversed(replaced):
        previous=backup/name
        if previous.exists() or previous.is_symlink():os.replace(previous,bin/name)
        else:(bin/name).unlink(missing_ok=True)
    raise
finally:
    for _,tmp,_ in staged:tmp.unlink(missing_ok=True)
result={'checkpoint':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'validation_report':str(args.report.resolve()),'scope':'per-user authorized installation; all staged then individual atomic replacements; no running windows controlled','backup':str(backup),'files':records,'resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED'}
path=ROOT/'docs/evidence/0.0.4'/('installed-checkpoint-'+str(time.time_ns())+'.json')
with path.open('x') as out:json.dump(result,out,indent=2)
print(path)
for record in records[:3]:print(record['name']+' '+record['installed_sha256'])
