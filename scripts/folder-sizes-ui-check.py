#!/usr/bin/env python3
"""Queued folder sizes functional evidence; no resource/startup measurement or OS input."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--manager',action='store_true');parser.add_argument('--summaries',action='store_true');parser.add_argument('--forget',action='store_true');parser.add_argument('--sync',action='store_true');parser.add_argument('--sort',action='store_true');parser.add_argument('--filters',action='store_true');args=parser.parse_args()
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'docs/evidence/folder-sizes';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report=dict(functional_gate='IN_PROGRESS',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',method='production actions/painted hits through private fixture pipe; real files-helper IPC and FLTK framebuffer; no native input',states=[],captures=[])

def summaries():
    """Counter-vs-recursive-total scope using the existing owned scene."""
    report=dict(functional_gate='FAIL',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',method='existing private Actions/painted handlers, real helper IPC and FLTK framebuffer; no OS input',states=[],captures=[])
    process=None;log=None
    with tempfile.TemporaryDirectory(prefix='folder-summary-',dir=BASE)as temp:
        root=Path(temp);source=root/'Projects';source.mkdir()
        for n in ['config','tmp']:(root/n).mkdir()
        (root/'config/config.toml').write_text('show_hidden=false\n')
        small=source/'A-Small';(small/'sub').mkdir(parents=True);(small/'a').write_bytes(b'123');(small/'sub/b').write_bytes(b'12345');(small/'sub/cycle').symlink_to(small);(small/'broken').symlink_to('missing')
        many=source/'B-Many';many.mkdir()
        for i in range(1025):(many/f'f{i}').touch()
        (source/'C-Empty').mkdir()
        if args.forget:
            opaque=source/os.fsdecode(b'D-Raw-\xff');opaque.mkdir();(opaque/'data').write_bytes(b'raw bytes')
        if args.sync or args.sort or args.filters:
            large=source/'Z-Large';large.mkdir();(large/'data').write_bytes(b'x'*20)
            if args.sort:
                for i,p in enumerate([small,many,source/'C-Empty',large]):os.utime(p,(1000+i*1000,1000+i*1000))
        if args.filters:(source/'.Hidden').mkdir()
        raw=root/'frame.ppm';log=root/'app.log';seq=0
        def tree_fingerprint():return {os.fsencode(p.relative_to(source)):('link',os.readlink(p))if p.is_symlink()else('dir',)if p.is_dir()else('file',hashlib.sha256(p.read_bytes()).hexdigest())for p in source.rglob('*')}
        original_tree=tree_fingerprint()if args.forget or args.filters else None
        original={str(p.relative_to(small)):hashlib.sha256(p.read_bytes()).hexdigest()for p in [small/'a',small/'sub/b']}
        def wait(check,why,exits=False):
            end=time.monotonic()+25
            while time.monotonic()<end:
                v=check()
                if v:return v
                if process.poll()is not None and not exits:raise AssertionError(why+' exit '+str(process.returncode))
                time.sleep(.015)
            raise AssertionError(why)
        def state(after=0,predicate=lambda s:True):
            def current():
                for line in reversed(log.read_text()[after:].splitlines()):
                    if line.startswith('FILES_HIDDEN_STATE '):
                        v=json.loads(line[len('FILES_HIDDEN_STATE '):])
                        if v['seq']==seq and predicate(v):return v
            v=wait(current,'state absent seq '+str(seq));report['states'].append(v);return v
        def command(action,exits=False):
            nonlocal seq
            after=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);pending.replace(raw.with_suffix('.command'))
            m=wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',log.read_text()[after:],re.M),'command absent '+action,exits);seq=int(m[1]);return after
        def act(action,predicate=lambda s:True):return state(command(action),predicate)
        def direct(s):return{bytes(r['name']):r for r in s['direct_summaries']}
        def measured(s):return{bytes(r['name']):r for r in s['folder_usage']['rows']}
        def capture(name,dims=(1000,650)):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def full():
                if not raw.exists():return False
                parts=raw.read_bytes().split(b'\n',3)
                if len(parts)!=4:return False
                w,h=map(int,parts[1].split());return(w,h)if len(parts[3])==w*h*3 else False
            assert wait(full,'framebuffer incomplete')==dims
            path=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],env=env,check=True,timeout=15)
            assert path.read_bytes()[16:24]==dims[0].to_bytes(4,'big')+dims[1].to_bytes(4,'big');report['captures'].append(dict(path=str(path),dimensions=dims))
        try:
            env={**os.environ,'TMPDIR':str(root/'tmp'),'MAGICK_TEMPORARY_PATH':str(root/'tmp'),'FILES_CONFIG_DIR':str(root/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1','FILES_TEST_PREVIEW':'1'}
            with log.open('w')as output:process=subprocess.Popen([str(ROOT/'target/release/files'),str(source),'--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','120'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
            if args.filters:
                state();act('list');act('dual-on')
                act('dual-filter-control:other:edit');act('dual-filter:A-');act('filter-key:enter');act('dual-filter-control:other:edit');act('dual-filter:Z-');act('filter-key:enter')
                s=act('size-key',lambda s:measured(s).get(b'Z-Large',{}).get('label')=='∑ 20 B'and not s['folder_usage']['running']);capture('pane-filters-independent')
                generation=s['generation'];owner=s['dual']['panels'][0]['id'];selection=[p['selection']for p in s['dual']['panels']]
                def stable(s):
                    assert s['generation']==generation and s['dual']['panels'][0]['id']==owner
                    assert [p['selection']for p in s['dual']['panels']]==selection
                    assert measured(s)[b'Z-Large']['label']=='∑ 20 B'
                s=act('dual-filter-control:other:clear');stable(s);assert s['dual']['panels'][0]['query']=='Z-'and s['dual']['panels'][1]['query']==''and len(s['dual']['panels'][1]['order'])==4;capture('pane-filter-clear-other-preserves-owner')
                s=act('dual-filter-control:active:clear');stable(s);assert all(p['query']==''and len(p['order'])==4 for p in s['dual']['panels']);capture('pane-filter-clear-active-retains-marks')
                act('dual-filter-control:active:edit');act('dual-filter:Z-');act('filter-key:enter');s=act('selection-clear');assert s['selection']==[]and s['selected']is None;capture('pane-filter-explicit-zero-before-clear')
                s=act('dual-filter-control:active:clear');assert s['selection']==[]and s['selected']is None and len(s['order'])==4
                s=act('dual-copy');assert s['dual']['copy_form']is None and s['selection']==[]and s['selected']is None;capture('pane-filter-zero-after-clear-no-copy');act('dest-dismiss')
                act('dual-filter-control:active:edit');act('dual-filter:Z-');act('filter-key:enter');act('selection-clear');act('dual-filter-control:active:edit');s=act('filter-key:escape');assert s['selection']==[]and s['selected']is None
                s=act('dual-copy');assert s['dual']['copy_form']is None;capture('pane-filter-zero-after-escape-no-copy');act('dest-dismiss')
                act('dual-filter-control:active:edit');act('dual-filter:Z-');act('filter-key:enter');act('dual-filter-control:other:edit');act('dual-filter:A-');act('filter-key:enter')
                act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);capture('pane-filter-controls-440',(440,440));act('bar-toggle');capture('pane-filter-controls-mc-notice-440',(440,440))
                assert tree_fingerprint()==original_tree;report['source_tree_unchanged']=True
                report['scope']='Independent filters and painted clear/edit controls; inactive clear preserves owner/generation/query/selection/real20B DU observation; explicit zero marks survive Clear and Escape, F5 opens no copy form.8actual1000/440FB including compact notice+MC bar, hidden root excluded, whole owned tree fingerprint unchanged. Loaded counters include hidden loaded entries and are not recursive totals. Edit-other explicitly transfers focus. No OS input/resources/lifetimes proof.'
            elif args.sort:
                def orders(s):return [[bytes.fromhex(n)for n in p['order']]for p in s['dual']['panels']]
                names=[b'A-Small',b'B-Many',b'C-Empty',b'Z-Large']
                state();act('list');act('dual-on');act('dual-switch');act('dual-select:A-Small');act('dual-switch');act('dual-state',lambda s:all(p['loaded']and p['summaries']==4 and p['stat_done']==4 for p in s['dual']['panels']))
                act('dual-select:Z-Large');s=act('size-key',lambda s:measured(s).get(b'Z-Large',{}).get('label')=='∑ 20 B'and not s['folder_usage']['running'])
                generation=s['generation'];active_id=s['dual']['panels'][0]['id'];selection=[p['selection']for p in s['dual']['panels']]
                def stable(s):
                    assert s['generation']==generation and s['dual']['panels'][0]['id']==active_id
                    assert [p['selection']for p in s['dual']['panels']]==selection
                    assert measured(s)[b'Z-Large']['label']=='∑ 20 B'
                s=act('dual-sort:other:size');stable(s);assert orders(s)==[names,[names[3],*names[:3]]];assert s['dual']['panels'][1]['descending'];capture('sort-other-size-without-focus-change')
                s=act('dual-sort:other:size');stable(s);assert not s['dual']['panels'][1]['descending'];capture('sort-other-size-ascending')
                s=act('dual-sort:other:name');stable(s);assert orders(s)==[names,names]
                s=act('dual-sort:other:name');stable(s);assert orders(s)==[names,list(reversed(names))];capture('sort-other-name-descending')
                s=act('dual-sort:other:date',lambda s:s['dual']['panels'][1]['sort']=='Modified');stable(s);assert orders(s)==[names,list(reversed(names))];capture('sort-other-modified-newest-first')
                s=act('dual-sort:active:name');stable(s);assert orders(s)==[list(reversed(names)),list(reversed(names))]
                act('commands-shortcut');s=act('commands-query:ordenar fecha');assert s['commands']['matches'][0]['id']=='sort-date'
                s=act('commands-key:enter',lambda s:s['commands']is None);stable(s);assert s['dual']['panels'][0]['sort']=='Modified'and s['dual']['panels'][0]['descending'];capture('sort-active-date-via-keyboard-command')
                s=act('dual-sort:active:date');stable(s);assert orders(s)==[names,list(reversed(names))]
                act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);s=act('dual-sort:other:name');stable(s);assert orders(s)==[names,names];capture('sort-compact-controls-440',(440,440))
                act('dual-on',lambda s:not s['dual']['enabled']);act('commands-shortcut');act('commands-query:ordenar nombre');s=act('commands-key:enter',lambda s:s['commands']is None);assert s['size_sort']['descending']is False;capture('sort-single-list-440',(440,440))
                report['scope']='Six real sort controls, per-panel direction and criteria, inactive sorting preserves owner/selection/generation/20B measurement. Name/Size/Modified and keyboard command dispatch,7 actual framebuffers including dual/single440.1real DU helper; metadata dates set on owned fixtures. No OS input/resource/performance proof.'
            elif args.sync:
                def orders(s):return [[bytes.fromhex(n)for n in p['order']]for p in s['dual']['panels']]
                names=[b'A-Small',b'B-Many',b'C-Empty',b'Z-Large']
                state();act('list');act('dual-on');act('dual-switch');act('sizes-sort');act('dual-select:A-Small');act('dual-switch')
                s=act('dual-state',lambda s:all(p['loaded']and p['summaries']==4 and p['stat_done']==4 for p in s['dual']['panels']))
                assert orders(s)==[names,names];other=s['dual']['panels'][1];stable=(other['id'],other['path_raw'],other['selection'])
                def stable_other(s):
                    p=s['dual']['panels'][1];assert(p['id'],p['path_raw'],p['selection'])==stable;assert orders(s)[0]==names
                act('dual-select:Z-Large');s=act('size-key',lambda s:measured(s).get(b'Z-Large',{}).get('label')=='∑ 20 B'and not s['folder_usage']['running']);stable_other(s);assert orders(s)[1]==[names[3],*names[:3]];capture('size-sync-first-completion')
                act('dual-select:A-Small');act('size-key',lambda s:measured(s).get(b'A-Small',{}).get('label')=='∑ 8 B'and not s['folder_usage']['running'])
                (large/'data').write_bytes(b'x');act('dual-select:Z-Large');s=act('sizes-selected-paused',lambda s:s['folder_usage']['paused']);stable_other(s);assert orders(s)[1]==names;capture('size-sync-browser-invalidates-old-key')
                s=act('sizes-resume',lambda s:measured(s).get(b'Z-Large',{}).get('label')=='∑ 1 B'and not s['folder_usage']['running']);stable_other(s);assert orders(s)[1]==[names[0],names[3],names[1],names[2]];capture('size-sync-remeasured-one-byte')
                act('clip-actions');act('manager-open',lambda s:s['size_manager']is not None and s['size_manager']['current']);act('manager-home');(large/'data').write_bytes(b'y'*16)
                act('manager-recalculate-paused',lambda s:s['folder_usage']['paused']);s=act('manager-escape');stable_other(s);assert orders(s)[1]==names;capture('size-sync-manager-invalidates-old-key')
                s=act('sizes-resume',lambda s:measured(s).get(b'Z-Large',{}).get('label')=='∑ 16 B'and not s['folder_usage']['running']);stable_other(s);assert orders(s)[1]==[names[3],*names[:3]];capture('size-sync-manager-completion')
                assert(large/'data').read_bytes()==b'y'*16
                report['scope']='Real measure_directory completion and browser/manager recalculation reorder inactive Size panel while active Name order, inactive selection and panel identity remain stable. Actor changes owned Z-Large20→1→16 bytes; other source bytes unchanged. No OS input/resource/startup/performance proof.'
            elif args.forget:
                state();act('list');act('sizes-listed-paused');act('clip-actions');act('manager-open',lambda s:s['size_manager']is not None and s['size_manager']['current']);
                done=lambda s:len(s['size_manager']['rows'])==4 and all((r['label']or'').startswith('∑')for r in s['size_manager']['rows'])
                s=act('manager-resume-hit',done);focus=s['selected'];generation=s['generation'];helpers=log.read_text().count('command=measure_directory')
                s=act('manager-raw');selected=s['size_manager']['rows'][s['size_manager']['selected']];old_id=selected['id'];old_stamp=s['size_manager']['stamp']
                act('manager-info',lambda s:s['size_manager']['details']and s['size_manager']['forget_hit']);capture('size-forget-detail')
                s=act('manager-forget-hit',lambda s:len(s['size_manager']['rows'])==3 and not s['size_manager']['details']);assert s['selected']==focus and s['generation']==generation
                assert old_id not in [r['id']for r in s['size_manager']['rows']]and (opaque/'data').read_bytes()==b'raw bytes'
                act('manager-old-forget:'+str(old_stamp)+':'+str(old_id));assert log.read_text().count('command=measure_directory')==helpers
                act('manager-forget-review-key',lambda s:s['size_manager']['report']is not None and s['size_manager']['forget_measured_hit'])
                act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);capture('size-forget-snapshot-review-440',(440,440))
                s=act('manager-forget-report-hit',lambda s:len(s['size_manager']['rows'])==0 and s['size_manager']['report']is not None and not s['size_manager']['forget_measured_hit'])
                assert s['size_manager']['summary']['bytes']is None and 'SIN MEDICIONES'in s['size_manager']['report']['text'];assert log.read_text().count('command=measure_directory')==helpers
                capture('size-forget-empty-not-zero-440',(440,440));act('manager-escape');s=act('manager-enqueue-paused',lambda s:len(s['size_manager']['rows'])==4 and s['folder_usage']['paused'])
                assert all(r['id']>old_id for r in s['size_manager']['rows'])and s['selected']==focus and s['generation']==generation;assert log.read_text().count('command=measure_directory')==helpers
                capture('size-forget-readmitted-without-refresh-440',(440,440))
                assert original=={str(p.relative_to(small)):hashlib.sha256(p.read_bytes()).hexdigest()for p in[small/'a',small/'sub/b']}
                assert tree_fingerprint()==original_tree;report['source_tree_unchanged']=True
                report['scope']='Forget observation by ID / only completed IDs in frozen report, no FS deletion/refresh/new helper. Actual directory8B/zero/raw data; unknown after forget not measuredzero, paused readmission with fresh IDs and same browser generation/selection. Core128-slot and ACK guards separate unit proof.'
            else:
                state();s=act('groups',lambda s:s['view']=='groups'and len(direct(s))==3)
                d=direct(s);assert d[b'A-Small']['entries']==3 and d[b'A-Small']['status']=='Complete';assert d[b'C-Empty']['entries']==0 and d[b'C-Empty']['status']=='Complete';assert d[b'B-Many']['entries']==1024 and d[b'B-Many']['status']=='Limited'
                assert all(' B'not in row['label']and 'Total:'not in row['display']for row in d.values());assert not measured(s)
                assert 'command=measure_directory'not in log.read_text();capture('direct-count-not-a-total')
                act('select-name:A-Small');s=act('size-hit',lambda s:measured(s).get(b'A-Small',{}).get('label')=='∑ 8 B'and not s['folder_usage']['running'])
                assert direct(s)[b'A-Small']['display'].startswith('Total: 8 B');capture('recursive-total-separated-from-direct-count')
                act('select-name:C-Empty');s=act('size-hit',lambda s:measured(s).get(b'C-Empty',{}).get('label')=='∑ 0 B'and not s['folder_usage']['running'])
                s=act('sizes-sort');assert s['size_sort']['descending'] and [bytes.fromhex(n)for n in s['order']]==[b'A-Small',b'C-Empty',b'B-Many'];s=act('sizes-sort');assert not s['size_sort']['descending'];order=[bytes.fromhex(n)for n in s['order']];assert order==[b'C-Empty',b'A-Small',b'B-Many'];assert b'B-Many'not in measured(s);capture('known-zero-before-eight-and-unknown')
                act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);capture('counter-and-total-controls-440',(440,440))
            command('callback',True);wait(lambda:process.poll()is not None,'normal close absent',True);assert process.returncode==0 and 'FILES_CLOSE_DRAINED'in log.read_text()
            ids=set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=measure_directory',log.read_text()));assert len(ids)==(1 if args.sort or args.filters else 4 if args.sync or args.forget else 2)
            for pid,start in ids:
                try:current=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
                except FileNotFoundError:continue
                assert current[19]!=start,'logged helper identity still exists (including Z)'
            assert original=={str(p.relative_to(small)):hashlib.sha256(p.read_bytes()).hexdigest()for p in [small/'a',small/'sub/b']}
            report.update(functional_gate='PASS',normal_exit0_after_ack=True,observed_helper_identities_gone=len(ids),scope=report.get('scope')if args.forget or args.sync or args.sort or args.filters else 'typed automatic direct counts capped1024 without byte sums; empty/limited distinct; explicit recursive8B and0B, unknown not sorted as zero;4actualPNG and sourcebytes unchanged; not a resource/performance/kernelD/native-input proof')
        except BaseException as error:report['error']=repr(error);raise
        finally:
            if log and log.exists():report['log']=log.read_text()
            if process and process.poll()is None:
                process.terminate()
                try:process.wait(timeout=3)
                except subprocess.TimeoutExpired:process.kill();process.wait()
            for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=hashlib.sha256((ROOT/'target/release'/name).read_bytes()).hexdigest()
            path=OUT/(('pane-filter-ui-'if args.filters else'sort-controls-ui-'if args.sort else'size-sync-ui-'if args.sync else'size-forget-ui-'if args.forget else'direct-summary-ui-')+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)

if args.summaries or args.forget or args.sync or args.sort or args.filters:
    summaries()
    raise SystemExit(0)

process=None;normal=False
with tempfile.TemporaryDirectory(prefix='recursive-search-',dir=BASE) as temp:
    base=Path(temp);source=base/'Projects';source.mkdir();config=base/'config';config.mkdir();(config/'config.toml').write_text('show_hidden=false\n')
    small=source/'A-Small';(small/'sub').mkdir(parents=True)
    (small/'a').write_bytes(b'123');(small/'sub/b').write_bytes(b'12345')
    (small/'sub/cycle').symlink_to(small);(small/'broken').symlink_to('missing')
    large=source/'B-Large';large.mkdir()
    for i in range(40000):(large/f'f{i:05}').write_bytes(b'x')
    (source/'C-Empty').mkdir()
    rawname=b'D-Raw-\xff';rawdir=os.fsencode(source)+b'/'+rawname;os.mkdir(rawdir)
    with open(rawdir+b'/data','wb') as f:f.write(b'123456789')
    (source/'E-Changed').mkdir();(source/'Skip').mkdir();(source/'.hidden').mkdir()
    (source/'outside-link').symlink_to(base)
    raw=base/'capture.ppm';log=base/'app.log';seq=0
    def wait(check,reason,allow_exit=False):
        end=time.monotonic()+25
        while time.monotonic()<end:
            value=check()
            if value:return value
            if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exit '+str(process.returncode))
            time.sleep(.02)
        raise AssertionError(reason)
    def state(after=0,predicate=lambda s:True):
        def current():
            rows=[]
            for line in log.read_text()[after:].splitlines():
                if line.startswith('FILES_HIDDEN_STATE '):
                    s=json.loads(line[len('FILES_HIDDEN_STATE '):])
                    if s['seq']==seq and predicate(s):rows.append(s)
            return rows[-1] if rows else None
        s=wait(current,'painted state absent, seq '+str(seq));report['states'].append(s);return s
    def command(action):
        global seq
        offset=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);pending.replace(raw.with_suffix('.command'))
        match=wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',log.read_text()[offset:],re.M),'command absent '+action,action=='callback')
        seq=int(match[1]);return offset
    def act(action,predicate=lambda s:True):return state(command(action),predicate)
    def rows(s):return {bytes(r['name']):r for r in s['folder_usage']['rows']}
    def done(s):return not s['folder_usage']['queue_work'] and not s['folder_usage']['running']
    def capture(name):
        raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
        def full():
            if not raw.exists():return None
            parts=raw.read_bytes().split(b'\n',3)
            if len(parts)!=4:return None
            w,h=map(int,parts[1].split());return (w,h) if len(parts[3])==w*h*3 else None
        dims=wait(full,'incomplete framebuffer');path=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],check=True,timeout=15);report['captures'].append(dict(path=str(path),dimensions=dims))
    try:
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1','FILES_TEST_PREVIEW':'1'}
        with log.open('w') as output:
            process=subprocess.Popen([str(ROOT/'target/release/files'),str(source),'--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','120'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
        state();act('list');act('select-name:A-Small')
        s=act('size-hit',lambda s:done(s) and b'A-Small' in rows(s));assert rows(s)[b'A-Small']['label']=='∑ 8 B'
        capture('single-measured-counts')
        # The listed-directory hint is deliberately replaced before the actual helper opens it.
        (source/'E-Changed').rename(base/'preserved-directory');(source/'E-Changed').write_bytes(b'actor')
        act('filter-query:type:dir !skip');s=act('sizes-listed-paused')
        assert s['folder_usage']['paused'] and not s['folder_usage']['running']
        assert all(row['label']=='En pausa' for row in rows(s).values())
        act('clip-actions');capture('batch-paused-actions')
        act('clip-actions')
        s=act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
        assert s['folder_usage']['paused'] and not s['folder_usage']['running']
        capture('batch-paused-compact-status')
        act('fixture-wide',lambda s:s['width']==1000 and s['height']==560)
        if args.manager:
            act('clip-actions');s=act('manager-open');assert s['size_manager']['current'] and len(s['size_manager']['rows'])==5
            assert s['size_manager']['summary']['bytes'] is None and s['size_manager']['summary']['queued']==5
            helpers_before_report=log.read_text().count('command=measure_directory')
            s=act('manager-report');assert 'SIN MEDICIONES' in s['size_manager']['report']['text']
            frozen=s['size_manager']['report']['text'];s=act('manager-report-blocked-pause')
            assert s['size_manager']['report']['text']==frozen and s['folder_usage']['paused']
            assert not s['size_manager']['row_hits'] and not s['size_manager']['cancel_hit']
            capture('size-report-paused-not-zero')
            s=act('manager-report-copy');assert s['size_manager']['note'].startswith('Solicitud de copia')
            assert log.read_text().count('command=measure_directory')==helpers_before_report
            act('manager-escape')
            focus=s['selected'];s=act('manager-raw');row=s['size_manager']['rows'][s['size_manager']['selected']]
            old_raw=row['id'];assert bytes(row['name'])==rawname
            s=act('manager-cancel-hit');assert s['size_manager']['retry_hit'] and not s['size_manager']['cancel_hit']
            s=act('manager-retry-hit');new_raw=s['size_manager']['rows'][s['size_manager']['selected']]['id'];assert new_raw!=old_raw
            s=act('manager-old-cancel:'+str(old_raw));assert s['size_manager']['rows'][s['size_manager']['selected']]['label']=='En pausa'
            assert s['selected']==focus;capture('manager-paused-raw-retry-ticket')
            s=act('manager-resume-hit',done);assert not s['folder_usage']['paused']
            act('manager-end');s=act('manager-info');assert s['size_manager']['details'] and 'Error' in s['size_manager']['rows'][-1]['label']
            capture('manager-full-error-detail')
            act('manager-info');s=act('manager-report-key')
            assert s['size_manager']['summary']['bytes']==40017 and s['size_manager']['summary']['measured']==4 and s['size_manager']['summary']['failed']==1
            assert 'ERROR · sin total medido' in s['size_manager']['report']['text'] and 'MEDIDO · 0 bytes' in s['size_manager']['report']['text']
            capture('size-report-error-excluded-from-subtotal')
            act('manager-escape');act('manager-close')
        else:
            # Real command catalog selection + painted Execute, still private input.
            act('commands-shortcut');act('commands-query:sizes-resume');act('commands-row:sizes-resume')
            s=act('commands-run-hit');assert not s['folder_usage']['paused']
        act('select-name:C-Empty')
        s=state(predicate=done);r=rows(s)
        assert set(r)=={b'A-Small',b'B-Large',b'C-Empty',rawname,b'E-Changed'},r
        assert r[b'A-Small']['label']=='∑ 8 B' and r[b'C-Empty']['label']=='∑ 0 B' and r[rawname]['label']=='∑ 9 B',r
        assert r[b'E-Changed']['label']=='Error' and '40000' in r[b'B-Large']['detail'],r
        assert s['selected']==b'C-Empty'.hex();capture('batch-measured-and-error')
        progress=[row for line in log.read_text().splitlines() if line.startswith('FILES_HIDDEN_STATE ') for row in json.loads(line[len('FILES_HIDDEN_STATE '):])['folder_usage']['rows'] if row['label'].startswith('≥')]
        report['positive_partial_progress_observed']=any(row['label']!='≥0 B' and row['label']!='≥ 0 B' for row in progress)
        s=act('sizes-stop');assert rows(s)==r and done(s),'late stop must preserve completed measurements'
        act('sizes-sort');s=act('sizes-sort');assert s['order'][-1]==b'E-Changed'.hex();capture('measured-sort-unknown-last')
        s=act('grid');assert rows(s)==r;capture('grid-measured-sizes')
        act('list');act('fixture-short');act('select-name:A-Small');capture('small-window-measured-inspector')
        # Restore only the failed source, and retry errors without recalculating good rows.
        (source/'E-Changed').unlink();(base/'preserved-directory').rename(source/'E-Changed')
        (source/'E-Changed'/'restored').write_bytes(b'1234567')
        before={name:row['label'] for name,row in r.items() if name!=b'E-Changed'}
        helpers_before=len(re.findall(r'FILES_PREVIEW_HELPER pid=\d+ start=\d+ command=measure_directory',log.read_text()))
        if args.manager:
            act('clip-actions');act('manager-open');act('manager-end');s=act('manager-retry-hit',done)
        else:
            act('commands-shortcut');act('commands-query:sizes-retry');act('commands-row:sizes-retry')
            s=act('commands-run-hit',done)
        assert rows(s)[b'E-Changed']['label']=='∑ 7 B'
        assert {name:row['label'] for name,row in rows(s).items() if name!=b'E-Changed'}==before
        assert len(re.findall(r'FILES_PREVIEW_HELPER pid=\d+ start=\d+ command=measure_directory',log.read_text()))==helpers_before+1
        capture('failed-only-retry-preserves-good-results')
        if args.manager:
            assert not s['size_manager']['retry_hit'];capture('manager-retry-disabled-after-success')
            s=act('manager-report');snapshot=s['size_manager']['report']['text']
            assert s['size_manager']['summary']['bytes']==40024 and s['size_manager']['summary']['measured']==5
            assert '\\xFF' in snapshot or '\\xff' in snapshot
            assert 'Subtotal medido: 40024 bytes' in snapshot and 'ni deduplica hardlinks' in snapshot
            s=act('manager-end');assert s['size_manager']['detail_top']>0
            capture('size-report-observations-end')
            s=act('manager-report-refresh');assert s['size_manager']['detail_top']==0 and s['size_manager']['report']['text']!=snapshot
            act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
            s=state();assert s['size_manager']['copy_report_hit']
            for x,y,w,h in s['size_manager']['control_rects']:assert x>=12 and y>=66 and x+w<=428 and y+h<=400
            capture('size-report-initial-440')
            act('manager-escape');act('fixture-wide')
            original=s['size_manager']['rows'];selected_id=original[s['size_manager']['selected']]['id']
            original_ids=[row['id'] for row in original]
            s=act('manager-order');assert s['size_manager']['display_order']=='Mayor'
            expected=[r['id'] for r in sorted(original,key=lambda r:({b'B-Large':0,b'D-Raw-\xff':1,b'A-Small':2,b'E-Changed':3,b'C-Empty':4}[bytes(r['name'])]))]
            assert s['size_manager']['display_ids']==expected
            assert s['size_manager']['rows'][s['size_manager']['selected']]['id']==selected_id
            assert [r['id'] for r in s['size_manager']['rows']]==original_ids
            capture('manager-largest-first-selection-preserved')
            s=act('manager-home');assert bytes(s['size_manager']['rows'][s['size_manager']['selected']]['name'])==b'B-Large'
            capture('manager-largest-first-home')
            s=act('manager-order-key');assert s['size_manager']['display_order']=='Nombre'
            assert s['size_manager']['display_ids']==[r['id'] for r in sorted(original,key=lambda r:bytes(r['name']))]
            act('manager-end');s=act('manager-info');assert s['size_manager']['details']
            assert bytes(s['size_manager']['rows'][s['size_manager']['selected']]['name'])==b'E-Changed'
            capture('manager-name-order-selected-detail')
            act('manager-info');s=act('manager-order');assert s['size_manager']['display_order']=='Cola'
            assert s['size_manager']['display_ids']==original_ids

            # Refresh only the selected completed observation, not the entire listing.
            s=act('manager-raw');selected=s['size_manager']['rows'][s['size_manager']['selected']];old_id=selected['id']
            untouched={bytes(v['name']):(v['id'],v['label']) for v in s['size_manager']['rows'] if bytes(v['name'])!=rawname}
            helper_count=log.read_text().count('command=measure_directory')
            with open(rawdir+b'/data','wb') as f:f.write(b'1'*19)
            s=act('manager-recalculate-key',lambda s:done(s) and rows(s)[rawname]['label']=='∑ 19 B' and s['size_manager']['recalculate_id']==next(v['id'] for v in s['size_manager']['rows'] if bytes(v['name'])==rawname))
            assert next(v['id'] for v in s['size_manager']['rows'] if bytes(v['name'])==rawname)!=old_id and s['size_manager']['recalculate_hit'] and not s['size_manager']['retry_hit']
            assert {bytes(v['name']):(v['id'],v['label']) for v in s['size_manager']['rows'] if bytes(v['name'])!=rawname}==untouched
            assert log.read_text().count('command=measure_directory')==helper_count+1
            assert s['size_manager']['summary']['bytes']==40034
            capture('size-recalculate-key-preserves-other-observations')
            act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
            capture('size-recalculate-button-440');s=state();old_id=next(v['id'] for v in s['size_manager']['rows'] if bytes(v['name'])==rawname)
            with open(rawdir+b'/data','wb') as f:f.write(b'2'*29)
            s=act('manager-recalculate-hit',lambda s:done(s) and rows(s)[rawname]['label']=='∑ 29 B' and s['size_manager']['recalculate_id']==next(v['id'] for v in s['size_manager']['rows'] if bytes(v['name'])==rawname))
            assert next(v['id'] for v in s['size_manager']['rows'] if bytes(v['name'])==rawname)!=old_id and s['size_manager']['rows'][s['size_manager']['selected']]['id']==next(v['id'] for v in s['size_manager']['rows'] if bytes(v['name'])==rawname)
            assert {bytes(v['name']):(v['id'],v['label']) for v in s['size_manager']['rows'] if bytes(v['name'])!=rawname}==untouched
            assert log.read_text().count('command=measure_directory')==helper_count+2 and s['size_manager']['summary']['bytes']==40044
            capture('size-recalculate-button-measured-440');act('fixture-wide')
            with open(rawdir+b'/data','wb') as f:f.write(b'123456789')
            report['recalculate_scope']='PASS selected raw folder 9→19 via F5→29 via painted button440, fresh attempt IDs/selection retained, exactly two helper admissions and all other observations/IDs unchanged. Unit proof pause/active ACK/stale ID/context/report guard; no inputSO/resource/snapshot proof.'
            act('manager-report');old_stamp=s['size_manager']['stamp'];s=act('manager-refresh');assert not s['size_manager']['current'] and s['size_manager']['row_hits']==0
            assert not s['size_manager']['copy_report_hit']
            capture('manager-stale-owner-controls-disabled')
            s=act('manager-recapture-hit');assert s['size_manager']['current'] and s['size_manager']['stamp']!=old_stamp
            s=act('manager-enqueue-paused');assert s['folder_usage']['paused'] and len(s['size_manager']['rows'])==5
            s=act('manager-old-stop:'+str(old_stamp));assert all(row['label']=='En pausa' for row in s['size_manager']['rows'])
            s=act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
            for x,y,w,h in s['size_manager']['control_rects']:assert x>=12 and y>=66 and x+w<=428 and y+h<=400
            capture('manager-controls-initial-440')
            act('manager-stop-hit');act('manager-close');act('fixture-short')
        else:
            act('commands-shortcut');s=act('commands-query:sizes-retry')
            assert not s['commands']['run_hit'], 'no failed rows must disable retry'
            capture('retry-disabled-without-errors')
            act('commands-key:escape')
        s=act('sizes-refresh',lambda s:done(s) and not rows(s));assert not rows(s)
        (small/'added').write_bytes(b'12345')
        act('select-name:A-Small');s=act('size-key',lambda s:done(s) and b'A-Small' in rows(s));assert rows(s)[b'A-Small']['label']=='∑ 13 B';capture('refresh-and-recalculate')
        command('sizes-listed');s=act('back',lambda s:bytes(s['path_raw'])==os.fsencode(base) and done(s));assert not rows(s)
        command('callback');wait(lambda:process.poll() is not None,'normal close missing',True)
        text=log.read_text();assert process.returncode==0 and 'FILES_CLOSE_DRAINED' in text
        identities=set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=measure_directory',text));assert identities
        for pid,start in identities:
            try:
                parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
            except FileNotFoundError:continue
            assert parts[19]!=start,'helper identity still exists, including zombie: '+pid
        assert (source/'E-Changed'/'restored').read_bytes()==b'1234567'
        assert (small/'a').read_bytes()==b'123' and (small/'sub/b').read_bytes()==b'12345'
        assert len(list(large.iterdir()))==40000 and (large/'f00000').read_bytes()==b'x' and (large/'f39999').read_bytes()==b'x'
        assert all(not s['error'] for s in report['states']),[(s['seq'],s['error']) for s in report['states'] if s['error']]
        report['size_manager']=args.manager
        if args.manager:report['size_report_scope']='completed-only checked subtotal; queued unknown distinct from measured zero; failed row excluded, exact 40017/40024 bytes; frozen capture/F5/scroll/I/Esc, raw escaped names, copy-request handler only/no clipboard delivery; report no extra helper, stale context disables copy and offers explicit recapture, controls440'
        if args.manager:report['manager_scope']='display order button/S Queue/Largest/Name, all5 IDs checked and queue unchanged, selected ID preserved, Home to largest, Name-End detail correct; borrowed same queue, raw cancelled/retried before admission and old row ID rejected, file focus unchanged, actual error/detail and single failed retry, no retry after success, refresh invalidates context/row hits, recapture rejects old panel Stop on fresh paused batch, controls440; active-cancel ACK is unit proof, no native input'
        report.update(functional_gate='PASS',normal_exit=True,logged_helper_identities=sorted(identities),helper_cleanup='logged PID/start identities absent or reused; Z not accepted; not lifetime coverage',scope='pause-before-admission, catalog resume and failed-only retry preserving good rows, retry disabled without errors, single/batch measured sizes, error not zero, filtered and hidden scope, raw directory, selection independent, late stop, sort, Grid and 800x440, refresh/recalculate, navigation invalidation and callback drain; no native input/resource/startup/snapshot proof');normal=True
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['log']=log.read_text() if log.exists() else ''
        if process is not None and process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=hashlib.sha256((ROOT/'target/release'/name).read_bytes()).hexdigest()
        target=OUT/(('size-manager-ui-' if args.manager else 'folder-sizes-ui-')+str(time.time_ns())+'.json')
        with target.open('x') as output:json.dump(report,output,ensure_ascii=False,indent=2)
        print(target)
