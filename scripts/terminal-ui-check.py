#!/usr/bin/env python3
"""Owned terminal launch scene using existing FLTK Action/framebuffer fixture.
Inert without --attempt. --native also exercises an owned transient user unit,
real default terminal and a bounded PTY/cwd writer; no OS input/config changes.
No resource/startup sampler. Original failures are retained.
"""
import argparse,ast,ctypes,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
P=argparse.ArgumentParser();P.add_argument('--attempt',action='store_true');P.add_argument('--native',action='store_true');args=P.parse_args()
if not args.attempt:P.print_help();raise SystemExit(0)
ROOT=Path(__file__).resolve().parent.parent;OUT=ROOT/'docs/evidence/terminal';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'FAIL','native_terminal_gate':'NOT_ATTEMPTED','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','method':'existing private Action/painted handlers + FLTK framebuffer, no OS input','captures':[],'states':[],'runs':[]}
# Only this harness adopts/reaps its own orphaned fake launcher, never product behavior.
if ctypes.CDLL(None,use_errno=True).prctl(36,1,0,0,0)!=0:raise RuntimeError('private subreaper unavailable')
class App:
    def __init__(self,root,folder,env,index):
        self.log=root/f'app-{index}.log';self.frame=root/f'frame-{index}.ppm';self.seq=0
        self.env={**env,'FILES_TEST_CAPTURE':str(self.frame)}
        self.out=self.log.open('w')
        self.p=subprocess.Popen([str(ROOT/'target/release/files'),os.fsdecode(folder),'--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','120'],env=self.env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=self.out,start_new_session=True)
        self.state();assert 'nerd=true'in self.log.read_text()
    def wait(self,check,why,exit_ok=False):
        until=time.monotonic()+25
        while time.monotonic()<until:
            value=check()
            if value:return value
            if self.p.poll()is not None and not exit_ok:raise RuntimeError('app exited: '+self.log.read_text()[-2500:])
            time.sleep(.02)
        raise AssertionError(why)
    def state(self,predicate=lambda s:True):
        def ready():
            for line in reversed(self.log.read_text().splitlines()):
                if line.startswith('FILES_HIDDEN_STATE '):
                    s=json.loads(line.split(' ',1)[1])
                    if s['seq']==self.seq and predicate(s):return s
        s=self.wait(ready,'state absent '+str(self.seq));self.dims=(s['width'],s['height']);report['states'].append(s);return s
    def command(self,action,exit_ok=False):
        offset=len(self.log.read_text());pending=self.frame.with_suffix('.pending');pending.write_text(action);pending.replace(self.frame.with_suffix('.command'))
        m=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',self.log.read_text()[offset:],re.M),'command absent '+action,exit_ok);self.seq=int(m[1])
    def act(self,action,predicate=lambda s:True):self.command(action);return self.state(predicate)
    def capture(self,name,dims=None):
        if dims is None:dims=self.dims
        self.frame.unlink(missing_ok=True);self.frame.with_suffix('.request').touch()
        def complete():
            if not self.frame.exists():return False
            parts=self.frame.read_bytes().split(b'\n',3)
            if len(parts)!=4:return False
            w,h=map(int,parts[1].split());return(w,h)if len(parts[3])==w*h*3 else False
        assert self.wait(complete,'frame incomplete')==dims
        path=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['/usr/bin/magick',str(self.frame),str(path)],check=True,timeout=15)
        assert path.read_bytes()[16:24]==dims[0].to_bytes(4,'big')+dims[1].to_bytes(4,'big');report['captures'].append({'path':str(path),'dimensions':dims})
    def close(self):
        self.command('callback',True);self.wait(lambda:self.p.poll()is not None,'close absent',True)
        assert self.p.returncode==0 and 'FILES_CLOSE_DRAINED'in self.log.read_text();self.out.close();report['runs'].append({'exit':0,'after_ack':True,'log':self.log.read_text()})
    def cleanup(self):
        if self.p.poll()is None:
            try:os.killpg(self.p.pid,signal.SIGTERM)
            except ProcessLookupError:pass
            try:self.p.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(self.p.pid,signal.SIGKILL);self.p.wait(timeout=3)
        self.out.close()
