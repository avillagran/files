#!/usr/bin/env python3
"""Per-tab browser state, real framebuffer and normal close; no performance sampling."""
import hashlib,json,os,re,subprocess,tempfile,time
from pathlib import Path
from memcheck import geometry,observed_geometry,stop_tree
ROOT=Path(__file__).resolve().parent.parent
BIN_DIR=Path.home()/'.local/bin'if '--installed'in __import__('sys').argv else ROOT/'target/release'
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
OUT=ROOT/'docs/evidence/0.0.4'
def session_check():
    """Extend the existing tabs fixture with durable-session behavior, never metrics."""
    result={'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'states':[],'runs':[],
            'method':'shared Actions and existing pointer handlers/framebuffer; no native input, clipboard or resource sampling'}
    opened=[]
    temporary=None
    class Window:
        def __init__(self,base,profile,path,label,no_session=False):
            self.raw=base/(label+'.ppm');self.log=base/(label+'.log');self.seq=0;self.label=label;self.closed=False
            private=base/(label+'-tmp');private.mkdir()
            self.env={**os.environ,'TMPDIR':str(private),'MAGICK_TEMPORARY_PATH':str(private),'FILES_CONFIG_DIR':str(profile),
                      'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_TABS':'1','FILES_TEST_SESSION':'1',
                      'FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1'}
            argv=[str(BIN_DIR/'files')]+([str(path)] if path else [])+['--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','90']+(['--no-session'] if no_session else [])
            with self.log.open('w') as out:self.process=subprocess.Popen(argv,env=self.env,stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
            opened.append(self)
            self.wait(lambda:'FILES_UI_SMOKE_OK' in self.log.read_text(),'smoke')
            self.state(condition=lambda s:s['width']==1000 and s['height']==650)
        def wait(self,check,why,allow_exit=False):
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                value=check()
                if value:return value
                if self.process.poll() is not None and not allow_exit:raise AssertionError(self.label+': '+why+' exit='+str(self.process.returncode))
                time.sleep(.02)
            raise AssertionError(self.label+': '+why)
        def state(self,after=0,condition=lambda s:True):
            def check():
                for line in reversed(self.log.read_text()[after:].splitlines(keepends=True)):
                    if not line.endswith('\n'):continue
                    if line.startswith('FILES_HIDDEN_STATE '):
                        s=json.loads(line[len('FILES_HIDDEN_STATE '):])
                        if s['seq']==self.seq and not s['config_dirty'] and condition(s):return s
                return False
            state=self.wait(check,'fresh state seq='+str(self.seq));result['states'].append({'run':self.label,**state});return state
        def command(self,action):
            after=len(self.log.read_text());pending=self.raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,self.raw.with_suffix('.command'))
            match=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',self.log.read_text()[after:],re.M),'command '+action,action=='tab-session-close')
            self.seq=int(match[1]);return after
        def act(self,action,condition=lambda s:True):return self.state(self.command(action),condition)
        def capture(self,name):
            self.raw.unlink(missing_ok=True);self.raw.with_suffix('.request').touch()
            def full():
                if not self.raw.exists():return False
                parts=self.raw.read_bytes().split(b'\n',3)
                return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
            self.wait(full,'framebuffer');p=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(self.raw),str(p)],env=self.env,check=True,timeout=10);result['captures'].append(str(p))
        def close(self):
            after=self.command('tab-session-close');self.wait(lambda:self.process.poll() is not None,'normal close',True)
            text=self.log.read_text();assert self.process.returncode==0 and 'FILES_SPACE_DRAINED' in text[after:] and 'FILES_CLOSE_DRAINED' in text[after:]
            self.closed=True;result['runs'].append({'label':self.label,'normal_exit0_after_ack':True,'log':text})
    try:
        instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')];assert len(matches)==1;os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance']
        temporary=tempfile.TemporaryDirectory(prefix='tabs-session-',dir=BASE)
        tmp=temporary.name
        base=Path(tmp);a=base/'A';b=base/'B';profile=base/'config';a.mkdir();b.mkdir();profile.mkdir()
        for folder in [a,b]:
            for name in ['keep.txt','keeper.txt','same.txt','.hidden']+[f'keep-{i:02}.txt' for i in range(60)]: (folder/name).write_text(folder.name+' content\n')
            (folder/'folder').mkdir();(folder/os.fsdecode(b'raw-\xff.txt')).write_text(folder.name+' raw\n')
        hx=lambda name:name.encode().hex()
        first=Window(base,profile,a,'save-explicit');assert first.state()['session_mode']=='off'
        first.act('tab-config-a');first.act('tab-new');first.act('tab-b-location');first.act('tab-config-b')
        first.act('tab-new');first.act('tab-a-location');first.act('tab-raw-select');first.act('tab-close');first.act('tab-a')
        first.act('tab-session-preferences');first.act('tab-session-save',lambda s:s['session_mode']=='writable' and not s['session_pending'] and not s['session_in_flight'] and s['session_error'] is None)
        first.act('tab-session-preferences');first.capture('session-saved-two-tabs');first.close();session=profile/'session.bin';saved=session.read_bytes();assert saved.startswith(b'files-session\0');result['saved_bytes']=len(saved)
        restored=Window(base,profile,None,'restored');sa=restored.state(condition=lambda s:s['session_mode']=='writable' and len(s['tabs'])==2)
        assert sa['path']==str(a) and sa['query']=='keep' and sa['view']=='grid' and sa['scroll']==120 and sa['grid_zoom']==1.25 and sa['pan_x']==48 and sa['descending'] and sa['anchor']==hx('keep.txt') and sa['selected']==hx('keeper.txt') and len(sa['selection'])==2 and sa['tab_closed']==1
        restored.capture('session-restored-A');sb=restored.act('tab-b');assert sb['path']==str(b) and sb['query']=='same' and sb['view']=='list' and sb['history_back'] and sb['selection']==[hx('same.txt')]
        restored.capture('session-restored-B-history');raw=restored.act('tab-reopen');assert raw['selected']==b'raw-\xff.txt'.hex();restored.act('tab-close');restored.act('tab-a');restored.close()
        prior=session.read_bytes();explicit=Window(base,profile,b,'explicit-no-autosave');se=explicit.state();assert se['path']==str(b) and len(se['tabs'])==1 and se['session_mode']=='off';explicit.close();assert session.read_bytes()==prior
        disabled=Window(base,profile,a,'disabled',True);assert disabled.state()['session_mode']=='disabled';disabled.act('tab-session-preferences');disabled.act('tab-session-save');disabled.close();assert session.read_bytes()==prior
        owner=Window(base,profile,None,'owner');owner.state(condition=lambda s:s['session_mode']=='writable' and len(s['tabs'])==2);owner.act('tab-b')
        other=Window(base,profile,a,'other-window');other.state();other.act('tab-session-preferences');ro=other.act('tab-session-save',lambda s:s['session_mode']=='readonly');assert ro['session_notice'] and not ro['session_pending'];other.act('tab-session-preferences');other.capture('session-other-window-readonly')
        owner.close();newer=session.read_bytes();assert newer!=prior;other.act('tab-session-state',lambda s:s['session_mode']=='readonly');other.close();assert session.read_bytes()==newer
        for label,broken in [('invalid',b'invalid-file-kept'),('future',newer[:len(b'files-session\0')]+(99).to_bytes(4,'little')+newer[len(b'files-session\0')+4:])]:
            session.write_bytes(broken);bad=Window(base,profile,a,label);bad.state();bad.act('tab-session-preferences');state=bad.act('tab-session-save',lambda s:s['session_mode']=='readonly');assert state['session_notice'] and session.read_bytes()==broken;bad.act('tab-session-preferences');bad.capture('session-'+label+'-preserved');bad.close();assert session.read_bytes()==broken
        assert (a/'same.txt').read_text()=='A content\n' and (b/'same.txt').read_text()=='B content\n'
        result.update(functional_gate='PASS',serialized_save_restore=True,argv_explicit_no_restore_no_autosave=True,no_session_disabled=True,multiwindow_no_takeover_no_overwrite=True,invalid_future_preserved=True,raw_selection_closed_tab_restored=True,resource_gate='NOT_MEASURED')
    except BaseException as error:
        result['error']=repr(error)
        for w in opened:
            if not w.closed:result['runs'].append({'label':w.label,'normal_exit0_after_ack':False,'log':w.log.read_text()})
        raise
    finally:
        for w in opened:
            if not w.closed:stop_tree(w.process,None)
        if temporary is not None:temporary.cleanup()
        for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:result[key]=hashlib.sha256((BIN_DIR/name).read_bytes()).hexdigest()
        path=OUT/('tabs-session-ui-'+str(time.time_ns())+'.json')
        with path.open('x') as out:json.dump(result,out,indent=2)
        print(json.dumps({'report':str(path),'functional_gate':result['functional_gate']}))

