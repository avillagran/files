#!/usr/bin/env python3
"""Inert by default. --attempt exercises only private loopback SSH/SFTP and owned FUSE mount."""
import argparse,ctypes,hashlib,json,os,pwd,re,select,signal,socket,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--attempt',action='store_true')
parser.add_argument('--sshfs',type=Path)
parser.add_argument('--connection-engine',action='store_true',help='Use actual core Connection process engine (debug lab only)')
parser.add_argument('--engine-close',action='store_true',help='Exit the core engine owner while mounted; private lab then disconnects')
parser.add_argument('--engine-bad-hostkey',action='store_true',help='Private wrong known-host key must be rejected by core engine')
parser.add_argument('--engine-registry',action='store_true',help='Durably register engine and inspect from a separate process')
parser.add_argument('--recovered-control',action='store_true',help='Explicit recovered controller performs busy/retry after original owner exits')
parser.add_argument('--ui-control',action='store_true',help='Use production UI recovered controls in private fixture after owner exit')
parser.add_argument('--ui-copy',action='store_true',help='Extend product UI scene with explicit SFTP copy policy and cancellation')
args=parser.parse_args()
if (args.engine_close or args.engine_bad_hostkey or args.engine_registry) and not args.connection_engine:parser.error('Engine scenarios require --connection-engine')
if args.engine_close and args.engine_bad_hostkey:parser.error('Choose one engine scenario')
if args.recovered_control and not(args.engine_registry and args.engine_close):parser.error('--recovered-control requires --engine-registry --engine-close')
if args.ui_copy and not args.ui_control:parser.error('--ui-copy requires --ui-control')
if args.ui_control and not(args.engine_registry and args.engine_close) :parser.error('--ui-control requires --engine-registry --engine-close')
if args.ui_control and args.recovered_control:parser.error('Choose UI or core recovered control')
if not args.attempt:
    parser.print_help();raise SystemExit(0)
if args.sshfs is None or not args.sshfs.is_absolute() or not args.sshfs.is_file() or not os.access(args.sshfs,os.X_OK):
    parser.error('--attempt requires an absolute executable --sshfs path')
OUT=ROOT/'docs/evidence/sftp-native';OUT.mkdir(parents=True,exist_ok=True)
LAB=ROOT/'.cargo/tmp/sftp-lab';LAB.mkdir(parents=True,exist_ok=True)
base=Path(tempfile.mkdtemp(prefix='loopback-',dir=LAB));os.chmod(base,0o700)
if args.ui_control:
    base=base/'sftp';base.mkdir(mode=0o700)
point=base/'mount'
source=base/'remote ñ%20$()';source.mkdir(mode=0o700)
(source/'alpha.txt').write_bytes(b'Files owned loopback SFTP content\n')
raw=b'raw-\xff.bin'
with open(os.fsencode(source)+b'/'+raw,'wb')as f:f.write(bytes(range(256)))
(source/'nested').mkdir();(source/'nested/child').write_bytes(b'child bytes');(source/'relative-link').symlink_to('alpha.txt')
marker='files-sftp-'+str(time.time_ns());mount_marker=marker
readonly_control=None;recovered_control=None;point_known=True;point_guard=None;server=None;client=None;busy=None;mounted=None;identities=set();identity_errors=[];report={'mount_gate':'IN_PROGRESS','functional_gate':'NOT_APPLICABLE','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','fixture':str(base),'captures':[],'method':'actual private loopback OpenSSH internal-sftp and real SSHFS/FUSE; core Target argv and strict mountinfo parser; no UI or OS input; no global install/config/key change'}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def ident(pid):
    path=Path('/proc',str(pid),'stat')
    try:
        try:raw=path.read_text()
        except FileNotFoundError:
            if not path.parent.exists():return None
            raw=path.read_text()
        fields=raw.rsplit(')',1)[1].split();return (pid,fields[19],fields[0])
    except OSError as error:
        identity_errors.append({'pid':pid,'errno':error.errno,'kind':'identity unreadable, not counted as gone'})
        return (pid,None,'?')
def remember():
    queue=[os.getpid()];seen=set()
    while queue:
        pid=queue.pop()
        if pid in seen:continue
        seen.add(pid)
        for f in Path('/proc',str(pid),'task').glob('*/children'):
            try:children=[int(v)for v in f.read_text().split()]
            except FileNotFoundError:continue
            for child in children:
                value=ident(child)
                if value is not None and value[1] is not None:identities.add(value[:2]);queue.append(child)