def identity(pid):
    try:fields=Path('/proc',str(pid),'stat').read_text().rsplit(')',1)[1].split();return fields[19],fields[0]
    except FileNotFoundError:return None
def gone(pid,start):
    current=identity(pid);return current is None or current[0]!=start
def probe_lines(path):
    if not path.exists():return []
    text=path.read_text();assert len(text)<=256*1024
    lines=text.splitlines()
    if text and not text.endswith('\n'):lines=lines[:-1]
    return[json.loads(line)for line in lines]
app=None;unit=None;native_owned=False
with tempfile.TemporaryDirectory(prefix='terminal-',dir=BASE)as temp:
    root=Path(temp);folder=root/os.fsdecode(b"Projects \xff '$()'");folder.mkdir();other=root/'Other';other.mkdir();(folder/'selected-child').mkdir();(folder/'source.txt').write_bytes(b'unchanged')
    bin=root/'bin';bin.mkdir();(bin/'fc-match').symlink_to('/usr/bin/fc-match');config=root/'config';config.mkdir();(config/'config.toml').write_text('show_hidden=false\n');(root/'tmp').mkdir()
    mode=root/'mode';events=root/'events';release=root/'release';ready=root/'native-ready';native_release=root/'native-release';writer=root/'writer.py';native_log=root/'native.log';host_path=os.environ['PATH']
    unit='files-terminal-fixture-'+str(time.time_ns())+'.service'
    writer.write_text("import os,sys,json,time\nfrom pathlib import Path\np=Path('/proc/self/stat').read_text().rsplit(')',1)[1].split()\npending=Path(sys.argv[1]+'.pending')\npending.write_text(json.dumps(dict(pid=os.getpid(),start=p[19],cwd=os.getcwdb().hex(),tty0=os.isatty(0),tty1=os.isatty(1),cgroup=Path('/proc/self/cgroup').read_text()[:4096])))\npending.replace(sys.argv[1])\nend=time.monotonic()+30\nwhile not Path(sys.argv[2]).exists()and time.monotonic()<end:time.sleep(.02)\nraise SystemExit(0 if Path(sys.argv[2]).exists()else 4)\n")
    common=f"import os,sys,json,time,subprocess\nfrom pathlib import Path\nMODE=Path({str(mode)!r}).read_text()\nEVENTS=Path({str(events)!r})\np=Path('/proc/self/stat').read_text().rsplit(')',1)[1].split()\n"
    broker=bin/'uwsm-app';broker.write_text('#!/usr/bin/python3\n'+common+f"assert sys.argv[1:4]==['-t','service','--']and sys.argv[5]=='terminal'\nassert all(a.encode('utf8')for a in sys.argv[1:])\nassert len(sys.argv[6])<=8192 and all(c in '0123456789abcdef'for c in sys.argv[6])\nif MODE=='native':\n args=['/usr/bin/uwsm-app','-u',{unit!r},'-p',{'Environment=PATH='+str(bin)+':'+host_path!r},*sys.argv[1:]]\n assert all(a.encode('utf8')for a in args)\n with Path({str(native_log)!r}).open('w')as out:code=subprocess.run(args,env={{**os.environ,'PATH':{host_path!r}}},stdout=out,stderr=out).returncode\n raise SystemExit(code)\nos.execv(sys.argv[4],sys.argv[4:])\n");broker.chmod(0o700)
    terminal=bin/'xdg-terminal-exec';terminal.write_text('#!/usr/bin/python3\n'+common+f"entry=dict(pid=os.getpid(),start=p[19],cwd=os.getcwdb().hex(),args=[os.fsencode(a).hex()for a in sys.argv[1:]],mode=MODE)\nwith EVENTS.open('a')as out:out.write(json.dumps(entry)+'\\n')\nif MODE=='native':\n assert sys.argv[-1]=='--'\n args=['/usr/bin/xdg-terminal-exec',*sys.argv[1:-1],{'--app-id='+unit[:-8]!r},'--title=Files terminal fixture','--','/usr/bin/python3',{str(writer)!r},{str(ready)!r},{str(native_release)!r}]\n os.execve(args[0],args,{{**os.environ,'PATH':{host_path!r}}})\nif MODE=='hold':\n end=time.monotonic()+20\n while not Path({str(release)!r}).exists()and time.monotonic()<end:time.sleep(.02)\n raise SystemExit(0 if Path({str(release)!r}).exists()else 4)\nraise SystemExit(7 if MODE=='fail7'else 0)\n");terminal.chmod(0o700)
    for source in [writer,broker,terminal]:ast.parse(source.read_text())
    env={**os.environ,'PATH':str(bin),'TMPDIR':str(root/'tmp'),'MAGICK_TEMPORARY_PATH':str(root/'tmp'),'PYTHONDONTWRITEBYTECODE':'1','FILES_CONFIG_DIR':str(config),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_OPEN':'1','FILES_CLOSE_TRACE':'1','FILES_TEST_DUAL_B':str(other),'RUST_BACKTRACE':'1'}
    def exact(entry,path):
        assert bytes.fromhex(entry['cwd'])==os.fsencode(path)
        assert[bytes.fromhex(a)for a in entry['args']]==[b'--dir='+os.fsencode(path),b'--']
    try:
        mode.write_text('ok');app=App(root,folder,env,1);app.act('list');app.act('select-name:selected-child');app.act('clip-actions');app.capture('terminal-actions');s=app.act('terminal-hit',lambda s:s['launcher']is None);assert 'FILES_TERMINAL_OUTCOME=opened'in app.log.read_text();exact(probe_lines(events)[-1],folder);app.capture('terminal-request-sent-raw-directory')
        app.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);app.act('clip-actions');app.capture('terminal-actions-all-fit-440');app.act('clip-actions');app.act('fixture-wide',lambda s:s['width']==1000 and s['height']==560)
        app.act('dual-on');app.act('dual-switch');app.act('dual-b-location',lambda s:s['path']==str(other)and s['dual']['panels'][0]['loaded']);app.act('dual-menu-key');app.capture('terminal-context-current-pane');s=app.act('terminal-context',lambda s:s['launcher']is None);exact(probe_lines(events)[-1],other)
        mode.write_text('fail7');app.act('commands-shortcut');s=app.act('commands-query:terminal');assert s['commands']['matches'][0]['id']=='terminal';app.capture('terminal-keyboard-command');s=app.act('commands-key:enter',lambda s:s['launcher']is None and '7'in s['error']);app.capture('terminal-launch-error7')
        broker.unlink();app.act('clip-actions');s=app.act('terminal-hit',lambda s:s['launcher']is None and 'no disponible'in s['error']);app.capture('terminal-launcher-unavailable');broker.write_text('#!/usr/bin/python3\n'+common+"os.execv(sys.argv[4],sys.argv[4:])\n");broker.chmod(0o700)
        mode.write_text('hold');count=len(probe_lines(events));app.act('clip-actions');app.act('terminal-hit',lambda s:s['launcher']=='Terminal');child=app.wait(lambda:probe_lines(events)[-1]if len(probe_lines(events))>count else False,'hold launcher absent');exact(child,other)
        app.act('commands-shortcut');s=app.act('commands-query:terminal');assert s['commands']['matches'][0]['reason']is not None;app.capture('terminal-single-flight-disabled-command');app.act('commands-key:escape')
        app.act('clip-actions');s=app.act('terminal-hit');assert 'pendiente'in s['error']and len(probe_lines(events))==count+1
        app.close();live=identity(child['pid']);assert live and live[0]==child['start']and live[1]!='Z';release.touch();pid,status=os.waitpid(child['pid'],0);assert pid==child['pid']and os.waitstatus_to_exitcode(status)==0 and gone(pid,child['start']);app=None;report['external_launcher_survives_files_exit_then_reaped']=True
        if args.native:
            # Restore full broker after the deliberate missing-command case.
            broker.write_text('#!/usr/bin/python3\n'+common+f"assert sys.argv[1:4]==['-t','service','--']and sys.argv[5]=='terminal'\nargs=['/usr/bin/uwsm-app','-u',{unit!r},'-p',{'Environment=PATH='+str(bin)+':'+host_path!r},*sys.argv[1:]]\nassert all(a.encode('utf8')for a in args)\nwith Path({str(native_log)!r}).open('w')as out:code=subprocess.run(args,env={{**os.environ,'PATH':{host_path!r}}},stdout=out,stderr=out).returncode\nraise SystemExit(code)\n");broker.chmod(0o700)
            absent=subprocess.run(['/usr/bin/systemctl','--user','show',unit,'--property=LoadState','--value'],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,timeout=3);assert absent.stdout.strip()in(b'not-found',b'')
            mode.write_text('native');app=App(root,folder,env,2);app.act('list');app.act('select-name:selected-child');app.act('commands-shortcut');app.act('commands-query:terminal');app.act('commands-key:enter',lambda s:s['launcher']is None)
            props=subprocess.run(['/usr/bin/systemctl','--user','show',unit,'--property=ExecStart','--value'],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,timeout=3)
            native_owned=os.fsencode(ROOT/'target/release/files-helper')in props.stdout and os.fsencode(folder).hex().encode()in props.stdout
            native=app.wait(lambda:json.loads(ready.read_text())if ready.exists()else None,'native terminal writer absent');assert bytes.fromhex(native['cwd'])==os.fsencode(folder)and native['tty0']and native['tty1'];assert unit in native['cgroup'];native_owned=True
            terminal_event=probe_lines(events)[-1];assert terminal_event['mode']=='native';exact(terminal_event,folder)
            def own_window():
                if gone(terminal_event['pid'],terminal_event['start']):raise AssertionError('native terminal identity lost')
                result=subprocess.run(['/usr/bin/hyprctl','clients','-j'],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,timeout=3)
                if result.returncode or len(result.stdout)>2*1024*1024:return None
                for client in json.loads(result.stdout):
                    if client.get('pid')==terminal_event['pid']and client.get('mapped')and str(client.get('class','')).startswith(unit[:-8]):return{k:client.get(k)for k in ['pid','class','mapped']}
            report['native_window']=app.wait(own_window,'owned actual terminal not mapped')
            report['native_writer']={k:v for k,v in native.items()if k!='cgroup'};app.state();app.capture('terminal-native-pty-cwd-confirmed');app.close();assert identity(native['pid'])and identity(native['pid'])[0]==native['start'];native_release.touch();app.wait(lambda:gone(native['pid'],native['start']),'native writer identity not gone',True)
            app.wait(lambda:subprocess.run(['/usr/bin/systemctl','--user','is-active',unit],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=3).returncode!=0,'native unit still active',True);app=None;native_owned=False;report['native_terminal_gate']='PASS controlled actual default terminal/PTY/raw cwd; proxy adds owned writer/options, no interactive input'
        assert(folder/'source.txt').read_bytes()==b'unchanged';report.update(functional_gate='PASS',source_bytes_unchanged=True,scope='Actions/context/keyboard; bounded ASCII broker→real helper decode→raw cwd/argv, unavailable/error/singleflight, root0ACK and external independent lifecycle; no resource/startup/native input proof')
    except BaseException as error:report['error']=repr(error);raise
    finally:
        release.touch();native_release.touch()
        if app:report['failed_log']=app.log.read_text();app.cleanup()
        if native_owned:subprocess.run(['/usr/bin/systemctl','--user','stop',unit],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=5)
        report['native_launch_log']=native_log.read_text()[:8192]if native_log.exists()else'';report['events']=probe_lines(events)
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=hashlib.sha256((ROOT/'target/release'/name).read_bytes()).hexdigest()
        target=OUT/('terminal-ui-'+str(time.time_ns())+'.json');target.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(target)