def columns_check():
    """Extend the tabs scene: Miller chain + captured operation source, no metrics."""
    report={'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'capture_dimensions':{},'states':[],'runs':[],
            'method':'real FLTK framebuffer; visible selector/rows/confirm use pointer handlers; left/right/rapid/filter use shared Model methods, no native input or clipboard',
            'scope':'Miller chain + sibling pruning + copy bytes + operation payloads + initial narrow instance; >8 depth/raw/modifiers/migration additionally unit-tested; no live resize or whole-chain durable restore claimed'}
    opened=[];temporary=None
    class Window:
        def __init__(self,base,root,label,size):
            self.raw=base/(label+'.ppm');self.log=base/(label+'.log');self.seq=0;self.label=label;self.closed=False;self.size=size
            private=base/(label+'-tmp');private.mkdir()
            self.env={**os.environ,'TMPDIR':str(private),'MAGICK_TEMPORARY_PATH':str(private),'FILES_CONFIG_DIR':str(base/(label+'-config')),
                      'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_TABS':'1','FILES_TEST_ISOLATE_INPUT':'1',
                      'FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1','FILES_TEST_COLUMNS_DEST':str(base/(label+'-destination'))}
            Path(self.env['FILES_TEST_COLUMNS_DEST']).mkdir()
            with self.log.open('w') as out:self.process=subprocess.Popen([str(BIN_DIR/'files'),str(root),'--ui-smoke','--ui-scene','hidden-context','--size',f'{size[0]}x{size[1]}','--scale','1','--exit-after','90'],env=self.env,stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
            opened.append(self)
            self.wait(lambda:'FILES_UI_SMOKE_OK' in self.log.read_text(),'smoke')
            self.wait(lambda:geometry(self.process.pid,*size) and observed_geometry(self.process.pid,*size),'geometry')
        def wait(self,check,why,allow_exit=False):
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                value=check()
                if value:return value
                if self.process.poll() is not None and not allow_exit:raise AssertionError(self.label+': '+why+' exit='+str(self.process.returncode))
                time.sleep(.02)
            raise AssertionError(self.label+': '+why)
        def state(self,after=0,condition=lambda s:True):
            def check():
                for line in reversed(self.log.read_text()[after:].splitlines()):
                    if line.startswith('FILES_HIDDEN_STATE '):
                        s=json.loads(line[len('FILES_HIDDEN_STATE '):])
                        if s['seq']==self.seq and condition(s):return s
                return False
            s=self.wait(check,'fresh state seq='+str(self.seq));report['states'].append({'run':self.label,**s});return s
        def command(self,action):
            after=len(self.log.read_text());pending=self.raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,self.raw.with_suffix('.command'))
            match=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',self.log.read_text()[after:],re.M),'command '+action,action=='columns-close');self.seq=int(match[1]);return after
        def act(self,action,condition=lambda s:True):return self.state(self.command(action),condition)
        def capture(self,name):
            self.raw.unlink(missing_ok=True);self.raw.with_suffix('.request').touch()
            def full():
                if not self.raw.exists():return False
                parts=self.raw.read_bytes().split(b'\n',3)
                if len(parts)!=4:return False
                dims=tuple(map(int,parts[1].split()))
                if len(parts[3])!=dims[0]*dims[1]*3:return False
                assert dims==self.size,('unexpected framebuffer dimensions',dims,self.size)
                return True
            self.wait(full,'framebuffer');p=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(self.raw),str(p)],env=self.env,check=True,timeout=10);report['captures'].append(str(p));report['capture_dimensions'][str(p)]=self.size
        def close(self):
            after=self.command('columns-close');self.wait(lambda:self.process.poll() is not None,'normal close',True)
            text=self.log.read_text();assert self.process.returncode==0 and 'FILES_SPACE_DRAINED' in text[after:] and 'FILES_CLOSE_DRAINED' in text[after:]
            self.closed=True;report['runs'].append({'label':self.label,'normal_exit0_after_ack':True,'log':text})
    try:
        instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')];assert len(matches)==1;os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance']
        temporary=tempfile.TemporaryDirectory(prefix='columns-ui-',dir=BASE);base=Path(temporary.name);root=base/'root';root.mkdir()
        for name in ['A','B']:
            folder=root/name;(folder/'nested').mkdir(parents=True)
            for target in [folder,folder/'nested']:
                for file in ['same.txt','last.txt','.hidden']:(target/file).write_text(name+' '+target.name+' body\n')
            (folder/os.fsdecode(b'raw-\xff')).write_text(name+' raw\n')
        expected={p:p.read_bytes() for p in root.rglob('*') if p.is_file()};hx=lambda s:s.encode().hex()
        ready=lambda s:bool(s['columns']['rows']) and not s['error'] and all(c['loaded'] and not c['error'] for c in s['columns']['rows'])
        w=Window(base,root,'main',(1000,650));w.act('columns-view',lambda s:s['view']=='columns' and ready(s));w.act('columns-select:A',ready)
        first=w.state(condition=ready);assert first['path']==str(root) and first['selected']==hx('A') and len(first['columns']['rows'])==2
        w.capture('columns-parent-selected-child-visible')
        w.act('columns-right',lambda s:s['path']==str(root/'A') and ready(s));w.act('columns-select:nested',ready);w.capture('columns-nested-chain')
        w.act('columns-right',lambda s:s['path']==str(root/'A/nested') and ready(s));w.act('columns-select:same.txt',ready);w.capture('columns-file-focused')
        form=w.act('columns-copy-form',lambda s:s['columns']['copy_form'] is not None);assert form['columns']['copy_form']['source']==str(root/'A/nested') and form['columns']['copy_form']['names']==[hx('same.txt')]
        w.capture('columns-copy-captured-source');w.act('columns-copy-confirm',lambda s:s['columns']['copy_success'] and not s['columns']['copy_busy']);assert (Path(w.env['FILES_TEST_COLUMNS_DEST'])/'same.txt').read_bytes()==expected[root/'A/nested/same.txt']
        w.act('columns-left',lambda s:s['path']==str(root/'A') and ready(s));w.act('columns-left',lambda s:s['path']==str(root) and ready(s))
        switched=w.act('columns-rapid',lambda s:ready(s) and len(s['columns']['rows'])==2 and s['columns']['rows'][1]['path']==str(root/'B'));assert switched['path']==str(root) and switched['selected']==hx('B');w.capture('columns-sibling-pruned')
        w.act('columns-right',lambda s:s['path']==str(root/'B') and ready(s));w.act('columns-select:same.txt',ready);w.act('columns-filter',lambda s:s['query']=='same')
        renamed=w.act('columns-rename',lambda s:s['dialog']=='rename');assert renamed['columns']['rename_names']==[hx('same.txt')];w.act('columns-dismiss');w.act('columns-clear-filter');raw=w.act('columns-raw',lambda s:s['selected']==b'raw-\xff'.hex());assert raw['path']==str(root/'B');w.close()
        small=Window(base,root,'narrow',(800,440));small.act('columns-view',ready);small.act('columns-select:A',ready);small.act('columns-right',lambda s:s['path']==str(root/'A') and ready(s));small.act('columns-select:nested',ready);small.act('columns-right',lambda s:s['path']==str(root/'A/nested') and ready(s));small.act('columns-select:same.txt',ready);small.capture('columns-initial-800x440');small.close()
        assert all(p.read_bytes()==data for p,data in expected.items());assert not list(root.rglob('.files-*'))
        report.update(functional_gate='PASS',copy_bytes_from_focused_column=True,source_fixtures_unchanged=True,normal_close_ack=True,initial_narrow=True,live_resize='UNVALIDATED_NOT_CLAIMED')
    except BaseException as error:
        report['error']=repr(error)
        raise
    finally:
        for w in opened:
            if not w.closed:
                report['runs'].append({'label':w.label,'normal_exit0_after_ack':False,'log':w.log.read_text() if w.log.exists() else ''})
                stop_tree(w.process,None)
        if temporary is not None:temporary.cleanup()
        for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:report[key]=hashlib.sha256((BIN_DIR/name).read_bytes()).hexdigest()
        path=OUT/('columns-ui-'+str(time.time_ns())+'.json')
        with path.open('x') as out:json.dump(report,out,indent=2)
        print(json.dumps({'report':str(path),'functional_gate':report['functional_gate']}))