def run(argv,timeout=10):
    return subprocess.run(argv,stdin=subprocess.DEVNULL,capture_output=True,timeout=timeout)
def probe():
    p=run([str(ROOT/'target/debug/examples/mount-table-probe'),'--point',str(point),mount_marker],3)
    if p.returncode:raise AssertionError('strict mountinfo query failed: '+p.stderr.decode(errors='replace')[:1024])
    return json.loads(p.stdout)
class ExpectedHostKeyRejection(Exception):pass
def expect(ok,why):
    if not ok:raise AssertionError(why)
def guard_reply(command=None,child=None):
    child=point_guard if child is None else child
    if command is not None:child.stdin.write(command.encode()+b'\n');child.stdin.flush()
    deadline=time.monotonic()+3;data=bytearray()
    while b'\n'not in data:
        remaining=deadline-time.monotonic()
        expect(remaining>0 and select.select([child.stdout],[],[],max(0,remaining))[0],'point guard response deadline')
        chunk=os.read(child.stdout.fileno(),4096)
        expect(bool(chunk),'point guard ended before response');data.extend(chunk)
        expect(len(data)<=32768,'point guard response limit')
    expect(data.endswith(b'\n')and data.count(b'\n')==1,'point guard response framing')
    return json.loads(data)
def cfgpath(p):return '"'+str(p).replace('\\','\\\\').replace('"','\\"')+'"'
try:
    libc=ctypes.CDLL(None,use_errno=True)
    expect(libc.prctl(36,1,0,0,0)==0,'private subreaper setup failed')
    if not args.connection_engine:
        point_guard=subprocess.Popen([str(ROOT/'target/debug/examples/mount-point-probe'),'--attempt',str(base)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        remember();prepared=guard_reply();point=Path(os.fsdecode(bytes(prepared['point'])))
        expect(point.parent==base,'point guard returned another parent')
        report['point_provenance']={'created_by_core':True,'before_mount':guard_reply('empty')}
        expect(report['point_provenance']['before_mount']['ok'],'core point preflight rejected')
    expect(not probe()['present'],'private mountpoint unexpectedly occupied')
    for name in ['host_ed25519','client_ed25519']:
        p=run(['/usr/bin/ssh-keygen','-q','-t','ed25519','-N','','-f',str(base/name)])
        expect(p.returncode==0,'owned fixture key generation failed')
    with socket.socket()as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    user=pwd.getpwuid(os.getuid()).pw_name
    pub=(base/('client_ed25519.pub'if args.engine_bad_hostkey else'host_ed25519.pub')).read_text().split()
    (base/'known_hosts').write_text(f'[127.0.0.1]:{port} {pub[0]} {pub[1]}\n')
    config=base/'sshd_config'
    config.write_text(f'''Port {port}
ListenAddress 127.0.0.1
HostKey {cfgpath(base/'host_ed25519')}
PidFile {cfgpath(base/'sshd.pid')}
AuthorizedKeysFile {cfgpath(base/'client_ed25519.pub')}
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
UsePAM no
PermitRootLogin no
PermitEmptyPasswords no
AllowUsers {user}
AllowTcpForwarding no
X11Forwarding no
PermitTunnel no
Subsystem sftp internal-sftp
LogLevel ERROR
''')
    client_config=base/'ssh_config'
    client_config.write_text(f'''Host {marker}
 HostName 127.0.0.1
 User {user}
 Port {port}
 IdentityFile {cfgpath(base/'client_ed25519')}
 IdentityAgent none
 IdentitiesOnly yes
 UserKnownHostsFile {cfgpath(base/'known_hosts')}
 GlobalKnownHostsFile /dev/null
 StrictHostKeyChecking yes
 BatchMode yes
 ControlMaster no
 ControlPath none
 ProxyCommand none
 ForwardAgent no
 ClearAllForwardings yes
 LogLevel ERROR
''')
    with (base/'sshd.log').open('wb')as log:
        server=subprocess.Popen(['/usr/bin/sshd','-D','-e','-f',str(config)],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=log,start_new_session=True)
    remember();end=time.monotonic()+5
    while True:
        expect(server.poll()is None,'private sshd exited: '+(base/'sshd.log').read_text(errors='replace')[:1024])
        try:
            with socket.create_connection(('127.0.0.1',port),timeout=.2):pass
            break
        except OSError:
            expect(time.monotonic()<end,'private sshd listener not ready');time.sleep(.02)
    if args.connection_engine:
        point_known=False
        engine_args=[str(ROOT/'target/debug/examples/sftp-connection-probe'),'--attempt',str(base),str(args.sshfs),'/usr/bin/fusermount3',marker,str(source),str(client_config)]
        if args.engine_registry:engine_args.append('--registry')
        client=subprocess.Popen(engine_args,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        remember();engine=guard_reply(child=client);point=Path(os.fsdecode(bytes(engine['point'])));mount_marker=point.name
        expect(point.parent==base,'core engine point outside fixture');point_known=True
        report['connection_engine']={'initial':engine}
    else:
        planned=run([str(ROOT/'target/debug/examples/sftp-target-plan'),'--plan',marker,'','',str(source),str(point)])
        expect(planned.returncode==0,'core Target argv plan failed');plan=json.loads(planned.stdout)
        argv=[os.fsdecode(bytes(v))for v in plan['argv']]
        # -F is an explicit LAB-ONLY authentication fixture override, not a product option.
        argv=['-F',str(client_config),*argv,'-o','fsname='+marker]
        with (base/'sshfs.log').open('wb')as log:
            client=subprocess.Popen([str(args.sshfs),*argv],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=log,start_new_session=True)
    end=time.monotonic()+12
    while True:
        remember()
        if args.connection_engine:
            engine=guard_reply('poll',client)
            if args.engine_bad_hostkey:
                expect(engine['phase']!='Ready','wrong hostkey was published Ready')
                if engine['phase']in ('Absent','Uncertain','Retained')and('host key'in engine['diagnostic'].lower()or'identification has changed'in engine['diagnostic'].lower()):
                    rejected=engine;report['connection_engine']['auth_rejection_observed']=rejected
                    if probe()['present']:
                        request=guard_reply('disconnect',client);report['connection_engine']['auth_disconnect_attempt']=request
                        if request['request_error']is not None:
                            absent_race=request['request_error']=='Montaje ausente; reconciliar antes de desconectar'and not probe()['present']
                            report['connection_engine']['auto_absence_verified_after_request']=absent_race
                            expect(absent_race,'cannot explicitly disconnect rejected FUSE mount: '+repr(request))
                    cleanup_deadline=time.monotonic()+5
                    while True:
                        remember();engine=guard_reply('poll',client)
                        if engine['phase']=='Absent'and engine['server_pid']is None and engine['unmount_pid']is None:break
                        expect(time.monotonic()<cleanup_deadline,'rejected connection not reconciled after explicit disconnect');time.sleep(.02)
                    expect(not probe()['present'],'rejected authentication mount remains')
                    report['connection_engine']['bad_hostkey']={'gate':'PASS','rejection':rejected,'after_disconnect':engine,'knownhost_bypass':False,'no_ready_published':True}
                    guard_reply('exit',client);client.wait(timeout=3);expect(client.returncode==0,'rejected connection owner exit failed')
                    report['mount_gate']='NOT_APPLICABLE';report['scope']='Expected private mismatched hostkey rejection, no serving mount accepted'
                    raise ExpectedHostKeyRejection()
            if not args.engine_bad_hostkey:expect(engine['phase']not in ('Absent','Uncertain'),'core connection failed before Ready: '+repr(engine))
        mounted=probe()
        if mounted['present']and(not args.connection_engine or engine['phase']=='Ready'):break
        expect(client.poll()is None,'SSHFS/driver exited before mount')
        expect(time.monotonic()<end,'mount not verified by deadline');time.sleep(.03)
    if args.connection_engine:report['connection_engine']['ready']=engine
    report['mounted_identity']=mounted;report['sshfs_identity']=ident(engine['server_pid'] if args.connection_engine else client.pid)[:2];report['sshd_identity']=ident(server.pid)[:2]
    expect(client.poll()is None,'foreground SSHFS must remain live')
    expect((point/'alpha.txt').read_bytes()==(source/'alpha.txt').read_bytes(),'mounted regular content differs')
    with open(os.fsencode(point)+b'/'+raw,'rb')as f:expect(f.read()==bytes(range(256)),'mounted raw filename differs')
    expect(os.readlink(point/'relative-link')=='alpha.txt','symlink literal changed')
    external=run(['/usr/bin/cat',str(point/'alpha.txt')])
    expect(external.returncode==0 and external.stdout==(source/'alpha.txt').read_bytes(),'independent process cannot read native mount')
    (point/'created-through-mount.txt').write_bytes(b'written through real SFTP mount')
    expect((source/'created-through-mount.txt').read_bytes()==b'written through real SFTP mount','mounted write did not reach server')
    rename_source=point/'rename-source';rename_target=point/'rename-target'
    rename_source.write_bytes(b'owned no-replace probe')
    rename=libc.renameat2
    rename.argtypes=[ctypes.c_int,ctypes.c_char_p,ctypes.c_int,ctypes.c_char_p,ctypes.c_uint];rename.restype=ctypes.c_int
    ctypes.set_errno(0)
    renamed=rename(-100,os.fsencode(rename_source),-100,os.fsencode(rename_target),1)
    rename_errno=ctypes.get_errno()if renamed!=0 else None
    report['rename_noreplace']={'return':renamed,'errno':rename_errno}
    if renamed==0:
        expect(rename_target.read_bytes()==b'owned no-replace probe'and not rename_source.exists(),'successful native rename result differs')
        report['rename_noreplace_gate']='PASS'
    else:
        expect(rename_source.read_bytes()==b'owned no-replace probe'and not rename_target.exists(),'rejected native rename changed names/data')
        report['rename_noreplace_gate']='UNSUPPORTED'if rename_errno in [22,38,95]else'FAIL'
    report['exclusive_link_probes']=[]
    for name,symlink in [('file',False),('symlink',True)]:
        old=point/('link-probe-'+name);new=point/('link-result-'+name);occupied=point/('link-occupied-'+name)
        if symlink:old.symlink_to('alpha.txt')
        else:old.write_bytes(b'owned exclusive link payload')
        occupied.write_bytes(b'existing actor data')
        record={'kind':name}
        try:
            os.link(old,new,follow_symlinks=False)
            left=old.lstat();right=new.lstat()
            record.update(success=True,same_inode=(left.st_dev,left.st_ino)==(right.st_dev,right.st_ino),source_inode=left.st_ino,destination_inode=right.st_ino)
            if symlink:expect(new.is_symlink()and os.readlink(new)=='alpha.txt','exclusive symlink payload changed')
            else:expect(new.read_bytes()==b'owned exclusive link payload','exclusive regular payload changed')
            try:os.link(old,occupied,follow_symlinks=False);record['collision_rejected']=False
            except FileExistsError:record['collision_rejected']=True
            expect(record['collision_rejected']and occupied.read_bytes()==b'existing actor data','exclusive link replaced occupant')
        except OSError as error:record.update(success=False,errno=error.errno,destination_present=new.exists()or new.is_symlink())
        report['exclusive_link_probes'].append(record)
    local=base/'copy-input';local.write_bytes(b'core copy over real SSHFS')
    copied=run([str(ROOT/'target/debug/examples/copy-once'),'--attempt',str(local),str(point/'core-copied')],15)
    expect(copied.returncode==0,'core copy probe process failed')
    outcome=json.loads(copied.stdout);report['core_copy']=outcome
    if outcome['ok']:
        expect((source/'core-copied').read_bytes()==local.read_bytes(),'core copied content differs')
        report['copy_engine_gate']='PASS'
    else:
        expect(outcome['completed']==0 and not (point/'core-copied').exists(),'failed core copy published unexpectedly')
        expect(not outcome['cleanup_errors'] and not outcome['rescue_paths'],'core copy cleanup unresolved')
        expect(not any(p.name.startswith('.files-copy-')for p in point.iterdir()),'core copy staging remains')
        report['copy_engine_gate']='FAIL'
    expect(local.read_bytes()==b'core copy over real SSHFS','core source changed')
    # Explicit adapter; the default staged-copy failure remains a separate gate.
    tree=base/'visible-input';(tree/'nested').mkdir(parents=True)
    (tree/'nested/data').write_bytes(b'visible exclusive transfer')
    (tree/'link').symlink_to('nested/data')
    with open(os.fsencode(tree)+b'/raw-\xff','wb')as stream:stream.write(b'raw payload')
    adapter=run([str(ROOT/'target/debug/examples/copy-once'),'--attempt','--visible-omit-symlink-times',str(tree),str(point/'visible-copy')],15)
    expect(adapter.returncode==0,'visible adapter process failed')
    outcome=json.loads(adapter.stdout);report['visible_copy']=outcome
    expect(outcome['ok'],'explicit visible adapter failed: '+repr(outcome))
    expect(outcome['visible_started']==1 and outcome['visible_completed']==1 and outcome['visible_uncertain']==0,'visible root accounting mismatch')
    expect(outcome['symlink_times_omitted']==1,'symlink metadata omission not reported')
    expect((source/'visible-copy/nested/data').read_bytes()==b'visible exclusive transfer','visible file bytes differ')
    expect(os.readlink(source/'visible-copy/link')=='nested/data','literal symlink differs')
    with open(os.fsencode(source/'visible-copy')+b'/raw-\xff','rb')as stream:expect(stream.read()==b'raw payload','raw file differs')
    expect((tree/'nested/data').read_bytes()==b'visible exclusive transfer','visible source mutated')
    repeated=run([str(ROOT/'target/debug/examples/copy-once'),'--attempt','--visible-omit-symlink-times',str(local),str(point/'visible-copy/nested/data')],15)
    collision=json.loads(repeated.stdout);report['visible_collision']=collision
    expect(not collision['ok'] and collision['completed']==0,'visible copy overwrote occupied file')
    expect((source/'visible-copy/nested/data').read_bytes()==b'visible exclusive transfer','occupied file changed')
    expect(not any(p.name.startswith('.files-copy-')for p in point.iterdir()),'visible adapter left staging')
    report['visible_adapter_gate']='PASS'
    if args.connection_engine and args.engine_close:
        owned_server=report['sshfs_identity']
        if args.engine_registry:
            registry_before=sha(base/'connection.bin')
            inspected=run([str(ROOT/'target/debug/examples/sftp-registry-probe'),'--inspect',str(base)])
            expect(inspected.returncode==0,'registry reader failed while owner alive')
            readonly=json.loads(inspected.stdout);report['registry_recovery']={'owner_alive':readonly}
            expect(not readonly['writable']and readonly['phase']=='Verified'and readonly['recovery']=='MatchingMountAndProcess','other window did not retain read-only verified observation')
            expect(sha(base/'connection.bin')==registry_before,'read-only probe changed registry')
        if args.recovered_control:
            readonly_control=subprocess.Popen([str(ROOT/'target/debug/examples/sftp-recovered-probe'),'--attempt',str(base),'/usr/bin/fusermount3'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            remember();read_state=guard_reply(child=readonly_control)
            expect(not read_state['writable'],'second recovered owner unexpectedly writable')
            rejected=guard_reply('disconnect',readonly_control)
            expect(rejected['result'].startswith('Err(')and rejected['pending_pid']is None,'readonly recovered control dispatched child')
            expect(probe()==mounted and sha(base/'connection.bin')==registry_before,'readonly attempt mutated namespace/record')
            report['recovered_readonly']={'initial':read_state,'rejected':rejected,'gate':'PASS'}
            guard_reply('exit',readonly_control);readonly_control.wait(timeout=3);expect(readonly_control.returncode==0,'readonly recovered exit failed')
        guard_reply('exit',client);client.wait(timeout=3);expect(client.returncode==0,'owner exit failed')
        current=ident(owned_server[0]);expect(current is not None and list(current[:2])==list(owned_server)and current[2]!='Z','SSHFS did not survive owner exit')
        expect(probe()==mounted,'mount changed after owner exit')
        if args.engine_registry:
            inspected=run([str(ROOT/'target/debug/examples/sftp-registry-probe'),'--inspect',str(base)])
            expect(inspected.returncode==0,'fresh process failed reading retained registry')
            recovered=json.loads(inspected.stdout);report['registry_recovery']['owner_gone']=recovered
            expect(recovered['writable']and recovered['phase']=='Verified'and recovered['recovery']=='MatchingMountAndProcess','registry did not recover exact mount+live process after owner exit')
            expect((recovered['server'][0],str(recovered['server'][1]))==tuple(owned_server),'restored PID/start differs')
            expect(sha(base/'connection.bin')==registry_before,'recovery observation mutated registry')
        readback=run(['/usr/bin/cat',str(point/'alpha.txt')])
        expect(readback.returncode==0 and readback.stdout==(source/'alpha.txt').read_bytes(),'mount not serving after owner exit')
        fresh_payload=b'created on server after engine owner exited'
        (source/'after-owner-exit-new-file').write_bytes(fresh_payload)
        fresh_read=run(['/usr/bin/cat',str(point/'after-owner-exit-new-file')])
        expect(fresh_read.returncode==0 and fresh_read.stdout==fresh_payload,'fresh remote lookup failed after owner exit')
        report['connection_engine']['owner_exit_serving']={'gate':'PASS','identity':list(current),'mount_unchanged':True,'external_read_exact':True,'fresh_server_file_read_exact':True,'not_GUI_close':True}
        if args.ui_control:
            import importlib.util
            spec=importlib.util.spec_from_file_location('sftp_ui_control',ROOT/'scripts/sftp-ui-control.py');ui=importlib.util.module_from_spec(spec);spec.loader.exec_module(ui)
            report['ui_control']=ui.recovered_ui(base,source,point,owned_server[0],probe,remember,expect,OUT,args.sshfs,client_config,marker,port,args.ui_copy)
            inspected=run([str(ROOT/'target/debug/examples/sftp-registry-probe'),'--inspect',str(base)])
            stored=json.loads(inspected.stdout)
            expect(stored['phase']=='Absent'and stored['server']is None,'GUI disconnect not durably closed')
            report['ui_control']['stored']=stored
            report['connection_engine']['cleanup']='Product UI explicit ordinary unmount; LAB reaped original adopted SSHFS'
        elif args.recovered_control:
            recovered_control=subprocess.Popen([str(ROOT/'target/debug/examples/sftp-recovered-probe'),'--attempt',str(base),'/usr/bin/fusermount3'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            remember();initial=guard_reply(child=recovered_control)
            report['recovered_control']={'initial':initial}
            expect(initial['phase']=='Registered'and initial['writable'],'recovered controller did not verify registered mount')
            expect(sha(base/'connection.bin')==registry_before,'opening recovered controller mutated registry')
            busy=os.open(point/'alpha.txt',os.O_RDONLY|os.O_CLOEXEC)
            requested=guard_reply('disconnect',recovered_control);report['recovered_control']['busy_requested']=requested
            expect(requested['result'].startswith('Ok(Started'),'recovered unmount not admitted')
            deadline=time.monotonic()+5
            while True:
                remember();state=guard_reply('poll',recovered_control)
                if state['pending_pid']is None:break
                expect(time.monotonic()<deadline,'recovered busy child ACK timeout');time.sleep(.02)
            report['recovered_control']['busy']=state
            expect(state['phase']=='Retained'and state['saved_phase']=='Uncertain'and bool(state['diagnostic']),'busy result not retained and recorded')
            expect(probe()==mounted,'busy recovered unmount changed identity')
            os.close(busy);busy=None
            requested=guard_reply('disconnect',recovered_control);report['recovered_control']['retry_requested']=requested
            expect(requested['result'].startswith('Ok(Started'),'recovered retry not admitted')
            deadline=time.monotonic()+5;server_reaped=False
            while True:
                if not server_reaped:
                    pid,status=os.waitpid(owned_server[0],os.WNOHANG)
                    if pid:expect(os.waitstatus_to_exitcode(status)==0,'adopted SSHFS failed');server_reaped=True
                remember();state=guard_reply('poll',recovered_control)
                if state['phase']=='Absent':break
                expect(time.monotonic()<deadline,'recovered disconnect did not reconcile absence/reap');time.sleep(.02)
            expect(server_reaped and state['pending_pid']is None and not probe()['present'],'Absent before process cleanup/mount absence')
            report['recovered_control']['complete']=state
            guard_reply('exit',recovered_control);recovered_control.wait(timeout=3);expect(recovered_control.returncode==0,'recovered controller exit failed')
            inspected=run([str(ROOT/'target/debug/examples/sftp-registry-probe'),'--inspect',str(base)])
            expect(inspected.returncode==0,'post recovered-disconnect registry read failed');stored=json.loads(inspected.stdout)
            expect(stored['phase']=='Absent'and stored['server']is None and stored['generation']>recovered['generation'],'recovered completion was not durable')
            report['recovered_control']['stored']=stored;report['recovered_control']['gate']='PASS'
            report['connection_engine']['cleanup']='Explicit recovered controller ordinary unmount; LAB reaped adopted SSHFS only'
        else:
            busy=os.open(point/'alpha.txt',os.O_RDONLY|os.O_CLOEXEC)
            denied=run(['/usr/bin/fusermount3','-u',str(point)],5);expect(denied.returncode!=0 and probe()==mounted,'busy lab unmount did not preserve identity')
            os.close(busy);busy=None
            released=run(['/usr/bin/fusermount3','-u',str(point)],5);expect(released.returncode==0 and not probe()['present'],'lab ordinary unmount unconfirmed')
            deadline=time.monotonic()+5
            while True:
                pid,status=os.waitpid(owned_server[0],os.WNOHANG)
                if pid:
                    expect(os.waitstatus_to_exitcode(status)==0,'adopted SSHFS exit nonzero');break
                expect(time.monotonic()<deadline,'adopted SSHFS did not exit');time.sleep(.02)
            report['connection_engine']['cleanup']='LAB ordinary unmount and adopted SSHFS exit0; not engine restart/recovery'
            if args.engine_registry:
                inspected=run([str(ROOT/'target/debug/examples/sftp-registry-probe'),'--inspect',str(base)])
                expect(inspected.returncode==0,'post-unmount registry inspection failed')
                absent=json.loads(inspected.stdout);report['registry_recovery']['after_lab_unmount']=absent
                expect(absent['phase']=='Verified'and absent['recovery'].startswith('NoMount'),'unconfirmed historical phase was silently cleared or current absence not observed')
                expect(sha(base/'connection.bin')==registry_before,'inspection automatically cleared durable record')
                report['registry_recovery']['gate']='PASS'
        report['connection_engine']['gate']='PASS'
    elif args.connection_engine:
        report['connection_engine']['cancel_ready']=guard_reply('cancel',client)
        expect(report['connection_engine']['cancel_ready']['phase']=='Retained','cancelled verified mount was not retained')
        expect(probe()==mounted,'cancel silently removed verified mount')
        busy=os.open(point/'alpha.txt',os.O_RDONLY|os.O_CLOEXEC)
        requested=guard_reply('disconnect',client);expect(requested['request_error']is None,'busy unmount request rejected before execution')
        end=time.monotonic()+5
        while True:
            remember();state=guard_reply('poll',client)
            if state['unmount_pid']is None:break
            expect(time.monotonic()<end,'busy unmount process not acknowledged');time.sleep(.02)
        expect(state['phase']=='Retained'and bool(state['diagnostic']),'busy unmount did not retain visible diagnostic')
        expect(probe()==mounted,'busy core unmount changed identity')
        report['connection_engine']['busy']=state
        os.close(busy);busy=None
        requested=guard_reply('disconnect',client);expect(requested['request_error']is None,'retry unmount request failed')
        end=time.monotonic()+5
        while True:
            remember();state=guard_reply('poll',client)
            if state['phase']=='Absent':break
            expect(time.monotonic()<end,'core disconnect did not confirm absence');time.sleep(.02)
        expect(state['server_pid']is None and state['unmount_pid']is None,'Absent before both processes reaped')
        expect(not probe()['present'],'native mount remains after core absence')
        report['connection_engine']['after_disconnect']=state;report['connection_engine']['gate']='PASS'
        guard_reply('exit',client);client.wait(timeout=3);expect(client.returncode==0,'connection driver exit failed')
    else:
        report['point_provenance']['mounted_parent']=guard_reply('parent')
        covered=guard_reply('empty');report['point_provenance']['mounted_empty_check']=covered
        expect(report['point_provenance']['mounted_parent']['ok'],'local parent verification failed while mounted')
        expect(not covered['ok']and 'no inspeccionar'in covered['error'],'core inspected mounted root instead of rejecting coverage')
        busy=os.open(point/'alpha.txt',os.O_RDONLY|os.O_CLOEXEC)
        denied=run(['/usr/bin/fusermount3','-u',str(point)],5)
        expect(denied.returncode!=0,'ordinary unmount unexpectedly succeeded with held file')
        expect(probe()==mounted,'busy unmount changed mounted identity')
        report['busy_unmount']={'status':denied.returncode,'message':denied.stderr.decode(errors='replace')[:1024]}
        os.close(busy);busy=None
        p=run(['/usr/bin/fusermount3','-u',str(point)],5);expect(p.returncode==0,'ordinary unmount failed: '+p.stderr.decode(errors='replace')[:1024])
        expect(not probe()['present'],'mount still present after successful unmount')
        client.wait(timeout=5);expect(client.returncode==0,'foreground SSHFS exit not0 after unmount')
        report['point_provenance']['after_unmount']=guard_reply('empty')
        expect(report['point_provenance']['after_unmount']['ok'],'underlying point identity/emptiness not verified after unmount')
        expect(guard_reply('exit')['ok'],'point guard exit rejected');point_guard.wait(timeout=3)
        expect(point_guard.returncode==0,'point guard exit failed')
        report['point_provenance']['gate']='PASS'
    report.update(mount_gate='PASS',scope='actual kernel FUSE mount verified by core strict mountinfo identity, OpenSSH internal-sftp with ephemeral fixture keys/knownhost and no real user SSH config, core Target source literalUnicode/percent/$() path, raw0xFF read, symlink literal, independent cat read and mounted write, busy ordinary unmount rejected/identity intact, unmount→absent and SSHFS0; no UI/input/performance/whole-lifetime/hostile-mount/powerloss/global acceptance')
except ExpectedHostKeyRejection:
    pass
except BaseException as e:
    report.update(mount_gate='FAIL',error=repr(e))
finally:
    if busy is not None:os.close(busy)
    remember()
    try:
        expect(point_known,'engine point identity not received; cannot establish absence at a guessed path')
        state=probe()
        if state['present']:
            if mounted is not None:expect(state==mounted,'cleanup mount identity changed')
            un=run(['/usr/bin/fusermount3','-u',str(point)],5)
            expect(un.returncode==0 and not probe()['present'],'cleanup ordinary unmount unconfirmed')
        report['mount_cleanup']='ABSENT'
    except BaseException as e:report['mount_cleanup']='UNVALIDATED';report['cleanup_error']=repr(e)
    for p in [readonly_control,recovered_control,client,server,point_guard]:
        if p is not None and p.poll()is None:
            p.terminate()
            try:p.wait(timeout=3)
            except subprocess.TimeoutExpired:
                p.kill()
                try:p.wait(timeout=3)
                except subprocess.TimeoutExpired:report.setdefault('process_cleanup_errors',[]).append({'pid':p.pid,'error':'still not reaped after kill deadline'})
    # Reap only our private harness's adopted processes; never global pkill/group matching.
    deadline=time.monotonic()+3
    while time.monotonic()<deadline:
        remember()
        while True:
            try:
                pid,status=os.waitpid(-1,os.WNOHANG)
                if pid==0:break
            except ChildProcessError:break
        alive=[]
        for pid,start in identities:
            st=ident(pid)
            if st is not None and st[1]==start:
                alive.append(st)
                if st[2]!='Z':
                    try:
                        fd=os.pidfd_open(pid)
                        try:
                            current=ident(pid)
                            if current is not None and current[1]==start:signal.pidfd_send_signal(fd,signal.SIGTERM)
                        finally:os.close(fd)
                    except ProcessLookupError:pass
                    except OSError as error:identity_errors.append({'pid':pid,'errno':error.errno,'kind':'pidfd cleanup unavailable'})
        if not alive:break
        time.sleep(.02)
    report['logged_process_identities']=[list(v)for v in sorted(identities)]
    survivors=[]
    for pid,start in identities:
        current=ident(pid)
        if current is not None and (current[1] is None or current[1]==start):survivors.append(list(current))
    report['identity_errors']=identity_errors
    report['observed_survivors']=survivors
    if survivors or identity_errors or report.get('process_cleanup_errors') or report.get('mount_cleanup')!='ABSENT':report['mount_gate']='UNVALIDATED'
    report['overall_gate']='PASS'if report['mount_gate']=='PASS'and report.get('copy_engine_gate','PASS')=='PASS'else('UNVALIDATED'if report['mount_gate']=='UNVALIDATED'else'FAIL')
    report['sshfs_binary']={'path':str(args.sshfs),'sha256':sha(args.sshfs)}
    if args.connection_engine:
        report['connection_engine_binary_sha256']=sha(ROOT/'target/debug/examples/sftp-connection-probe')
        if args.engine_registry:report['registry_probe_binary_sha256']=sha(ROOT/'target/debug/examples/sftp-registry-probe')
        if args.recovered_control:report['recovered_control_binary_sha256']=sha(ROOT/'target/debug/examples/sftp-recovered-probe')
    else:report['point_guard_binary_sha256']=sha(ROOT/'target/debug/examples/mount-point-probe')
    report['logs']={n:(base/n).read_text(errors='replace')[-8192:]for n in ['sshd.log','sshfs.log']if (base/n).exists()}
    report['fixture_retained']='Owned0700 fixture retained; no recursive deletion around mountpoints; test-only keys not printed or installed'
    path=OUT/('sftp-loopback-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
raise SystemExit(0 if report['overall_gate']=='PASS' else 1)
