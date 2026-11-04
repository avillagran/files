#!/usr/bin/env python3
"""Owned incremental search evidence; production handlers/framebuffer, no OS input or resource sampling."""
import argparse,fcntl,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--background',action='store_true');parser.add_argument('--marks',action='store_true');parser.add_argument('--transfer',action='store_true');parser.add_argument('--retry',action='store_true');parser.add_argument('--reports',action='store_true');parser.add_argument('--saved',action='store_true');parser.add_argument('--pages',action='store_true');parser.add_argument('--scope',action='store_true');args=parser.parse_args()
if args.reports and not args.retry:parser.error('--reports requires --retry')
if args.retry and not args.transfer:parser.error('--retry requires --transfer')
if args.transfer and not args.marks:parser.error('--transfer requires --marks')
if args.marks and not args.background:parser.error('--marks requires --background')
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'docs/evidence/search-order';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report=dict(functional_gate='IN_PROGRESS',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',method='owned filesystem fixtures and independent Files processes; painted handlers/private command pipes and real FLTK framebuffer; no native input',states=[],captures=[],processes=[])
apps=[]
runtime_owned=None
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
class App:
    def __init__(self,label,folder,base,config,place,size='1000x650'):
        self.label=label;self.seq=0;self.raw=base/(label+'.ppm');self.log=base/(label+'.log')
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_BOOKMARK_PLACE':str(place),'FILES_CLOSE_TRACE':'1','FILES_TEST_PREVIEW':'1','FILES_TEST_SEARCH_HOLD_FIRST_BATCH':'1'}
        with self.log.open('w') as output:
            self.process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--ui-smoke','--ui-scene','hidden-context','--size',size,'--scale','1','--exit-after','180'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
        apps.append(self);report['processes'].append(dict(label=label,pid=self.process.pid,root_raw=list(os.fsencode(folder))))
        self.state(predicate=lambda s:s['bookmarks']['browser_loaded']);self.act('list')
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
            for line in self.log.read_text()[after:].splitlines():
                if line.startswith('FILES_HIDDEN_STATE '):
                    state=json.loads(line[len('FILES_HIDDEN_STATE '):])
                    q=state.get('search',{})
                    population=q.get('report',{}).get('hits',[]) if q.get('report')is not None else q.get('provisional')or[]
                    saved=state.get('saved_searches',{});panel=saved.get('panel')
                    criteria_painted=q.get('filters_open')and q.get('criteria_painted')and len(q.get('filter_targets',[]))==4 and all(x>=12 and x+w<=state['width']-12 and y+h<=state['height']-144 for x,y,w,h in q['filter_targets'])
                    painted=(q.get('scope_menu')and q.get('scope_controls',0)>0) or criteria_painted or panel is not None or state.get('destination_editor')is not None or bool(state.get('copy_result')and state['copy_result']['notice'])or not q.get('visible')or not population or (q.get('row_hits',0)>0 and len(q.get('display_paths',[]))==len(population))
                    view=state.get('notice_view',{});nav=view.get('navigation',[])
                    notice_painted=not args.reports or not view.get('open')or (('Start'in nav)==(view['scroll']>0)and ('End'in nav)==(view['scroll']<max(0,view['total']-view['rows'])))
                    saved_painted=panel is None or (panel['input_painted']and bool(saved['buttons'])and all(b['stamp']==panel['stamp']for b in saved['buttons']))
                    search_ready=not(args.saved or args.pages) or not q.get('visible')or panel is not None or state.get('destination_editor')is not None or bool(state.get('copy_result')and state['copy_result']['notice'])or(q.get('header_painted')and q.get('input_painted'))
                    scope_ready=not args.scope or not q.get('visible')or(q.get('scope_controls',0)>0 if q.get('scope_menu')else q.get('scope_root_painted'))
                    if state['seq']==self.seq and painted and notice_painted and saved_painted and search_ready and scope_ready and predicate(state):states.append(state)
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



def check_pages(base,config,target):
    def root(label,count,raw=False):
        folder=base/label;folder.mkdir()
        for i in range(count-int(raw)):(folder/f'page-{i:04}.txt').write_bytes(b'owned page '+str(i).encode())
        if raw:
            with open(os.fsencode(folder)+b'/page-\xff.txt','wb')as out:out.write(b'owned raw')
        return folder
    def ready(sequence):
        return lambda s:s['search_pages']['current']==sequence and not s['search_pages']['loading']and s['search']['report']is not None and s['search']['row_hits']>0
    def paths(s):return {bytes(h['relative'])if isinstance(h['relative'],list)else os.fsencode(h['relative'])for h in s['search']['report']['hits']}
    def open_pages(label,folder):
        app=App(label,folder,base,config,target);app.act('recursive-open');app.act('recursive-key:ctrlf');app.act('recursive-text:ext:txt');return app
    def live(app):
        ids=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=search_pages',app.log.read_text());assert ids
        pid,start=ids[-1];stat=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split();assert stat[19]==start and stat[0]!='Z';return [pid,start]
    folder=root('Pages1207',1207,True);before={os.fsencode(f.name):sha(f)for f in folder.iterdir()}
    p=open_pages('pages-1207',folder);s=p.act('recursive-pages',ready(0));assert len(paths(s))==500 and s['search']['running']and not s['search']['report']['complete']
    first=paths(s);epoch=s['search_pages']['epoch'];live(p);p.capture('search-pages-first-window-live')
    p.act('recursive-key:down');s=p.act('recursive-key:down');selected=bytes(s['search']['report']['hits'][s['search']['selected']]['relative'])if isinstance(s['search']['report']['hits'][s['search']['selected']]['relative'],list)else os.fsencode(s['search']['report']['hits'][s['search']['selected']]['relative'])
    p.act('recursive-mark:'+str(s['search']['selected']));s=p.act('recursive-key:f5',lambda s:s['destination_editor']is not None and not s['search']['capture_busy'])
    assert s['destination_editor']['multi_roots']==1;p.capture('search-pages-current-copy-review');p.act('clip-cancel-dialog');assert not list(target.iterdir())
    s=p.act('recursive-key:page-next',ready(1));second=paths(s);assert len(second)==500 and not first&second and not s['search']['marked'];p.capture('search-pages-second-window')
    s=p.act('recursive-page-old-next:'+str(epoch));assert s['search_pages']['current']==1 and not s['search_pages']['loading']
    credits=p.log.read_text().count('FILES_SEARCH_PAGE_CREDIT')
    s=p.act('recursive-page-prev',ready(0));assert os.fsencode(s['search']['report']['hits'][s['search']['selected']]['relative'])==selected if isinstance(s['search']['report']['hits'][s['search']['selected']]['relative'],str)else bytes(s['search']['report']['hits'][s['search']['selected']]['relative'])==selected
    s=p.act('recursive-page-next',ready(1));assert p.log.read_text().count('FILES_SEARCH_PAGE_CREDIT')==credits
    p.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
    s=p.act('recursive-page-next',lambda s:ready(2)(s)and s['search_pages']['ended']and not s['search']['running']and not s['search_pages']['next_hit'])
    third=paths(s);assert len(third)==207 and len(first|second|third)==1207 and b'page-\xff.txt'in first|second|third and s['search_pages']['complete']
    assert s['search_pages']['bytes']<=2*1024*1024
    for x,y,w,h in s['search_pages']['rects']:assert x>=12 and y>=66 and x+w<=428 and y+h<=400
    p.capture('search-pages-terminal-440');assert len(re.findall(r'command=search_pages',p.log.read_text()))==1
    s=p.act('recursive-key:f9',ready(0));report['pages_live_before_stop']=live(p)
    s=p.act('recursive-cancel',lambda s:not s['search']['running']and s['search_pages']['cancelled']and s['search_pages']['current']==0 and not s['search_pages']['next_hit'])
    assert len(paths(s))==500 and s['search_pages']['found']==500;p.capture('search-pages-stopped-keeps-window')
    spawns=p.log.read_text().count('command=search_pages');p.state();assert p.log.read_text().count('command=search_pages')==spawns
    s=p.act('recursive-key:f9',ready(0));report['pages_live_before_close']=live(p);p.capture('search-pages-restarted-before-active-close');p.close()
    assert {os.fsencode(f.name):sha(f)for f in folder.iterdir()}==before
    e=open_pages('pages-exact',root('Pages1000',1000));e.act('recursive-pages',ready(0));e.act('recursive-page-next',ready(1))
    s=e.act('recursive-page-next',lambda s:s['search_pages']['ended']and not s['search']['running']and not s['search_pages']['next_hit'])
    assert s['search_pages']['current']==1 and len(paths(s))==500 and s['search_pages']['found']==1000 and s['search']['report']['complete'];e.capture('search-pages-empty-terminal-keeps-last');e.close()
    h=open_pages('pages-history',root('Pages4507',4507));h.act('recursive-pages',ready(0))
    for sequence in range(1,10):s=h.act('recursive-page-next',ready(sequence));assert s['search_pages']['bytes']<=2*1024*1024
    assert s['search_pages']['oldest']>=2 and s['search_pages']['ended'];h.capture('search-pages-history-eviction-visible')
    while s['search_pages']['prev_hit']:s=h.act('recursive-page-prev')
    assert s['search_pages']['current']==s['search_pages']['oldest'];h.capture('search-pages-oldest-retained');h.close()
    # Full500 opaque513-byte paths used to inflate during decoding and reject a valid page.
    folder=base/'LongRawPages';folder.mkdir();parent=folder
    for _ in range(6):parent=parent/('d'*80);parent.mkdir()
    for i in range(501):
        name=b'f'+f'{i:04}'.encode()+b'-'+b'x'*16+b'\xff.txt'
        with open(os.fsencode(parent)+b'/'+name,'wb')as out:out.write(b'x')
    r=open_pages('pages-long-raw',folder);s=r.act('recursive-pages',ready(0));assert len(paths(s))==500 and all(len(path)==513 for path in paths(s));r.capture('search-pages-500-long-raw-paths')
    chosen=s['search']['report']['hits'][s['search']['selected']]['relative'];chosen=bytes(chosen)if isinstance(chosen,list)else os.fsencode(chosen)
    s=r.act('recursive-locate',lambda s:not s['search']['visible']and not s['search']['running']and bytes(s['path_raw'])==os.fsencode(parent)and s['selected']==chosen.rsplit(b'/',1)[-1].hex());r.capture('search-pages-long-raw-located');r.close()
    folder=root('ContentPages',602)
    for file in folder.iterdir():file.write_bytes(b'header\nneedle text\n')
    c=open_pages('pages-content',folder);c.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
    c.act('recursive-content-hit');c.act('recursive-content-input:needle');s=c.act('recursive-pages',ready(0))
    assert len(paths(s))==500 and s['search']['report']['content']['files']==500
    assert all(h['content']==dict(line=2,byte=7)and h['snippet']['text'][h['snippet']['start']:h['snippet']['end']]=='needle'for h in s['search']['report']['hits'])
    c.capture('search-pages-content-first-440');s=c.act('recursive-key:page-next',lambda s:ready(1)(s)and not s['search']['running'])
    assert len(paths(s))==102 and s['search']['report']['content']['files']==602 and s['search_pages']['complete'];c.capture('search-pages-content-terminal-440');c.close()
    report['pages_content_scope']='PASS literal input/602 matches=500+102, line2/byte7/UTF8 snippet needle exact, cumulative files602 and terminal after reap, real440px framebuffers; no clipboard bytes/resources/native input'
    report['pages_scope']='PASS1207=500/500/207 union/raw; explicit keyboard/painted credit, cached prev/next no new credit, marks page-local/current copy review cancel, stale epoch ignored; exact1000 empty terminal retains500;4507 history max8/2MiB explicit eviction;500opaque513B paths decoded and located byte-exact; stop retains valid window/counters with no continuation; restart/live helper before callback/root0ACK. Private handlers/framebuffers, no native input/resources/lifetimes/snapshot/global ordering proof.'

def check_scope(base,config):
    root=base/'ScopeRoot';root.mkdir();nested=root/'Nested';nested.mkdir();raw=root/os.fsdecode(b'Raw-\xfe');raw.mkdir()
    for folder in [root,nested,raw]:(folder/'needle.txt').write_bytes(b'owned needle')
    fingerprints={os.fsencode(p.relative_to(root)):sha(p)for p in root.rglob('*')if p.is_file()}
    app=App('search-scope',root,base,config,root)
    app.act('recursive-open');s=app.act('recursive-query:ext:txt',lambda s:s['search']['report']is not None and not s['search']['running']);q=s['search'];query=q['query'];hidden=q['hidden'];fields=q['fields']
    target=next(i for i,h in enumerate(q['report']['hits'])if (bytes(h['relative'])if isinstance(h['relative'],list)else os.fsencode(h['relative']))==b'Nested/needle.txt')
    app.act('recursive-select:'+str(target));app.act('recursive-scope-menu',lambda s:s['search']['scope_menu']);app.capture('search-scope-menu')
    app.act('recursive-key:escape',lambda s:not s['search']['scope_menu']);before=app.log.read_text().count('command=search_batches')
    app.act('recursive-key:altright',lambda s:bytes(s['search']['root'])==os.fsencode(nested)and s['search']['report']is None)
    assert app.log.read_text().count('command=search_batches')==before
    s=app.state();assert s['search']['query']==query and s['search']['hidden']==hidden and s['search']['fields']==fields
    app.capture('search-scope-narrowed-criteria-preserved')
    app.act('recursive-key:enter',lambda s:s['search']['report']is not None and not s['search']['running'])
    app.act('recursive-key:altleft',lambda s:bytes(s['search']['root'])==os.fsencode(root)and s['search']['report']is None)
    app.act('recursive-key:enter',lambda s:s['search']['report']is not None and not s['search']['running'])
    s=app.state();i=next(i for i,h in enumerate(s['search']['report']['hits'])if (bytes(h['relative'])if isinstance(h['relative'],list)else os.fsencode(h['relative']))==b'Raw-\xfe/needle.txt')
    app.act('recursive-select:'+str(i));app.act('recursive-scope-menu');app.act('recursive-scope-result',lambda s:bytes(s['search']['root'])==os.fsencode(raw))
    app.capture('search-scope-raw-folder')
    app.act('recursive-key:enter',lambda s:s['search']['report']is not None and not s['search']['running'])
    app.act('recursive-scope-menu');app.act('recursive-scope-parent',lambda s:bytes(s['search']['root'])==os.fsencode(root))
    app.act('recursive-scope-menu');app.act('recursive-scope-back',lambda s:bytes(s['search']['root'])==os.fsencode(raw))
    app.close()
    small=App('search-scope-small',nested,base,config,root,'440x440');small.act('recursive-open');small.act('recursive-scope-menu');small.capture('search-scope-menu-440')
    small.act('recursive-scope-parent',lambda s:bytes(s['search']['root'])==os.fsencode(root));small.capture('search-scope-parent-440');small.act('recursive-key:f6');small.capture('search-scope-content-mode-440');small.close()
    live=base/'ScopeLive';live.mkdir()
    for i in range(80):(live/f'needle-{i}.txt').write_bytes(b'owned live scope')
    live_hashes={p.name:sha(p)for p in live.glob('*.txt')}
    gate=live/'.files-search-gate';gate.touch()
    active=App('search-scope-live',live,base,config,root);active.act('recursive-open')
    active.act('recursive-query:ext:txt',lambda s:s['search']['running']and s['search']['provisional']is not None and s['search']['report']is None)
    pid,start=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=search_batches',active.log.read_text())[-1]
    proc=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split();assert proc[19]==start and proc[0]!='Z'
    active.capture('search-scope-live-provisional')
    active.act('recursive-key:altup',lambda s:bytes(s['search']['root'])==os.fsencode(base)and s['search']['report']is None)
    s=active.act('recursive-key:enter',lambda s:s['search']['report']is not None and not s['search']['running'])
    paths=[bytes(h['relative'])if isinstance(h['relative'],list)else os.fsencode(h['relative'])for h in s['search']['report']['hits']]
    assert b'ScopeLive/needle-0.txt'in paths and not any(p.startswith(b'needle-')for p in paths)
    active.capture('search-scope-live-new-root');active.close();gate.unlink();assert live_hashes=={p.name:sha(p)for p in live.glob('*.txt')}
    for capture in report['captures']:
        assert tuple(capture['dimensions'])==((440,440)if '440'in Path(capture['path']).name else(1000,650))
    report['old_helper_live_before_scope_change']={'pid':int(pid),'start':start,'state':proc[0]}
    assert fingerprints=={os.fsencode(p.relative_to(root)):sha(p)for p in root.rglob('*')if p.is_file()}
    identities=set().union(*(set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=search_(?:batches|pages)',app.log.read_text()))for app in apps));assert identities
    for pid,start in identities:
        try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
        except FileNotFoundError:continue
        assert parts[19]!=start,'helper identity remains including Z'
    report.update(functional_gate='PASS',normal_exits=3,logged_helpers=[list(v)for v in sorted(identities)],scope='Search root parent/result/back with preserved criteria and raw path, explicit Enter only, keyboard handler + painted mouse actions; three apps and callback0 after ACK,8FB including440/contentmode; actual live old helper during scope change, new-root paths no old unprefixed rows; source file hashes unchanged. No native input/lifetime/resources proof.')

with tempfile.TemporaryDirectory(prefix='search-order-',dir=BASE) as temp:
    base=Path(temp);config=base/'config';config.mkdir();source=base/'Source';source.mkdir()
    for folder in ['a','z']:(source/folder).mkdir()
    for name in ['a/file10.txt','z/file2.txt','a/file2.txt']:(source/name).write_bytes(b'owned text')
    (source/'folder.txt').mkdir();(source/'link.txt').symlink_to('a/file2.txt')
    raw=b'raw-\xff.txt';(source/os.fsdecode(raw)).write_bytes(b'raw owned')
    for n in range(36):(source/f'item{n}.txt').write_bytes(b'owned '+str(n).encode())
    before={os.fsencode(p.relative_to(source)):sha(p)for p in source.rglob('*')if p.is_file() and not p.is_symlink()}
    gate=source/'.files-search-gate'
    def provisional(s):return s['search']['provisional'] is not None and s['search']['running'] and s['search']['report'] is None
    def final(s):return not s['search']['running'] and s['search']['report'] is not None
    def wire(v):return os.fsencode(v)if isinstance(v,str)else bytes(v)
    def selected(s):
        q=s['search'];hits=q['provisional'] if q['report'] is None else [h['relative']for h in q['report']['hits']]
        return wire(hits[q['selected']])
    def paths(s):return list(map(bytes,s['search']['display_paths']))
    try:
        if args.scope:check_scope(base,config)
        else:
            raw_target=base/os.fsdecode(b'Copied-\xfe')
            if args.transfer:raw_target.mkdir()
            a=App('search-order',source,base,config,raw_target if args.transfer else source);a.act('recursive-open');gate.touch()
            s=a.act('recursive-query:ext:txt',provisional);chosen=selected(s)
            s=a.act('recursive-order');assert s['search']['display_order']=='Nombre' and selected(s)==chosen
            a.capture('search-order-provisional-name')
            if args.background:
                original_active=s['tab_active'];original_path=s['path_raw'];original_query=s['query']
                helpers_before=a.log.read_text().count('command=search_batches')
                provisional_paths=s['search']['provisional'];chosen_index=s['search']['selected']
                if args.marks:
                    s=a.act('recursive-check:'+str(chosen_index));assert list(map(bytes,s['search']['marked']))==[chosen]
                    old_epoch=s['search']['result_epoch'];a.capture('search-marks-provisional-checkbox')

                s=a.act('recursive-background');assert len(s['tabs'])==2 and s['tab_active']==original_active
                assert s['path_raw']==original_path and s['query']==original_query and selected(s)==chosen
                assert s['search']['provisional']==provisional_paths and s['search']['selected']==chosen_index and s['search']['running']
                assert s['search']['background_tab']==s['tabs'][-1]['id']
                assert a.log.read_text().count('command=search_batches')==helpers_before
                a.capture('search-background-provisional-kept')

            gate.unlink();s=a.state(predicate=final);assert selected(s)==chosen and s['search']['report']['complete']
            if args.marks:
                assert list(map(bytes,s['search']['marked']))==[chosen] and s['search']['result_epoch']!=old_epoch
                before_marks=s['search']['marked'];s=a.act('recursive-old-check:'+str(old_epoch)+':0');assert s['search']['marked']==before_marks
                a.act('recursive-key:marks-none')
            p=paths(s);assert len(p)==42 and p.index(b'a/file2.txt')<p.index(b'z/file2.txt')<p.index(b'a/file10.txt')
            a.capture('search-order-final-natural-name')
            s=a.act('recursive-order-key');assert s['search']['display_order']=='Tipo' and selected(s)==chosen
            assert paths(s)[0]==b'folder.txt' and paths(s)[-1]==b'link.txt'
            s=a.act('recursive-key:home');assert selected(s)==paths(s)[0]
            s=a.act('recursive-key:down');assert selected(s)==paths(s)[1]
            a.capture('search-order-kind-keyboard')
            s=a.act('recursive-order');assert s['search']['display_order']=='Ruta' and paths(s)==sorted(paths(s))
            i=next(i for i,h in enumerate(s['search']['report']['hits'])if wire(h['relative'])==raw)
            a.act('recursive-select:'+str(i));a.act('fixture-minimum',lambda s:s['width']==440)
            s=a.act('recursive-order');assert selected(s)==raw and s['search']['display_order']=='Nombre'
            a.capture('search-order-name-raw-selected-440')
            if args.background:
                s=a.act('recursive-background');assert len(s['tabs'])==3 and selected(s)==raw and s['search']['visible']
                assert s['search']['background_hit'] and len(s['search']['result_controls'])==3
                for x,y,w,h in s['search']['result_controls']:assert x>=12 and x+w<=428 and y+h<=440
                a.capture('search-background-raw-440')

            s=a.act('recursive-locate',lambda s:not s['search']['visible'] and s['selected']==raw.hex() and not s['search']['running'])
            a.capture('search-order-locate-raw-440')
            if args.background:
                s=a.act('recursive-tab-last',lambda s:s['bookmarks']['browser_loaded'] and s['selected']==raw.hex())
                assert bytes(s['path_raw'])==os.fsencode(source) and s['tab_active']!=original_active and s['query']==''
                a.capture('search-background-raw-activated')
                a.act('recursive-tab-first',lambda s:s['bookmarks']['browser_loaded'])
                a.act('recursive-open');s=a.act('recursive-query:ext:txt',final)
                directory=next(i for i,h in enumerate(s['search']['report']['hits'])if wire(h['relative'])==b'folder.txt')
                a.act('recursive-select:'+str(directory));s=a.act('recursive-key:ctrlenter')
                assert len(s['tabs'])==4 and s['search']['visible'] and bytes(s['tabs'][-1]['path_raw'])==os.fsencode(source/'folder.txt')
                a.act('recursive-key:escape')
                s=a.act('recursive-tab-last',lambda s:s['bookmarks']['browser_loaded'] and bytes(s['path_raw'])==os.fsencode(source/'folder.txt'))
                assert not s['order'] and s['selected'] is None
                a.capture('search-background-directory-activated')
                a.act('recursive-tab-first',lambda s:s['bookmarks']['browser_loaded'])
                a.act('recursive-open');s=a.act('recursive-query:ext:txt',final)
                s=a.act('recursive-select:0')
                if args.marks:
                    a.act('recursive-key:shiftdown');s=a.act('recursive-key:shiftdown')
                    assert len(s['search']['marked'])==3
                    selected_paths=list(map(bytes,s['search']['marked']));expected=[]
                    for path in paths(s):
                        if path in selected_paths:expected.append(os.fsencode(source/'folder.txt' if path==b'folder.txt' else (source/os.fsdecode(path)).parent))
                    before_tabs=len(s['tabs']);s=a.act('recursive-background')
                    assert len(s['tabs'])==before_tabs+3 and s['search']['background_count']==3
                    assert [bytes(t['path_raw'])for t in s['tabs'][-3:]]==expected
                    a.capture('search-marks-three-background-tabs-440')
                    s=a.act('recursive-key:insert');assert len(s['search']['marked'])==2
                    a.capture('search-marks-insert-keeps-other-marks')
                    s=a.act('recursive-key:ctrla');assert len(s['search']['marked'])==42
                    tabs_before=s['tabs'];s=a.act('recursive-background');assert s['tabs']==tabs_before and 'lote' in s['search']['error']
                    a.capture('search-marks-too-many-tabs-whole-batch-rejected')
                    s=a.act('recursive-marks-button');assert not s['search']['marked'] and s['search']['marks_explicit']
                    s=a.act('recursive-background');assert 'No quedan' in s['search']['error'] and s['tabs']==tabs_before
                    a.capture('search-marks-explicit-empty-no-focused-fallback')
                    s=a.act('recursive-select:0')
                    report['marks_scope']='painted checkbox marks held provisional path and survives final epoch; stale hit ignored, ShiftDown visual range3, atomic3backgroundtabs in visualorder, CtrlA42/rejected whole tabbatch, painted None button/explicit empty no focus fallback; bounds/raw/pruning unit proof'
                while len(s['tabs'])<8:s=a.act('recursive-key:ctrlenter')
                tabs=s['tabs'];active=s['tab_active'];s=a.act('recursive-background')
                assert len(s['tabs'])==8 and s['tabs']==tabs and s['tab_active']==active and s['search']['visible'] and 'ocho' in s['search']['error']
                a.capture('search-background-limit-eight-440')
                report['background_scope']='provisional search+selection/order+source context retained, no extra helper at background admission; raw result440 through painted button and activation preserves exact selection; directory CtrlEnter activates empty target without launcher; repeated CtrlEnter capped8, failure keeps tabs/search/active; no preload/new worker/native input/resources'
            expected_remaining=before
            if args.transfer:
                target=raw_target;replaced_target=base/'Replaced';replaced_target.mkdir();moved=base/'Moved';moved.mkdir();conflict=base/'Conflict';conflict.mkdir();cancelled=base/'Cancelled';cancelled.mkdir()
                roots=[b'a/file10.txt',b'z/file2.txt',b'folder.txt',raw,b'link.txt']
                def select_roots(values):
                    state=a.state()
                    if not state['search']['visible']:
                        a.act('recursive-open');state=a.act('recursive-query:ext:txt',final)
                    a.act('recursive-key:marks-none')
                    for name in values:
                        index=next(i for i,h in enumerate(state['search']['report']['hits'])if wire(h['relative'])==name)
                        state=a.act('recursive-mark:'+str(index))
                    assert set(map(bytes,state['search']['marked']))==set(values)
                    return state
                def review(action):return a.act(action,lambda s:s['destination_editor']is not None and not s['search']['capture_busy'])
                def form_destination(path):return a.act('recursive-place-destination')if path==raw_target else a.act('recursive-destination:'+str(path))
                def queue_done(previous,ok):return lambda s:s['copy_progress']['phase']=='Idle' and s['copy_result']is not None and s['copy_result']['id']!=previous and s['copy_result']['ok']==ok
                def fingerprint():
                    return {name:(os.lstat(source/os.fsdecode(name)).st_ino,os.lstat(source/os.fsdecode(name)).st_mode,os.lstat(source/os.fsdecode(name)).st_size,os.lstat(source/os.fsdecode(name)).st_mtime_ns)for name in roots}
                # Duplicate names are rejected before capture, not silently reduced to one parent.
                select_roots([b'a/file2.txt',b'z/file2.txt']);s=a.act('recursive-copy');assert not s['search']['capture_busy']and 'mismo nombre' in s['search']['error']and s['destination_editor']is None
                a.capture('search-transfer-duplicate-basenames-explicit')
                select_roots(roots);s=review('recursive-key:f5');assert s['destination_editor']['multi_parents']==3 and s['destination_editor']['multi_roots']==5
                a.capture('search-transfer-copy-review')
                a.act('clip-cancel-dialog');assert a.state()['search']['visible']
                # Replace AFTER a real captured review, preserving size/mtime; all roots stay untouched.
                review('recursive-copy');form_destination(replaced_target)
                old_stat=(source/'a/file10.txt').stat();(source/'a/file10.txt').rename(base/'old-file10');(source/'a/file10.txt').write_bytes(b'other text');os.utime(source/'a/file10.txt',ns=(old_stat.st_atime_ns,old_stat.st_mtime_ns))
                previous=a.state()['copy_result'];previous=previous['id']if previous else None
                s=a.act('clip-confirm',queue_done(previous,False));assert s['copy_result']['completed']==0 and not list(replaced_target.iterdir())and (source/'a/file10.txt').read_bytes()==b'other text'
                a.capture('search-transfer-replacement-rejected')
                a.act('clip-dismiss');(source/'a/file10.txt').unlink();(base/'old-file10').rename(source/'a/file10.txt')
                original=fingerprint();select_roots(roots);review('recursive-copy');form_destination(cancelled)
                a.act('clip-paused');a.act('clip-confirm',lambda s:s['destination_editor']is None and s['copy_progress']['phase']=='Paused');assert fingerprint()==original and not list(cancelled.iterdir())
                a.act('recursive-close');s=a.act('clip-cancel-job',lambda s:s['copy_progress']['phase']=='Idle'and s['copy_result']['cancelled']);assert not list(cancelled.iterdir())and fingerprint()==original
                a.capture('search-transfer-paused-copy-cancelled');a.act('clip-dismiss')
                select_roots(roots);review('recursive-copy');form_destination(target);previous=a.state()['copy_result']['id']
                s=a.act('clip-confirm',queue_done(previous,True));assert s['copy_result']['completed']==5 and fingerprint()==original
                for name in roots:
                    to=target/os.fsdecode(name.rsplit(b'/',1)[-1]);fr=source/os.fsdecode(name)
                    assert to.exists()or to.is_symlink()
                    if fr.is_symlink():assert os.readlink(to)==os.readlink(fr)
                    elif fr.is_file():assert sha(to)==sha(fr)
                a.capture('search-transfer-copy-completed')
                # Move review is nonmutating, can be cancelled, and EXDEV is rejected as a batch.
                select_roots(roots);s=review('recursive-key:shiftf6');assert s['destination_editor']['moving'];form_destination(moved)
                s=a.act('clip-confirm',lambda s:s['destination_editor']is not None and s['destination_editor']['prepared']);assert fingerprint()==original and not list(moved.iterdir())
                a.act('fixture-minimum',lambda s:s['width']==440);a.capture('search-transfer-move-review-440');a.act('clip-cancel-dialog')
                select_roots([b'a/file10.txt',b'z/file2.txt']);review('recursive-move');form_destination(Path('/proc'))
                s=a.act('clip-confirm',lambda s:s['destination_editor']is not None and not s['destination_editor']['preparing']and bool(s['destination_editor']['error']));assert not s['destination_editor']['prepared']and fingerprint()==original
                a.capture('search-transfer-exdev-batch-rejected');a.act('clip-cancel-dialog')
                # Late destination conflict leaves confirmed first root and pending second; no global rollback.
                review('recursive-move');form_destination(conflict);a.act('clip-confirm',lambda s:s['destination_editor']is not None and s['destination_editor']['prepared'])
                (conflict/'file2.txt').write_bytes(b'actor occupant');previous=a.state()['copy_result']['id']
                s=a.act('clip-confirm',queue_done(previous,False));assert s['copy_result']['batch_states']==['Confirmed','Pending']
                assert not (source/'a/file10.txt').exists()and (conflict/'file10.txt').read_bytes()==b'owned text'and (source/'z/file2.txt').read_bytes()==b'owned text'and (conflict/'file2.txt').read_bytes()==b'actor occupant'
                a.capture('search-transfer-move-partial-ledger')
                if args.retry:
                    # The occupied target still rejects; the already confirmed inode is never replayed.
                    first_inode=(conflict/'file10.txt').lstat().st_ino;old=s['copy_result']['id']
                    s=a.act('dest-retry-pending',queue_done(old,False));assert s['copy_result']['batch_states']==['Confirmed','Pending']and s['copy_result']['completed']==0
                    assert (conflict/'file10.txt').lstat().st_ino==first_inode and (conflict/'file2.txt').read_bytes()==b'actor occupant'
                    a.capture('search-retry-conflict-preserved')
                    (conflict/'file2.txt').unlink();old=s['copy_result']['id']
                    s=a.act('dest-retry-key',queue_done(old,True));assert s['copy_result']['completed']==1 and s['copy_result']['moving']
                    assert (conflict/'file10.txt').lstat().st_ino==first_inode and (conflict/'file2.txt').lstat().st_ino==original[b'z/file2.txt'][0]
                    assert not(source/'a/file10.txt').exists()and not(source/'z/file2.txt').exists()
                    a.act('recursive-close');shown=a.act('dest-details',lambda s:s['copy_result']['notice']);assert shown['copy_result']['message'].startswith('Resultado de movimiento #');a.capture('search-retry-pending-only-completed')
                    if args.reports:
                        a.capture('operation-report-first-440');view=a.state()['notice_view'];rows=view['rows'];total=view['total'];assert total>rows>1 and view['scroll']==0 and 'Start'not in view['navigation']
                        stable=a.state()['copy_result']['id'];s=a.act('dest-report-key:page-down');assert s['notice_view']['scroll']==min(rows-1,total-rows);a.capture('operation-report-page-down-440')
                        a.act('dest-report-key:home')
                        s=a.act('dest-report-end');assert s['notice_view']['scroll']==total-rows and 'End'not in s['notice_view']['navigation']and 'Pages(1)'not in s['notice_view']['navigation'];a.capture('operation-report-last-page-440')
                        s=a.act('dest-report-key:down');assert s['notice_view']['scroll']==total-rows
                        s=a.act('dest-report-prev');assert s['notice_view']['scroll']==max(0,total-rows-(rows-1))
                        s=a.act('dest-report-key:home');assert s['notice_view']['scroll']==0
                        s=a.act('dest-report-next');assert s['notice_view']['scroll']==min(rows-1,total-rows)
                        a.act('dest-report-key:end');s=a.act('fixture-wide',lambda s:s['width']==1000 and s['notice_view']['rows']>rows);v=s['notice_view'];assert v['scroll']==max(0,total-v['rows']);a.capture('operation-report-wide-last-page')
                        if v['scroll']>0:s=a.act('dest-report-start')
                        else:
                            assert 'Start'not in v['navigation'];s=a.act('dest-report-key:home')
                        assert s['notice_view']['scroll']==0
                        s=a.act('dest-report-key:right');assert s['notice_view']['pan']==min(80,s['notice_view']['pan_limit'])
                        s=a.act('dest-report-key:left');assert s['notice_view']['pan']==0
                        assert s['copy_result']['id']==stable and s['copy_result']['completed']==1
                        a.act('fixture-minimum',lambda s:s['width']==440 and s['notice_view']['rows']==rows);a.capture('operation-report-restored-440')
                        report['report_navigation_scope']='PASS real Move report440/1000: keyPgDown/Home/Down clamp, painted last/prev/next/start buttons, full last page, wide resize clamps using new visible rows, horizontal pan bounded, operation identity/completed unchanged. No native input/lifetimes/resources.'
                    # Restore BOTH only in this private fixture before a separate new intent.
                    (conflict/'file2.txt').rename(source/'z/file2.txt')
                    report['retry_scope']='PASS painted pending-only retry440: occupied target rejects without replaying Confirmed; explicit fixture occupant removal then original Pending moves with same inode, completed1 in THIS attempt. Same proof/token, fresh IDs; no recapture/rollback/globalresource/nativeinput claim.'
                a.act('clip-dismiss')
                # Explicit fixture-owned restore before a NEW user intent, never product rollback.
                (conflict/'file10.txt').rename(source/'a/file10.txt');original=fingerprint()
                select_roots(roots);review('recursive-move');form_destination(moved);a.act('clip-confirm',lambda s:s['destination_editor']is not None and s['destination_editor']['prepared'])
                a.act('clip-paused');a.act('clip-confirm',lambda s:s['destination_editor']is None and s['copy_progress']['phase']=='Paused');assert fingerprint()==original and not list(moved.iterdir())
                a.act('recursive-close');a.capture('search-transfer-move-paused-no-mutation');job=a.state()['copy_progress']['active'];assert job is not None
                s=a.act('clip-resume',lambda s:s['copy_progress']['phase']=='Idle'and s['copy_result']is not None and s['copy_result']['id']==job and s['copy_result']['ok']);assert s['copy_result']['completed']==5 and s['copy_result']['moving']
                for name in roots:
                    fr=source/os.fsdecode(name);to=moved/os.fsdecode(name.rsplit(b'/',1)[-1]);assert not os.path.lexists(fr)and os.path.lexists(to)and to.lstat().st_ino==original[name][0]
                expected_remaining={name:h for name,h in before.items()if name not in roots}
                a.capture('search-transfer-move-completed')
                runtime=Path(os.environ.get('XDG_RUNTIME_DIR','/nonexistent'))
                if runtime.is_dir()and not runtime.is_symlink()and runtime.stat().st_uid==os.getuid()and runtime.stat().st_dev!=source.stat().st_dev:
                    runtime_owned=tempfile.TemporaryDirectory(prefix='files-search-exdev-',dir=runtime)
                    cross=Path(runtime_owned.name);select_roots([b'item0.txt']);s=review('recursive-move');form_destination(cross)
                    s=a.act('clip-confirm',lambda s:s['destination_editor']is not None and s['destination_editor']['prepared'])
                    assert s['destination_editor']['exdev']
                    a.capture('search-transfer-single-exdev-reviewed')
                    previous=a.state()['copy_result']['id'];s=a.act('clip-confirm',queue_done(previous,True))
                    assert s['copy_result']['exdev']and s['copy_result']['completed']==1 and not(source/'item0.txt').exists()and sha(cross/'item0.txt')==before[b'item0.txt']
                    expected_remaining.pop(b'item0.txt');a.capture('search-transfer-single-exdev-completed')
                    report['single_exdev_scope']='PASS single regular nlink1 on observed distinct existing runtime st_dev; captured FD proof→existing journaled engine; bytes/source absence. Not trees/hardlinks/batches/powerloss.'
                else:report['single_exdev_scope']='NOT_VALIDATED: distinct owned existing runtime filesystem unavailable'

                report['transfer_scope']='real AuxIo capture3parents5roots; copied raw/link/dir/file bytes and source metadata unchanged, cancel review/paused copy; replacement same size+mtime after review rejects before copy, no rollback of actor; move prepared/cancel/EXDEV no mutation, late collision ledger Confirmed/Pending+occupant preserved, explicit fixture restore then NEW capture/paused→move5 inodes preserved. No inputSO/wholelifetimes/resource/powerloss/EXDEVbatch support.'
            if args.saved:
                if not a.state()['search']['visible']:a.act('recursive-open')
                a.act('recursive-query:ext:txt',final);a.act('recursive-filters')
                fields=['1B','1MiB','2000-01-01','2100-01-01']
                for i,value in enumerate(fields,1):a.act('recursive-field:'+str(i));a.act('recursive-text:'+value)
                a.act('recursive-filters');a.act('recursive-content-hit');a.act('recursive-content-input:owned');s=a.act('recursive-hidden');assert s['search']['hidden']and not s['show_hidden']
                s=a.act('recursive-order');saved_order=s['search']['display_order']
                root_before=s['search']['root'];browser_before=s['path_raw'];helpers=a.log.read_text().count('command=search_batches')
                if args.transfer:
                    retained_result=a.state()['copy_result']['id']
                    a.act('saved-key:save',lambda s:s['saved_searches']['loaded']and not s['saved_searches']['busy']and s['saved_searches']['panel']['editing'])
                    a.act('saved-label:'+('x'*97));a.capture('notice-owner-invalid-label')
                    a.act('saved-details');a.capture('notice-owner-unrelated-diagnostic');owned=a.state()
                    assert owned['notice_owner']['allowed'] is None and owned['notice_owner']['detail_targets']==0 and owned['copy_result']['id']==retained_result
                    a.act('clip-dismiss')
                s=a.act('saved-key:save',lambda s:s['saved_searches']['loaded']and not s['saved_searches']['busy']and s['saved_searches']['panel']['editing']);a.act('saved-label:Código @ literal');a.capture('saved-search-label-440')
                s=a.act('saved-commit',lambda s:not s['saved_searches']['busy']and len(s['saved_searches']['rows'])==1);row=s['saved_searches']['rows'][0];assert row['label']=='Código @ literal'and row['fields']==fields and row['content']=='owned'and row['hidden']and row['root']==root_before and row['order']=={'Ruta':0,'Nombre':1,'Tipo':2}[saved_order]
                assert s['search']['fields']==fields and s['search']['content']=='owned'and a.log.read_text().count('command=search_batches')==helpers;a.capture('saved-search-catalogue-440')
                a.act('saved-key:save');a.act('saved-label:Cancelada');a.act('saved-key:escape');a.act('saved-close');s=a.act('recursive-query:ext:md',final);assert len(s['saved_searches']['rows'])==1
                helpers=a.log.read_text().count('command=search_batches');a.act('saved-open');s=a.act('saved-load');assert not s['search']['running']and s['search']['report']is None and s['search']['query']=='ext:txt'and s['search']['fields']==fields and s['search']['content']=='owned'and s['path_raw']==browser_before and s['search']['display_order']==saved_order
                assert a.log.read_text().count('command=search_batches')==helpers and not s['show_hidden'];a.capture('saved-search-loaded-no-execution')
                s=a.act('recursive-key:enter',final);assert s['search']['report']['content']is not None;a.capture('saved-search-explicit-results')
                if args.transfer:
                    a.act('recursive-close');a.act('dest-details');a.capture('notice-owner-original-result-reopened');owned=a.state()
                    assert owned['notice_owner']['allowed']==retained_result and owned['notice_owner']['copy']==retained_result and owned['notice_owner']['detail_targets']==1
                    a.act('clip-dismiss')
                    report['notice_owner_scope']='PASS retained real operation survives unrelated Saved validation diagnostic; full-report target absent there, restored only by reopening original result. No export or OS clipboard invoked.'

            a.close()
            assert expected_remaining=={os.fsencode(p.relative_to(source)):sha(p)for p in source.rglob('*')if p.is_file() and not p.is_symlink()}
            if args.pages:
                pages_target=base/'PagesTarget';pages_target.mkdir();check_pages(base,config,pages_target)
            if args.saved:
                other=base/os.fsdecode(b'Other-\xfe');other.mkdir();(other/'other.txt').write_bytes(b'owned isolated')
                b=App('saved-second',other,base,config,other,'440x440');b.act('recursive-open');b.act('saved-open',lambda s:s['saved_searches']['loaded']and len(s['saved_searches']['rows'])==1 and not s['saved_searches']['busy']);helpers=b.log.read_text().count('command=search_batches')
                s=b.act('saved-here');assert bytes(s['search']['root'])==os.fsencode(other)and s['search']['fields']==fields and s['search']['hidden']and not s['search']['running'];assert b.log.read_text().count('command=search_batches')==helpers
                b.capture('saved-search-load-here-raw-root');s=b.act('recursive-key:enter',final);assert len(s['search']['report']['hits'])==1
                b.act('saved-key:save',lambda s:s['saved_searches']['panel']['editing']);b.act('saved-label:Raw @ aquí');s=b.act('saved-commit',lambda s:not s['saved_searches']['busy']and len(s['saved_searches']['rows'])==2);assert bytes(s['saved_searches']['rows'][1]['root'])==os.fsencode(other)and s['saved_searches']['panel']['selected']==2;b.capture('saved-search-two-roots');b.act('saved-close');b.close()
                c=App('saved-third',source,base,config,source,'440x440');c.act('recursive-open');s=c.act('saved-open',lambda s:s['saved_searches']['loaded']and not s['saved_searches']['busy']);assert len(s['saved_searches']['rows'])==2 and bytes(s['saved_searches']['rows'][1]['root'])==os.fsencode(other)
                c.act('saved-select:2');helpers=c.log.read_text().count('command=search_batches');s=c.act('saved-key:enter');assert bytes(s['search']['root'])==os.fsencode(other)and not s['search']['running']and c.log.read_text().count('command=search_batches')==helpers;c.capture('saved-search-raw-root-reopened');s=c.act('recursive-key:enter',final);assert len(s['search']['report']['hits'])==1
                c.act('saved-open');c.act('saved-select:2');c.act('saved-key:f2');c.act('saved-label:Renombrada');s=c.act('saved-key:enter',lambda s:not s['saved_searches']['busy']and s['saved_searches']['rows'][1]['label']=='Renombrada');c.capture('saved-search-renamed');c.act('saved-remove',lambda s:not s['saved_searches']['busy']and len(s['saved_searches']['rows'])==1);c.act('saved-close');c.close();assert (other/'other.txt').read_bytes()==b'owned isolated'
                file=config/'searches.bin';data=bytearray(file.read_bytes());data[8]=2;file.write_bytes(data);future=sha(file)
                d=App('saved-future',source,base,config,source,'440x440');d.act('recursive-open');s=d.act('saved-open',lambda s:bool(s['saved_searches']['error'])and not s['saved_searches']['busy']);assert not s['saved_searches']['loaded']and sha(file)==future;d.capture('saved-search-future-preserved');d.act('saved-close');d.close();assert sha(file)==future
                report['saved_scope']='PASS four owned processes save/name/filter/content/hidden, cancel-label retains1, load no helper until Enter, raw root Here and persisted raw root reopened, rename stable ID then remove metadata only, future version preserved with error; storage ACK never changed search criteria. Private keys/painted handlers/framebuffer, no native input/clipboard/lifetimes/resources.'
            identities=set().union(*(set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=search_(?:batches|pages)',app.log.read_text()))for app in apps));assert identities
            for pid,start in identities:
                try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
                except FileNotFoundError:continue
                assert parts[19]!=start,'helper identity remains including Z'
            report.update(functional_gate='PASS',normal_exits=len(apps),logged_helpers=[list(v)for v in sorted(identities)],scope='real search_batches held up to5s by existing fixture gate; name sorting provisional→final preserves raw path selection; 42hits natural2before10/tie path, directories-first and symlink-last kind; keyboard visualHomeDown; pathorder; raw0xFF selected after sort→Locate440 exact listing; source hashes/root0drainACK/loggedhelpersgone strictZfalse. Private handlers/framebuffer, no native input/whole-lifetime/resources/latency proof.')
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        gate.unlink(missing_ok=True)
        report['logs']={app.label:app.log.read_text()for app in apps}
        for app in apps:
            if app.process.poll()is None:
                os.killpg(app.process.pid,signal.SIGTERM)
                try:app.process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
        if runtime_owned is not None:runtime_owned.cleanup()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha(ROOT/'target/release'/name)
        path=OUT/(('search-scope-ui-' if args.scope else 'search-pages-ui-' if args.pages else 'saved-search-ui-' if args.saved else 'operation-report-ui-' if args.reports else 'search-retry-ui-' if args.retry else 'search-transfer-ui-' if args.transfer else 'search-marks-ui-' if args.marks else 'search-background-ui-' if args.background else 'search-order-ui-')+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