def dual_check():
    """Two live browser panes and captured raw operation paths, never metrics."""
    report={'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'capture_dimensions':{},'states':[],'runs':[],
            'method':'real FLTK framebuffer; rows/other header/context/confirm/pause use existing pointer handlers, F9/Tab/F5/F6 equivalents use shared Model Actions; no native input/clipboard',
            'scope':'two independent live lists/history/filter/selection, raw destination+names, real copy and sameFS move, focus/panel close after queue admission; initial narrow instance, not live resize; joint limits/worker lease additionally unit tested'}
    opened=[];temporary=None
    class Window:
        def __init__(self,base,root,label,size):
            self.raw=base/(label+'.ppm');self.log=base/(label+'.log');self.seq=0;self.label=label;self.closed=False;self.size=size
            private=base/(label+'-tmp');private.mkdir()
            self.env={**os.environ,'TMPDIR':str(private),'MAGICK_TEMPORARY_PATH':str(private),'FILES_CONFIG_DIR':str(base/(label+'-config')),
                      'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_TABS':'1','FILES_TEST_ISOLATE_INPUT':'1',
                      'FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1','FILES_TEST_DUAL_B':os.fspath(base/os.fsdecode(b'B-\xfe'))}
            with self.log.open('w') as out:self.process=subprocess.Popen([str(BIN_DIR/'files'),str(root),'--ui-smoke','--ui-scene','hidden-context','--size',f'{size[0]}x{size[1]}','--scale','1','--exit-after','90'],env=self.env,stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
            opened.append(self)
            self.wait(lambda:'FILES_UI_SMOKE_OK' in self.log.read_text(),'smoke')
            self.wait(lambda:geometry(self.process.pid,*size) and observed_geometry(self.process.pid,*size),'geometry')
        def wait(self,check,why,allow_exit=False):
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                value=check()
                if value:return value
                if self.process.poll() is not None and not allow_exit:raise AssertionError(self.label+': '+why+' exit='+str(self.process.returncode))
                time.sleep(.02)
            raise AssertionError(self.label+': '+why)
        def state(self,after=0,condition=lambda s:True):
            def check():
                for line in reversed(self.log.read_text()[after:].splitlines()):
                    if line.startswith('FILES_HIDDEN_STATE '):
                        s=json.loads(line[len('FILES_HIDDEN_STATE '):])
                        if s['seq']==self.seq and condition(s):return s
                return False
            s=self.wait(check,'fresh state seq='+str(self.seq));report['states'].append({'run':self.label,**s});return s
        def command(self,action):
            after=len(self.log.read_text());pending=self.raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,self.raw.with_suffix('.command'))
            match=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',self.log.read_text()[after:],re.M),'command '+action,action=='dual-close');self.seq=int(match[1]);return after
        def act(self,action,condition=lambda s:True):return self.state(self.command(action),condition)
        def capture(self,name):
            self.raw.unlink(missing_ok=True);self.raw.with_suffix('.request').touch()
            def full():
                if not self.raw.exists():return False
                parts=self.raw.read_bytes().split(b'\n',3)
                if len(parts)!=4:return False
                dims=tuple(map(int,parts[1].split()))
                if len(parts[3])!=dims[0]*dims[1]*3:return False
                assert dims==self.size,('unexpected framebuffer dimensions',dims,self.size)
                return True
            self.wait(full,'framebuffer');p=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(self.raw),str(p)],env=self.env,check=True,timeout=10);report['captures'].append(str(p));report['capture_dimensions'][str(p)]=self.size
        def close(self):
            after=self.command('dual-close');self.wait(lambda:self.process.poll() is not None,'normal close',True)
            text=self.log.read_text();assert self.process.returncode==0 and 'FILES_SPACE_DRAINED' in text[after:] and 'FILES_CLOSE_DRAINED' in text[after:]
            self.closed=True;report['runs'].append({'label':self.label,'normal_exit0_after_ack':True,'log':text})

    try:
        instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')];assert len(matches)==1;os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance']
        temporary=tempfile.TemporaryDirectory(prefix='dual-ui-',dir=BASE);base=Path(temporary.name);a=base/'A';b=base/os.fsdecode(b'B-\xfe')
        for root in (a,b):
            (root/'folder').mkdir(parents=True)
            for name in ('same.txt','other.txt','.hidden'):(root/name).write_bytes((root.name+'/'+name).encode(errors='surrogateescape'))
            (root/'folder/inside.txt').write_bytes(b'nested')
        copy=a/os.fsdecode(b'copy-\xff');move=a/os.fsdecode(b'move-\xff');copy.write_bytes(b'A copied raw bytes\n');move.write_bytes(b'A moved raw bytes\n')
        expected={p:p.read_bytes() for root in (a,b) for p in root.rglob('*') if p.is_file()};hx=lambda p:os.fsencode(p).hex()
        ready=lambda s:s['dual']['enabled'] and all(p['loaded'] and not p['error'] for p in s['dual']['panels'])
        active=lambda s:next(p for p in s['dual']['panels'] if p['active'])
        w=Window(base,a,'main',(1000,650));w.act('dual-on',ready);owner_a=active(w.state())['id']
        w.act('dual-other-header',ready);w.act('dual-b-location',ready);owner_b=active(w.state())['id'];assert owner_a!=owner_b
        w.act('dual-select:same.txt');w.act('dual-filter',ready);assert active(w.state())['query']=='same';w.act('dual-clear-filter',ready)
        w.act('dual-folder',lambda s:ready(s) and active(s)['path_raw']==hx(b/'folder'));w.act('dual-back',lambda s:ready(s) and active(s)['path_raw']==hx(b))
        w.act('dual-switch',ready);w.act('dual-batch');w.act('dual-switch',ready);state=w.act('dual-menu-other',lambda s:s['context'] is not None)
        assert active(state)['id']==owner_a and set(state['selection'])=={hx('same.txt'),hx('other.txt')};w.capture('dual-other-panel-context-batch');w.act('dual-menu-close')
        form=w.act('dual-copy-raw',lambda s:s['dual']['copy_form'] is not None)['dual']['copy_form'];assert form['source']==hx(a) and form['destination']==hx(b) and form['names']==[hx(copy.name)]
        w.capture('dual-copy-opposite-raw-destination');w.act('dual-paused');w.act('dual-confirm',lambda s:s['dual']['copy_phase']=='Paused');assert not (b/copy.name).exists()
        queued=w.state();assert queued['dual']['copy_owner']==[owner_a,1];w.capture('dual-paused-shared-queue');w.act('dual-filter:copy')
        w.act('dual-switch',ready);w.act('dual-filter');w.act('dual-select:same.txt');w.act('dual-resume',lambda s:ready(s) and not s['dual']['copy_busy'] and s['dual']['copy_success'])
        after=w.state();assert active(after)['id']==owner_b and active(after)['query']=='same' and after['selected']==hx('same.txt');assert next(p for p in after['dual']['panels'] if not p['active'])['query']=='copy';assert (b/copy.name).read_bytes()==expected[copy] and copy.read_bytes()==expected[copy];assert (b/'same.txt').read_bytes()==expected[b/'same.txt']
        w.capture('dual-copy-finished-other-focus-preserved');w.act('dual-clear-filter');w.act('dual-switch',ready);w.act('dual-clear-filter')
        form=w.act('dual-move-raw',lambda s:s['dual']['copy_form'] is not None)['dual']['copy_form'];assert form['source']==hx(a) and form['destination']==hx(b)
        w.act('dual-paused');w.act('dual-confirm',lambda s:bool(s['dual']['copy_form'] and s['dual']['copy_form']['reviewed']));assert move.read_bytes()==expected[move] and not (b/move.name).exists()
        w.capture('dual-move-reviewed-opposite');w.act('dual-confirm',lambda s:s['dual']['copy_phase']=='Paused');assert move.read_bytes()==expected[move] and not (b/move.name).exists()
        w.act('dual-switch',ready);closed=w.act('dual-close-panel',lambda s:not s['dual']['enabled']);assert active(closed)['id']==owner_a and closed['dual']['copy_busy']
        w.act('dual-resume',lambda s:not s['dual']['copy_busy'] and s['dual']['copy_success']);assert not move.exists() and (b/move.name).read_bytes()==expected[move];w.capture('dual-move-completed-after-panel-close');w.close()
        small=Window(base,a,'narrow',(800,440));small.act('dual-on',ready);small.act('dual-other-header');small.act('dual-b-location',ready);small.act('dual-select:same.txt');small.capture('dual-initial-800x440')
        small.act('dual-preferences');small.capture('dual-preferences-initial-800x440');small.act('dual-preferences');small.close()
        for p,data in expected.items():
            if p!=move:assert p.read_bytes()==data,(p,'changed source or occupant')
        assert not list(a.rglob('.files-*')) and not list(b.rglob('.files-*'))
        report.update(functional_gate='PASS',raw_destination_and_names_exact=True,copy_preserved_source_and_occupants=True,move_samefs_inode_and_metadata='UNIT_TESTED_CORE_NOT_REMEASURED',review_and_paused_move_non_mutating=True,completion_focus_and_query_preserved=True,panel_close_keeps_queue=True,normal_close_ack=True,initial_narrow=True,live_resize='UNVALIDATED_NOT_CLAIMED')
    except BaseException as error:
        report['error']=repr(error)
        raise
    finally:
        for w in opened:
            if not w.closed:
                report['runs'].append({'label':w.label,'normal_exit0_after_ack':False,'log':w.log.read_text() if w.log.exists() else ''})
                stop_tree(w.process,None)
        if temporary is not None:temporary.cleanup()
        for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:report[key]=hashlib.sha256((BIN_DIR/name).read_bytes()).hexdigest()
        path=OUT/('dual-ui-'+str(time.time_ns())+'.json')
        with path.open('x') as out:json.dump(report,out,indent=2)
        print(json.dumps({'report':str(path),'functional_gate':report['functional_gate']}))

