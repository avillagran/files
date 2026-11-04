#!/usr/bin/env python3
"""Owned copy conflict policy evidence; production handlers/framebuffer, no OS input or resource sampling."""
import argparse,fcntl,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--allocated',action='store_true');parser.add_argument('--marked-report',action='store_true');parser.add_argument('--marked',action='store_true');parser.add_argument('--installed',action='store_true');args=parser.parse_args()
if args.allocated:args.marked_report=True
if args.marked_report:args.marked=True
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'docs/evidence/sizes-open';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report=dict(functional_gate='IN_PROGRESS',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',method='owned filesystem fixtures and independent Files processes; painted handlers/private command pipes and real FLTK framebuffer; no native input',states=[],captures=[],processes=[])
BINARY=Path.home()/'.local/bin/files' if args.installed else ROOT/'target/release/files'
apps=[]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
class App:
    def __init__(self,label,folder,base,config,place,size='1000x650'):
        self.label=label;self.seq=0;self.raw=base/(label+'.ppm');self.log=base/(label+'.log')
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_BOOKMARK_PLACE':str(place),'FILES_CLOSE_TRACE':'1','FILES_TEST_PREVIEW':'1'}
        with self.log.open('w') as output:
            self.process=subprocess.Popen([str(BINARY),str(folder),'--ui-smoke','--ui-scene','hidden-context','--size',size,'--scale','1','--exit-after','180'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
        apps.append(self);report['processes'].append(dict(label=label,pid=self.process.pid,root_raw=list(os.fsencode(folder))))
        self.state(predicate=lambda s:s['bookmarks']['browser_loaded']);self.act('list');self.state(predicate=lambda s:s['bookmarks']['browser_loaded'] and s.get('listing_watch',{}).get('settled',True))
    def wait(self,check,why,exits=False):
        end=time.monotonic()+25
        while time.monotonic()<end:
            value=check()
            if value:return value
            if self.process.poll() is not None and not exits:raise AssertionError(self.label+': '+why+' exit '+str(self.process.returncode))
            time.sleep(.02)
        raise AssertionError(self.label+': '+why)
    def state(self,after=0,predicate=lambda s:True):
        def current():
            states=[]
            for line in self.log.read_text()[after:].splitlines(keepends=True):
                if not line.endswith('\n'):continue
                if line.startswith('FILES_HIDDEN_STATE '):
                    state=json.loads(line[len('FILES_HIDDEN_STATE '):])
                    painted=True
                    if state['seq']==self.seq and painted and predicate(state):states.append(state)
            return states[-1] if states else None
        s=self.wait(current,'painted state absent seq '+str(self.seq));report['states'].append(dict(app=self.label,state=s));return s
    def command(self,action,exits=False):
        offset=len(self.log.read_text());pending=self.raw.with_suffix('.pending');pending.write_text(action);pending.replace(self.raw.with_suffix('.command'))
        match=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',self.log.read_text()[offset:],re.M),'command absent '+action,exits)
        self.seq=int(match[1]);return offset
    def act(self,action,predicate=lambda s:True):return self.state(self.command(action),predicate)
    def capture(self,name):
        self.raw.unlink(missing_ok=True);self.raw.with_suffix('.request').touch()
        def full():
            if not self.raw.exists():return None
            parts=self.raw.read_bytes().split(b'\n',3)
            if len(parts)!=4:return None
            w,h=map(int,parts[1].split());return (w,h)if len(parts[3])==w*h*3 else None
        dims=self.wait(full,'incomplete framebuffer');path=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(self.raw),str(path)],check=True,timeout=15);report['captures'].append(dict(app=self.label,path=str(path),dimensions=dims))
    def close(self):
        self.command('callback',True);self.wait(lambda:self.process.poll() is not None,'normal close absent',True);assert self.process.returncode==0 and 'FILES_CLOSE_DRAINED' in self.log.read_text()


with tempfile.TemporaryDirectory(prefix='sizes-open-',dir=BASE) as temp:
    base=Path(temp);config=base/'config';config.mkdir();source=base/'Projects';source.mkdir()
    for name,n in [('A-Small',8),('B-Large',32),('C-Empty',0),(os.fsdecode(b'D-Raw-\xff'),9)]:
        folder=source/name;folder.mkdir()
        if n:(folder/'data').write_bytes(bytes(range(n)))
    if args.marked:
        (source/'E-File').write_bytes(b'not a directory')
        (source/'F-Link').symlink_to('B-Large')
        (source/r'D-Raw-\xFF').mkdir();(source/r'D-Raw-\xFF'/'data').write_bytes(b'literal raw')
    original={os.fsencode(p.relative_to(source)):sha(p)for p in source.rglob('data')}
    def done(s):return not s['folder_usage']['queue_work'] and not s['folder_usage']['running']
    try:
        a=App('sizes-open',source,base,config,source)
        if args.marked:
            a.act('select-name:A-Small');a.act('mark-next')
            count=len(re.findall(r'FILES_PREVIEW_HELPER pid=',a.log.read_text()))
            s=a.act('sizes-marked');assert s['error'] and not s['folder_usage']['rows'] and not s['folder_usage']['queue_work']
            assert len(re.findall(r'FILES_PREVIEW_HELPER pid=',a.log.read_text()))==count;a.capture('marked-empty-does-not-measure-focus')
            a.act('select-name:A-Small');a.act('toggle-raw-name');a.act('toggle-name:E-File');a.act('toggle-name:F-Link');a.act('toggle-name:B-Large');s=a.act('mark-next')
            marks=s['selection'];focus=s['selected'];assert focus==b'C-Empty'.hex() and b'B-Large'.hex() not in marks
            a.act('commands-shortcut');a.act('commands-query:marcadas');a.act('commands-row:sizes-marked')
            s=a.act('commands-key:enter',lambda s:done(s) and s['size_manager']is not None and s['size_manager']['row_hits']>0)
            assert s['selection']==marks and s['selected']==focus
            assert {bytes(row['name']):row['label']for row in s['size_manager']['rows']}=={b'A-Small':'∑ 8 B',b'D-Raw-'+bytes([255]):'∑ 9 B'}
            assert s['size_manager']['summary']['bytes']==17 and len(s['folder_usage']['rows'])==2
            ids=[row['id']for row in s['size_manager']['rows']];a.capture('marked-two-folders-exclude-unmarked-focus-file-and-link')
            s=a.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);rects=s['size_manager']['control_rects']
            assert s['size_manager']['marked_hit'] and all(x>=12 and y>=66 and w>0 and h>0 and x+w<=428 and y+h<=400 for x,y,w,h in rects)
            for i,a_rect in enumerate(rects):
                for b_rect in rects[i+1:]:
                    x,y,w,h=a_rect;xx,yy,ww,hh=b_rect;assert x+w<=xx or xx+ww<=x or y+h<=yy or yy+hh<=y
            a.capture('marked-manager-controls-fit-440')
            s=a.act('manager-marked-key',lambda s:done(s) and s['size_manager']['row_hits']>0)
            assert ids!=[row['id']for row in s['size_manager']['rows']] and s['size_manager']['summary']['bytes']==17
            assert s['selection']==marks and s['selected']==focus;a.capture('marked-key-recalculates-exact-marks')
            s=a.act('manager-enqueue-paused',lambda s:s['folder_usage']['paused'] and s['folder_usage']['queue_work'] and len(s['size_manager']['control_rects'])>4)
            old=[row['id']for row in s['size_manager']['rows']];count=len(re.findall(r'FILES_PREVIEW_HELPER pid=',a.log.read_text()))
            s=a.act('manager-marked-key');assert s['size_manager']['error'] and s['folder_usage']['paused'] and s['folder_usage']['queue_work']
            assert [row['id']for row in s['size_manager']['rows']]==old and not s['size_manager']['marked_hit']
            assert len(re.findall(r'FILES_PREVIEW_HELPER pid=',a.log.read_text()))==count;a.capture('marked-busy-rejection-preserves-paused-batch')
            a.act('manager-stop-hit',done);a.act('manager-close');a.act('fixture-wide',lambda s:s['width']==1000)
            report['marked_scope']='real command palette and managerM; only visible marked directories measured, rawname exact, unmarked focus/file/symlink excluded, selection/focus unchanged, no focus fallback,17B subtotal, min440 positive nonoverlap hit rects, explicit recalculation fresh IDs, paused busy rejection no new helper/no cancellation; source bytes intact'
        a.act('sizes-listed',done);a.act('clip-actions');s=a.act('manager-open');assert s['size_manager']['open_hit']
        if args.marked_report:
            helpers=len(re.findall(r'FILES_PREVIEW_HELPER pid=',a.log.read_text()))
            old={bytes(r['name']):r['id']for r in s['size_manager']['rows']};assert len(old)==5
            s=a.act('manager-report');assert s['size_manager']['summary']['bytes']==60 and not s['size_manager']['report_marked'];a.capture('marked-report-all-queue-60B')
            s=a.act('manager-report-scope-hit');assert s['size_manager']['report_marked'] and s['size_manager']['summary']['bytes']==17 and len(s['size_manager']['report_ids'])==2
            text=s['size_manager']['report']['text'];assert 'Alcance: carpetas marcadas visibles'in text and 'Nombre: "B-Large"'not in text and 'sin observación: 0'in text
            assert len(re.findall(r'FILES_PREVIEW_HELPER pid=',a.log.read_text()))==helpers;a.capture('marked-report-subtotal-17B-excludes-unselected-observations')
            s=a.act('fixture-minimum',lambda s:s['width']==440);rects=s['size_manager']['control_rects']
            assert all(x>=12 and y>=66 and w>0 and h>0 and x+w<=428 and y+h<=400 for x,y,w,h in rects)
            for i,(x,y,w,h)in enumerate(rects):
                for xx,yy,ww,hh in rects[i+1:]:assert x+w<=xx or xx+ww<=x or y+h<=yy or yy+hh<=y
            a.capture('marked-report-scope-and-forget-controls-440')
            s=a.act('manager-report-refresh');assert s['size_manager']['report_marked'] and s['size_manager']['summary']['bytes']==17
            s=a.act('manager-forget-report-hit');assert s['size_manager']['report_marked'] and s['size_manager']['summary']['bytes']is None and s['size_manager']['summary']['unobserved']==2 and not s['size_manager']['report_ids']
            remaining={bytes(r['name']):r['id']for r in s['size_manager']['rows']}
            assert remaining=={n:i for n,i in old.items()if n not in [b'A-Small',b'D-Raw-'+bytes([255])]}
            assert 'SIN OBSERVACIÓN'in s['size_manager']['report']['text'] and 'SIN MEDICIONES'in s['size_manager']['report']['text'];a.capture('marked-report-forget-only-captured-ids-missing-not-zero')
            s=a.act('manager-report-scope-hit');assert not s['size_manager']['report_marked'] and s['size_manager']['summary']['bytes']==43 and s['size_manager']['summary']['measured']==3
            assert len(re.findall(r'FILES_PREVIEW_HELPER pid=',a.log.read_text()))==helpers;a.capture('marked-report-all-unselected-43B-retained')
            a.act('manager-report');s=a.act('manager-report-marked-key');assert s['size_manager']['report_marked'] and s['size_manager']['summary']['unobserved']==2 and s['size_manager']['summary']['bytes']is None
            assert len(re.findall(r'FILES_PREVIEW_HELPER pid=',a.log.read_text()))==helpers
            a.act('manager-report');a.act('fixture-wide',lambda s:s['width']==1000)
            report['marked_report_scope']='all queue60B vs marked17B; raw-name subset excludes unselected observations, immutable scoped refresh, scope switch and Forget controlsfit440; Forget uses only two captured measured IDs, three unselected IDs/43B remain; forgotten marked rows explicitly unobserved/not0, no new helper and filesystem bytes retained'
        a.act('manager-order');s=a.act('manager-home');row=s['size_manager']['rows'][s['size_manager']['selected']]
        assert bytes(row['name'])==b'B-Large' and row['label']=='∑ 32 B'
        a.capture('sizes-largest-selected-open-control')
        s=a.act('manager-open-key',lambda s:s['bookmarks']['browser_loaded'] and bytes(s['path_raw'])==os.fsencode(source/'B-Large'))
        assert s['size_manager'] is None and not s['folder_usage']['rows'] and done(s)
        a.capture('sizes-alt-enter-largest-folder')
        a.act('history-back',lambda s:s['bookmarks']['browser_loaded'] and bytes(s['path_raw'])==os.fsencode(source))
        a.act('sizes-listed',done);a.act('clip-actions');a.act('manager-open');s=a.act('manager-refresh',lambda s:s['bookmarks']['browser_loaded'] and not s['size_manager']['current'])
        assert not s['size_manager']['current'] and not s['size_manager']['open_hit']
        a.capture('sizes-stale-context-open-disabled')
        a.act('manager-recapture-hit');a.act('manager-close');a.act('sizes-listed',done);a.act('clip-actions');a.act('manager-open');a.act('manager-raw')
        a.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
        a.capture('sizes-raw-open-control-440')
        s=a.act('manager-open-folder',lambda s:s['bookmarks']['browser_loaded'] and bytes(s['path_raw'])==os.fsencode(source)+b'/D-Raw-\xff')
        assert s['size_manager'] is None and done(s);a.capture('sizes-raw-folder-opened-440')
        a.close()
        if args.allocated:
            allocation=base/'Allocation';allocation.mkdir()
            for name in ['A-Allocated','B-Shared','C-Empty',os.fsdecode(b'D-Raw-\xff')]: (allocation/name).mkdir()
            data=allocation/'A-Allocated/data';data.write_bytes(b'x'*4096)
            os.link(data,allocation/'A-Allocated/alias');os.link(data,allocation/'B-Shared/shared')
            sparse=allocation/'A-Allocated/sparse'
            with sparse.open('wb')as file:file.truncate(1024*1024)
            raw=allocation/os.fsdecode(b'D-Raw-\xff')/'data';raw.write_bytes(b'raw')
            (allocation/'E-File').write_bytes(b'file');(allocation/'F-Link').symlink_to('A-Allocated')
            before={os.fsencode(p.relative_to(allocation)):sha(p) for p in [data,allocation/'A-Allocated/alias',allocation/'B-Shared/shared',sparse,raw]}
            def values(folder):
                byname=0;seen={}
                for path in folder.iterdir():
                    stat=path.lstat()
                    if path.is_file() and not path.is_symlink():byname+=stat.st_blocks*512;seen[(stat.st_dev,stat.st_ino)]=stat.st_blocks*512
                return {'by_name':{'state':'bytes','value':byname},'by_identity':{'state':'bytes','value':sum(seen.values())}}
            expected_a=values(allocation/'A-Allocated');expected_shared=values(allocation/'B-Shared');expected_raw=values(raw.parent)
            allocation_config=base/'allocation-config';allocation_config.mkdir()
            b=App('allocation',allocation,base,allocation_config,allocation);b.act('selection-clear')
            count=len(re.findall(r'FILES_PREVIEW_HELPER pid=',b.log.read_text()))
            s=b.act('sizes-marked-allocated');assert s['error'] and not s['folder_usage']['rows']
            assert len(re.findall(r'FILES_PREVIEW_HELPER pid=',b.log.read_text()))==count
            b.act('select-name:A-Allocated');b.act('commands-shortcut');b.act('commands-query:bloques');b.act('commands-row:sizes-blocks')
            s=b.act('commands-key:enter',lambda s:done(s) and s['size_manager']is not None and s['size_manager']['row_hits']>0)
            row=s['size_manager']['rows'][0];assert row['mode']=='allocated' and row['allocation']==expected_a
            assert 'Bloques por nombre:'in row['detail'];first=row['id'];b.capture('allocated-focused-sparse-and-hardlinks')
            b.act('manager-info');b.capture('allocated-details-regular-file-blocks');b.act('manager-info')
            s=b.act('manager-blocks-hit',lambda s:done(s) and s['size_manager']['rows'][0]['id']!=first)
            assert s['size_manager']['rows'][0]['allocation']==expected_a
            b.act('manager-close');b.act('selection-clear');b.act('toggle-name:A-Allocated');b.act('toggle-raw-name');b.act('toggle-name:E-File');b.act('toggle-name:F-Link');b.act('toggle-name:B-Shared');b.act('toggle-name:B-Shared');s=b.act('mark-next');assert s['selected']==b'C-Empty'.hex()
            b.act('clip-actions');b.act('manager-open');s=b.act('manager-marked-blocks-key',lambda s:done(s) and len(s['size_manager']['rows'])==3)
            rows={bytes(r['name']):r for r in s['size_manager']['rows']}
            assert set(rows)=={b'A-Allocated',b'B-Shared',b'D-Raw-'+bytes([255])}
            assert rows[b'A-Allocated']['allocation']==expected_a and rows[b'B-Shared']['allocation']==expected_shared and rows[b'D-Raw-'+bytes([255])]['allocation']==expected_raw
            assert all(r['mode']=='allocated'for r in rows.values());b.capture('allocated-marked-excludes-focus-file-link')
            oldids={name:r['id']for name,r in rows.items()}
            s=b.act('manager-blocks-key',lambda s:done(s) and s['size_manager']['rows'][0]['id']!=oldids[b'A-Allocated'])
            fresh={bytes(r['name']):r for r in s['size_manager']['rows']}
            assert fresh[b'A-Allocated']['allocation']==expected_a and s['selected']==b'C-Empty'.hex()
            assert all(fresh[name]['id']==oldids[name]for name in oldids if name!=b'A-Allocated')
            b.capture('allocated-row-key-does-not-use-browser-focus')
            s=b.act('fixture-minimum',lambda s:s['width']==440);rects=s['size_manager']['control_rects']
            assert s['size_manager']['blocks_hit'] and s['size_manager']['marked_blocks_hit'] and len(rects)>=9 and all(x>=12 and y>=66 and w>0 and h>0 and x+w<=428 and y+h<=400 for x,y,w,h in rects)
            for i,(x,y,w,h)in enumerate(rects):
                for xx,yy,ww,hh in rects[i+1:]:assert x+w<=xx or xx+ww<=x or y+h<=yy or yy+hh<=y
            b.capture('allocated-nine-controls-fit-440')
            helpers=len(re.findall(r'FILES_PREVIEW_HELPER pid=',b.log.read_text()))
            s=b.act('manager-report');text=s['size_manager']['report']['text']
            assert f"Bloques por nombre: {expected_a['by_name']['value']} bytes"in text and f"Por identidad en esta carpeta: {expected_a['by_identity']['value']} bytes"in text
            assert 'no es una unión entre carpetas'in text;b.capture('allocated-report-values-and-root-limits-440')
            b.act('manager-end');b.capture('allocated-report-scroll-raw-values-440')
            s=b.act('manager-blocks-key');assert s['size_manager']['report']['text']==text
            assert len(re.findall(r'FILES_PREVIEW_HELPER pid=',b.log.read_text()))==helpers
            s=b.act('manager-forget-report-hit');assert not s['size_manager']['rows'] and not s['size_manager']['report_ids']
            assert len(re.findall(r'FILES_PREVIEW_HELPER pid=',b.log.read_text()))==helpers
            b.close();assert before=={os.fsencode(p.relative_to(allocation)):sha(p)for p in [data,allocation/'A-Allocated/alias',allocation/'B-Shared/shared',sparse,raw]}
            report['allocation_scope']='actual allocated helper via focused catalog, selected-row mouse control and marked Shift+B; sparse/hardlink expectations from owned metadata, raw names, excludes unmarked focus/file/link;9controls fit440; captured report exact per-root allocations/no union, B from report no helper, scoped Forget preserves files'
        assert original=={os.fsencode(p.relative_to(source)):sha(p)for p in source.rglob('data')}
        if args.marked:assert (source/'C-Empty').is_dir() and (source/'E-File').read_bytes()==b'not a directory' and (source/'F-Link').is_symlink() and os.readlink(source/'F-Link')=='B-Large'
        identities=set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=measure_directory', '\n'.join(app.log.read_text()for app in apps)))
        assert identities,'no logged measurement helper identities'
        for pid,start in identities:
            try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
            except FileNotFoundError:continue
            assert parts[19]!=start,'helper identity still exists, including zombie: '+pid
        report['logged_measure_helpers']=sorted(identities)
        report.update(functional_gate='PASS',normal_exits=2 if args.allocated else 1,scope='4folders completed with existing helper; largest sort+Home selects32B; AltEnter opens selectedfolder and invalidates sizes; historyback; stale refresh disables open control; recapture/recalculate; raw0xFF folder opened via painted Nerdbutton at440; source bytes intact; root0drainACK. Private handlers/real framebuffer, no native input/resource/helper-lifetime claim.')
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['logs']={app.label:app.log.read_text() for app in apps}
        for app in apps:
            if app.process.poll() is None:
                os.killpg(app.process.pid,signal.SIGTERM)
                try:app.process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha((Path.home()/'.local/bin'/name) if args.installed else ROOT/'target/release'/name)
        path=OUT/('sizes-open-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