def clipboard_check(kde_only=False):
    """Private file clipboard through the existing framebuffer/command fixture."""
    report={'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'capture_dimensions':{},'states':[],'runs':[],
            'method':'visible entry/context-menu/Actions/dialog/queue targets use shared pointer handlers; navigation and context keyboard use Model handlers; real FLTK framebuffer, NO native keyboard/mouse events',
            'scope':'private same-window files buffer and private-pipe injected DnD, NOT OS MIME file clipboard/DnD. Text fixtures use own FLTK text immediately and after returning to the event loop; no compositor ownership/external focus/clipboard claim. Initial narrow geometry only; no live resize or resource sampling.'}
    opened=[];temporary=None
    class Window:
        def __init__(self,base,root,label,size):
            self.raw=base/(label+'.ppm');self.log=base/(label+'.log');self.seq=0;self.label=label;self.closed=False;self.size=size
            private=base/(label+'-tmp');private.mkdir()
            self.env={**os.environ,'TMPDIR':str(private),'MAGICK_TEMPORARY_PATH':str(private),'FILES_CONFIG_DIR':str(base/(label+'-config')),
                      'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_TABS':'1','FILES_TEST_ISOLATE_INPUT':'1',
                      'FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1','FILES_TEST_CLIP_A':os.fspath(root),'FILES_TEST_CLIP_B':os.fspath(base/os.fsdecode(b'B-\xfe'))}
            with self.log.open('w') as out:self.process=subprocess.Popen([str(BIN_DIR/'files'),str(root),'--ui-smoke','--ui-scene','hidden-context','--size',f'{size[0]}x{size[1]}','--scale','1','--exit-after','90'],env=self.env,stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
            opened.append(self)
            self.wait(lambda:'FILES_UI_SMOKE_OK' in self.log.read_text(),'smoke')
            self.wait(lambda:geometry(self.process.pid,*size) and observed_geometry(self.process.pid,*size),'geometry')
        def wait(self,check,why,allow_exit=False):
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                value=check()
                if value:return value
                if self.process.poll() is not None and not allow_exit:raise AssertionError(self.label+': '+why+' exit='+str(self.process.returncode))
                time.sleep(.02)
            raise AssertionError(self.label+': '+why)
        def state(self,after=0,condition=lambda s:True):
            def check():
                raw=self.log.read_bytes();text=raw[:raw.rfind(b'\n')+1].decode('utf-8')
                for line in reversed(text[after:].splitlines()):
                    if line.startswith('FILES_HIDDEN_STATE '):
                        s=json.loads(line[len('FILES_HIDDEN_STATE '):])
                        if s['seq']==self.seq and condition(s):return s
                return False
            s=self.wait(check,'fresh state seq='+str(self.seq));report['states'].append({'run':self.label,**s});return s
        def command(self,action):
            after=len(self.log.read_text());pending=self.raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,self.raw.with_suffix('.command'))
            match=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',self.log.read_text()[after:],re.M),'command '+action,action=='clip-close');self.seq=int(match[1]);return after
        def act(self,action,condition=lambda s:True):return self.state(self.command(action),condition)
        def capture(self,name):
            self.raw.unlink(missing_ok=True);self.raw.with_suffix('.request').touch()
            def full():
                if not self.raw.exists():return False
                parts=self.raw.read_bytes().split(b'\n',3)
                if len(parts)!=4:return False
                dims=tuple(map(int,parts[1].split()))
                if len(parts[3])!=dims[0]*dims[1]*3:return False
                assert dims==self.size,('unexpected framebuffer dimensions',dims,self.size)
                return True
            self.wait(full,'framebuffer');p=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(self.raw),str(p)],env=self.env,check=True,timeout=10);report['captures'].append(str(p));report['capture_dimensions'][str(p)]=self.size
        def close(self):
            after=self.command('clip-close');self.wait(lambda:self.process.poll() is not None,'normal close',True)
            text=self.log.read_text();assert self.process.returncode==0 and 'FILES_SPACE_DRAINED' in text[after:] and 'FILES_CLOSE_DRAINED' in text[after:]
            self.closed=True;report['runs'].append({'label':self.label,'normal_exit0_after_ack':True,'log':text})

    try:
        instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')];assert len(matches)==1;os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance']
        temporary=tempfile.TemporaryDirectory(prefix='clipboard-ui-',dir=BASE);base=Path(temporary.name);a=base/'A';b=base/os.fsdecode(b'B-\xfe');a.mkdir();b.mkdir()
        raw=a/os.fsdecode(b'copy-\xff');raw.write_bytes(b'raw source bytes\n')
        drag=a/os.fsdecode(b'dnd-\xff');drag.write_bytes(b'private dnd source\n')
        for name in ('cut-a','cut-b','changed','late'):(a/name).write_bytes(('source '+name).encode())
        (b/'occupant').write_bytes(b'never overwrite');hx=lambda p:os.fsencode(p).hex()
        ready=lambda s:s['dual']['panels'][0]['loaded'] and not s['error']
        buffered=lambda n:lambda s:not s['clipboard']['busy'] and s['clipboard']['count']==n
        form=lambda s:s['dual']['copy_form'] is not None
        reviewed=lambda s:form(s) and s['dual']['copy_form']['reviewed'] and not s['dual']['copy_form']['error']
        done=lambda s:not s['dual']['copy_busy'] and s['dual']['copy_id']>0
        def select(w,path,toggle=False):w.act(('clip-toggle:' if toggle else 'clip-select:')+hx(path.name))
        def menu(w):w.act('clip-menu',lambda s:s['context'] is not None)
        def paste(w):menu(w);return w.act('clip-paste',form)
        if kde_only:
            w=Window(base,a,'kde',(1000,650));w.act('list',ready);w.act('clip-b',ready)
            s=w.act('clip-kde-cut',lambda s:form(s) and s['native_transfer']['fixture_ack'] is True)
            f=s['dual']['copy_form'];assert f['moving'] and f['source']==hx(a) and f['destination']==hx(b) and f['names']==[hx('changed')]
            assert (a/'changed').read_bytes()==b'source changed' and not (b/'changed').exists()
            w.capture('native-kde-private-cut-review');w.act('clip-cancel-dialog',lambda s:s['dual']['copy_form'] is None)
            assert (a/'changed').read_bytes()==b'source changed' and not (b/'changed').exists();w.capture('native-kde-private-cancelled');w.close()
            report.update(functional_gate='PASS',scope='KDE private pipes to AuxIo and Move review, simulated ACK; no native input',kde_cut_review=True,cancel_preserves_source=True,native_mime_interoperability='UNVALIDATED_NO_NATIVE_INPUT',normal_close_ack=True)
            return
        w=Window(base,a,'main',(1000,650));w.act('list',ready);select(w,raw);menu(w);w.capture('clipboard-copy-context');w.act('clip-copy',buffered(1));generation=w.state()['clipboard']['generation']
        w.act('clip-b',ready);f=paste(w)['dual']['copy_form'];assert f['source']==hx(a) and f['destination']==hx(b) and f['names']==[hx(raw.name)]
        w.capture('clipboard-copy-review');w.act('clip-cancel-dialog',lambda s:s['dual']['copy_form'] is None);assert w.state()['clipboard']['count']==1 and not (b/raw.name).exists()
        w.act('clip-actions');w.act('clip-actions-paste',form);w.act('clip-paused');w.act('clip-confirm',lambda s:s['dual']['copy_phase']=='Paused');w.act('clip-cancel-job',done)
        assert not (b/raw.name).exists() and raw.read_bytes()==b'raw source bytes\n' and w.state()['clipboard']['count']==1;w.act('clip-dismiss')
        paste(w);w.act('clip-confirm',lambda s:done(s) and s['dual']['copy_success']);assert (b/raw.name).read_bytes()==raw.read_bytes() and w.state()['clipboard']['generation']==generation
        w.capture('clipboard-copy-finished');w.act('clip-a',ready);select(w,a/'cut-a');select(w,a/'cut-b',True);menu(w);w.act('clip-cut',buffered(2))
        assert (a/'cut-a').read_bytes()==b'source cut-a' and (a/'cut-b').read_bytes()==b'source cut-b'
        cut_generation=w.state()['clipboard']['generation'];w.act('clip-b',ready);paste(w);w.act('clip-confirm',reviewed)
        w.act('clip-cancel-dialog',lambda s:s['dual']['copy_form'] is None)
        assert w.state()['clipboard']['count']==2 and w.state()['clipboard']['generation']==cut_generation
        paste(w);w.act('clip-paused');w.act('clip-confirm',reviewed);w.act('clip-confirm',lambda s:s['dual']['copy_phase']=='Paused');w.act('clip-cancel-job',done)
        assert w.state()['clipboard']['count']==2 and w.state()['clipboard']['generation']==cut_generation
        assert (a/'cut-a').read_bytes()==b'source cut-a' and (a/'cut-b').read_bytes()==b'source cut-b' and not (b/'cut-a').exists() and not (b/'cut-b').exists()
        w.act('clip-dismiss');paste(w);w.act('clip-confirm',reviewed);assert not (b/'cut-a').exists() and not (b/'cut-b').exists()
        (b/'cut-b').write_bytes(b'actor occupant');w.act('clip-confirm',lambda s:done(s) and s['clipboard']['count']==1)
        assert not (a/'cut-a').exists() and (b/'cut-a').read_bytes()==b'source cut-a' and (a/'cut-b').read_bytes()==b'source cut-b' and (b/'cut-b').read_bytes()==b'actor occupant'
        w.capture('clipboard-cut-partial-retained');w.act('clip-dismiss');(b/'cut-b').unlink();paste(w);w.act('clip-confirm',reviewed);w.act('clip-confirm',lambda s:done(s) and s['clipboard']['count']==0)
        assert not (a/'cut-b').exists() and (b/'cut-b').read_bytes()==b'source cut-b';w.capture('clipboard-cut-retry-finished')
        w.act('clip-a',ready);select(w,a/'changed');menu(w);w.act('clip-cut',buffered(1))
        cut_before=w.state()['clipboard']['generation']
        sender=w.act('clip-dnd-send',lambda s:s['native_transfer']['drained'] and 'fixture aislada' in (s['native_transfer']['message'] or ''))
        assert sender['clipboard']['kind']=='Cut' and sender['clipboard']['generation']==cut_before and sender['clipboard']['count']==1
        assert (a/'changed').read_bytes()==b'source changed';w.capture('native-dnd-sender-private-ready')
        (a/'changed').rename(a/'preserved');(a/'changed').write_bytes(b'actor replacement')
        w.act('clip-b',ready);paste(w);w.act('clip-confirm',lambda s:form(s) and bool(s['dual']['copy_form']['error']))
        assert w.state()['clipboard']['count']==1 and (a/'changed').read_bytes()==b'actor replacement' and (a/'preserved').read_bytes()==b'source changed' and not (b/'changed').exists();w.capture('clipboard-replacement-rejected');w.act('clip-cancel-dialog')
        w.act('clip-text-own',lambda s:s['editing']);w.act('clip-text-request',lambda s:s['edit']=='texto á\tb' and not s['clipboard']['text_pending']);w.act('clip-text-cancel')
        w.act('clip-text-own',lambda s:s['editing']);w.act('clip-text-request-later',lambda s:s['edit']=='texto á\tb' and not s['clipboard']['text_pending']);w.act('clip-text-cancel')
        w.act('clip-a',ready);select(w,a/'late');menu(w);w.act('clip-cut',buffered(1));old=w.state()['clipboard']['generation'];w.act('clip-b',ready);paste(w);w.act('clip-paused');w.act('clip-confirm',reviewed);w.act('clip-confirm',lambda s:s['dual']['copy_phase']=='Paused')
        w.act('clip-a',ready);select(w,raw);menu(w);w.act('clip-copy',buffered(1));new=w.state()['clipboard']['generation'];assert new!=old
        w.act('clip-resume',lambda s:done(s) and s['dual']['copy_success']);assert w.state()['clipboard']['generation']==new and w.state()['clipboard']['kind']=='Copy' and w.state()['clipboard']['count']==1
        assert not (a/'late').exists() and (b/'late').read_bytes()==b'source late' and (b/'occupant').read_bytes()==b'never overwrite'
        # Explicit private-pipe fixture, no native Wayland offer or forged serial.
        w.act('clip-b',ready)
        hovered=w.act('clip-dnd-hover',lambda s:s['native_transfer']['hover_path'] is not None)
        assert bytes(hovered['native_transfer']['hover_path'])==os.fsencode(b)
        w.capture('native-dnd-private-hover')
        d=w.act('clip-dnd-drop',lambda s:form(s) and s['native_transfer']['fixture_ack'] is True)
        assert d['dual']['copy_form']['source']==hx(a) and d['dual']['copy_form']['destination']==hx(b) and d['dual']['copy_form']['names']==[hx(drag.name)]
        assert drag.read_bytes()==b'private dnd source\n' and not (b/drag.name).exists()
        w.capture('native-dnd-private-review')
        w.act('clip-cancel-dialog',lambda s:s['dual']['copy_form'] is None)
        assert drag.read_bytes()==b'private dnd source\n' and not (b/drag.name).exists()
        w.act('clip-dnd-hover',lambda s:s['native_transfer']['hover_path'] is not None)
        w.act('clip-dnd-drop',lambda s:form(s) and s['native_transfer']['fixture_ack'] is True)
        w.act('clip-confirm',lambda s:done(s) and s['dual']['copy_success'])
        assert drag.read_bytes()==b'private dnd source\n' and (b/drag.name).read_bytes()==b'private dnd source\n'
        w.capture('native-dnd-private-copy-finished');w.close()
        small=Window(base,a,'narrow',(800,440));small.act('list',ready);select(small,raw);menu(small);small.act('clip-copy',buffered(1));menu(small)
        def inside(s):
            targets=s['clipboard']['action_targets'];assert targets and all(x>=0 and y>=0 and width>0 and height>0 and x+width<=800 and y+height<=440 for t in targets for x,y,width,height in [t['rect']]);return s
        inside(small.state());small.capture('clipboard-context-initial-800x440')
        assert any('Pegar del sistema' in label for label in small.state()['context']['items'])
        before=small.state()['clipboard']['generation']
        rejected=small.act('clip-system-paste',lambda s:'gesto/foco real' in (s['native_transfer']['message'] or ''))
        assert rejected['native_transfer']['drained'] and rejected['dual']['copy_form'] is None and not rejected['dual']['copy_busy'] and rejected['clipboard']['generation']==before
        small.capture('clipboard-system-without-gesture-rejected-initial-800x440')
        small.act('clip-actions');inside(small.state());small.capture('clipboard-actions-initial-800x440');small.close()
        assert not list(a.rglob('.files-*')) and not list(b.rglob('.files-*'))
        report.update(private_dnd_sender_auxio_copy_preserves_cut=True,sender_dispatch='PRIVATE_STUB_NO_NATIVE_INPUT',private_dnd_pipe_auxio_review_copy=True,private_dnd_cancel_preserves_source=True,dnd_native_interoperability='UNVALIDATED_PRIVATE_PIPE_ONLY',native_system_control_visible=True,native_without_gesture_rejected_no_fallback=True,native_mime_interoperability='UNVALIDATED_NO_NATIVE_INPUT',functional_gate='PASS',raw_copy_exact=True,cut_capture_and_review_non_mutating=True,cancel_does_not_consume=True,partial_cut_retry_preserves_original_proofs=True,replacement_rejected=True,old_ack_preserves_new_buffer=True,owned_text_callback_exact=True,owned_text_after_eventloop_exact=True,menus_initial_narrow=True,normal_close_ack=True,live_resize='UNVALIDATED_NOT_CLAIMED',tree_cleanup='No process sampler: normal root exit0 plus application drain ACK, not independent descendant-lifetime coverage')
    except BaseException as error:
        report['error']=repr(error)
        raise
    finally:
        for w in opened:
            if not w.closed:
                report['runs'].append({'label':w.label,'normal_exit0_after_ack':False,'log':w.log.read_text() if w.log.exists() else ''});stop_tree(w.process,None)
        if temporary is not None:temporary.cleanup()
        for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:report[key]=hashlib.sha256((BIN_DIR/name).read_bytes()).hexdigest()
        path=OUT/('clipboard-ui-'+str(time.time_ns())+'.json')
        with path.open('x') as out:json.dump(report,out,indent=2)
        print(json.dumps({'report':str(path),'functional_gate':report['functional_gate']}))

if '--kde-only' in __import__('sys').argv:
    clipboard_check(kde_only=True)
    raise SystemExit(0)

if '--clipboard' in __import__('sys').argv:
    clipboard_check()
    raise SystemExit(0)

if '--dual' in __import__('sys').argv:
    dual_check()
    raise SystemExit(0)

if '--columns' in __import__('sys').argv:
    columns_check()
    raise SystemExit(0)

if '--sessions' in __import__('sys').argv:
    session_check()
    raise SystemExit(0)

report={'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'states':[],
        'method':'plus and visible tab targets use pointer handlers; off-strip tab selection and cycle/close/reopen use shared Actions; no native keyboard/mouse input or clipboard access'}
try:
    instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2))
    matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')]
    assert len(matches)==1,'Ambiguous compositor instance for the current Wayland socket'
    os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance']
    with tempfile.TemporaryDirectory(prefix='tabs-ui-',dir=BASE) as temp:
        base=Path(temp);a=base/'A';b=base/'B';a.mkdir();b.mkdir()
        for folder in [a,b]:
            (folder/"folder").mkdir();(folder/"folder"/"inside.txt").write_text("fixture")
            for name in ['keep.txt','keeper.txt','same.txt','.hidden']+[f'keep-{i:02}.txt' for i in range(60)]:
                (folder/name).write_text(folder.name+' content\n')
            (folder/os.fsdecode(b'raw-\xff.txt')).write_text(folder.name+' raw\n')
        raw=base/'capture.ppm';log=base/'app.log';private=base/'tmp';private.mkdir()
        env={**os.environ,'TMPDIR':str(private),'MAGICK_TEMPORARY_PATH':str(private),'FILES_CONFIG_DIR':str(base/'config'),
             'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_TABS':'1','FILES_TEST_PREVIEW':'1',
             'FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1'}
        with log.open('w') as out:process=subprocess.Popen([str(BIN_DIR/'files'),str(a),'--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','90'],env=env,stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
        normal=False;seq=0
        def wait(check,reason,allow_exit=False):
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                value=check()
                if value:return value
                if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exited='+str(process.returncode))
                time.sleep(.02)
            raise AssertionError(reason)
        def command(action):
            global seq
            after=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
            match=wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',log.read_text()[after:],re.M),'command failed '+action,action=='tab-close')
            seq=int(match[1]);return after
        def state(after=0,condition=lambda s:True):
            def read():
                for line in reversed(log.read_text()[after:].splitlines()):
                    if line.startswith('FILES_HIDDEN_STATE '):
                        s=json.loads(line[len('FILES_HIDDEN_STATE '):])
                        if s['seq']==seq and not s['config_dirty'] and condition(s):return s
                return False
            s=wait(read,'fresh ready state absent seq='+str(seq));report['states'].append(s);return s
        def act(action,condition=lambda s:True):return state(command(action),condition)
        def capture(name):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def full():
                if not raw.exists():return False
                parts=raw.read_bytes().split(b'\n',3)
                return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
            wait(full,'framebuffer incomplete');p=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(p)],env=env,check=True,timeout=10);report['captures'].append(str(p))
        hx=lambda s:s.encode().hex()
        try:
            wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'smoke absent')
            wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'geometry absent')
            report['geometry']=observed_geometry(process.pid,1000,650);initial=state();assert len(initial['tabs'])==1
            aa=act('tab-config-a');assert aa['query']=='keep' and aa['view']=='grid' and aa['scroll']==120 and len(aa['selection'])==2;aid=aa['tab_active'];capture('tabs-A-selection')
            act('tab-new');bb=act('tab-b-location');bid=bb['tab_active'];assert bid!=aid and bb['path']==str(b)
            bb=act('tab-config-b');assert bb['query']=='same' and bb['view']=='list' and bb['selection']==[hx('same.txt')] and bb['history_back'];capture('tabs-B-independent')
            aa=act('tab-a');assert aa['tab_active']==aid and aa['query']=='keep' and aa['view']=='grid' and aa['scroll']==120 and aa['grid_zoom']==1.25 and aa['descending'] and aa['anchor']==hx('keep.txt') and aa['selected']==hx('keeper.txt') and len(aa['selection'])==2
            aa=act('tab-rapid');assert aa['tab_active']==aid and len(aa['selection'])==2 and aa['anchor']==hx('keep.txt');capture('tabs-restored-after-rapid')
            act('tab-b');act('tab-preview',lambda s:s['preview']=='text');act('tab-preview-b')
            aa=act('tab-a',lambda s:s['preview']=='closed' and s['preview_drained']);assert aa['path']==str(a)
            act('tab-hidden-select');act('tab-b');act('tab-hidden');aa=act('tab-a');assert not aa['show_hidden'] and not aa['selection'] and aa['selected'] is None
            aa=act('tab-raw-select');assert aa['selected']==b'raw-\xff.txt'.hex();act('tab-b');aa=act('tab-a');assert aa['selected']==b'raw-\xff.txt'.hex();capture('tabs-raw-selection')
            for _ in range(6):act('tab-new')
            limit=act('tab-new');assert len(limit['tabs'])==8 and 'ocho' in limit['error'];capture('tabs-eight-limit')
            old=limit['tab_active'];act('tab-close');reopened=act('tab-reopen');assert len(reopened['tabs'])==8 and reopened['tab_active']>old
            act('tab-file',lambda s:s['selected_metadata'])
            narrow=act('fixture-narrow',lambda s:s['width']==800 and s['height']==500);capture('tabs-resize-800x500')
            directory=act('tab-directory',lambda s:s['selected_metadata']);assert all(r[1]+r[3]<=500-174 for r in directory['calculate_targets']);capture('tabs-directory-800x500')
            short=act('fixture-short',lambda s:s['width']==800 and s['height']==440);capture('tabs-resize-800x440')
            assert all(r[1]+r[3]<=440-174 for r in short['calculate_targets'])
            act('tab-file',lambda s:s['selected_metadata']);capture('tabs-file-800x440')
            act('tab-close-others');after=command('tab-close');wait(lambda:process.poll() is not None,'last tab did not drain and exit',True)
            text=log.read_text();assert process.returncode==0 and 'FILES_SPACE_DRAINED' in text[after:] and 'FILES_CLOSE_DRAINED' in text[after:]
            identities=sorted(set((int(p),int(s)) for p,s in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=',text)));assert identities
            for pid,start in identities:
                proc=Path(f'/proc/{pid}')
                try:fields=(proc/'stat').read_text().rsplit(')',1)[1].split();assert int(fields[19])!=start,'logged helper still present (zombie rejected)'
                except (FileNotFoundError,ProcessLookupError):assert not proc.exists(),'known PID directory unreadable'
            assert (a/'same.txt').read_text()=='A content\n' and (b/'same.txt').read_text()=='B content\n'
            normal=True;report.update(functional_gate='PASS',normal_last_tab_exit0_after_ack=True,independent_history_view_filter_selection=True,rapid_switch_before_pump=True,hidden_global_and_pruned=True,opaque_selection=True,eight_tabs_and_reopen=True,logged_helper_cleanup={'complete':True,'identities':identities,'zombies_accepted':False,'scope':'logged PID/start identities only; not helper alive at close or lifetime coverage'},log=text)
        except BaseException:
            report['log']=log.read_text();raise
        finally:
            if not normal:stop_tree(process,None)
except BaseException as error:
    report['error']=repr(error);raise
finally:
    for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:report[key]=hashlib.sha256((BIN_DIR/name).read_bytes()).hexdigest()
    p=OUT/('tabs-ui-'+str(time.time_ns())+'.json')
    with p.open('x') as out:json.dump(report,out,indent=2)
    print(json.dumps({'report':str(p),'functional_gate':report['functional_gate']}))
