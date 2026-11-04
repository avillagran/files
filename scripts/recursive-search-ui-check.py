#!/usr/bin/env python3
"""Recursive search functional evidence; no resource/startup measurement or OS input."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--guided-regex',action='store_true');parser.add_argument('--regex',action='store_true');parser.add_argument('--installed',action='store_true');parser.add_argument('--calendar',action='store_true');parser.add_argument('--content',action='store_true');parser.add_argument('--controls',action='store_true');parser.add_argument('--progress',action='store_true');parser.add_argument('--editing',action='store_true');args=parser.parse_args()
if args.guided_regex:args.regex=True
ROOT=Path(__file__).resolve().parent.parent
BINARY=Path.home()/'.local/bin/files' if args.installed else ROOT/'target/release/files'
OUT=ROOT/'docs/evidence/search';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report=dict(functional_gate='IN_PROGRESS',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',method='production actions/painted hits through private fixture pipe; real files-helper IPC and FLTK framebuffer; no native input',states=[],captures=[])
process=None;normal=False
with tempfile.TemporaryDirectory(prefix='recursive-search-',dir=BASE) as temp:
    base=Path(temp);source=base/'Projects';source.mkdir();config=base/'config';config.mkdir();(config/'config.toml').write_text('show_hidden=false\n')
    for path in ['app/src/main.rs','app/tests/main.rs','docs/manual.md','.secret/private.rs','rootnote.rs']:
        p=source/path;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b'fixture\n')
    (source/'old-large.bin').write_bytes(b'L'*2048)
    (source/'recent-small.bin').write_bytes(b'S'*16)
    os.utime(source/'old-large.bin',(86400,86400))
    os.utime(source/'recent-small.bin',(1767225600,1767225600))
    rawdir=b'src-\xfe';rawname=b'needle-\xff.rs';rawparent=os.fsencode(source)+b'/'+rawdir;os.mkdir(rawparent)
    with open(rawparent+b'/'+rawname,'wb') as f:f.write(b'raw\n')
    outside=base/'outside';outside.mkdir();(outside/'external.rs').write_bytes(b'must not enter\n')
    (source/'outside-link').symlink_to(outside);(source/'app/src/cycle').symlink_to(source)
    bulk=source/'bulk';bulk.mkdir()
    for i in range(605):(bulk/f'hit-{i:04}.txt').write_bytes(b'cap\n')
    if args.progress:
        work=source/'progress-work';work.mkdir()
        for i in range(40000):(work/f'item-{i:05}.dat').write_bytes(b'x')
    content_needle='Hola "mundo" '+chr(92)
    if args.content:
        (source/'good.ctxt').write_text('head\n'+content_needle+'\nend\n')
        (source/'binary.ctxt').write_bytes(content_needle.encode()+b'\0')
        (source/'invalid.ctxt').write_bytes(content_needle.encode()+b'\xff')
        (source/'big.ctxt').write_bytes(content_needle.encode()+b'x'*(1024*1024))
        (source/'context.snip').write_text('ñ'*200+'\nbefore\taguja\tfin\r\n'+'z'*400)
        (source/'long.snip').write_text('ñ'*512)
        with open(rawparent+b'/'+rawname,'wb') as f:f.write(b'raw\n'+content_needle.encode())
    if args.regex:
        (source/'empty.rxt').write_bytes(b'')
        (source/'good.rxt').write_bytes(b'x'*32767+'á\n日本\nerror42\nend'.encode().replace(b'\n',bytes([10])))
        (source/'literal.rxt').write_bytes(b'a.*b')
        (source/'binary.rxt').write_bytes(b'error42'+bytes([0]))
        (source/'bad.rxt').write_bytes(b'error42'+bytes([255]))
        (source/'big.rxt').write_bytes(b'x'*(1024*1024+1))
        regex_raw=b'regex-'+bytes([255])+b'.rxt'
        with open(rawparent+b'/'+regex_raw,'wb') as stream:stream.write(b'prefix\nerror7\nend'.replace(b'\n',bytes([10])))
    calendar_expected=None
    if args.calendar:
        import datetime as dt
        from zoneinfo import ZoneInfo
        zone=ZoneInfo('America/Santiago');today=dt.datetime.now(zone).date()
        def midnight(day):return int(dt.datetime.combine(day,dt.time(),zone).timestamp())
        a=midnight(today);b=midnight(today+dt.timedelta(days=1))
        samples={'today-start.cal':a*10**9,'today-end.cal':b*10**9-1,'tomorrow.cal':b*10**9,'yesterday.cal':(midnight(today-dt.timedelta(days=1))+3600)*10**9,'six-days.cal':(midnight(today-dt.timedelta(days=6))+3600)*10**9,'seven-days.cal':(midnight(today-dt.timedelta(days=7))+3600)*10**9}
        for name,stamp in samples.items():
            (source/name).write_bytes(name.encode());os.utime(source/name,ns=(stamp,stamp))
        raw_calendar=b'raw-\xff.cal';path=os.fsencode(source)+b'/'+raw_calendar
        with open(path,'wb')as stream:stream.write(b'raw calendar bytes')
        os.utime(path,ns=((a+123)*10**9,(a+123)*10**9))
        (source/'outside-date.cal').symlink_to(outside/'external.rs')
        calendar_expected={'today':{b'today-start.cal',b'today-end.cal',raw_calendar},'yesterday':{b'yesterday.cal'},'seven':{b'today-start.cal',b'today-end.cal',raw_calendar,b'yesterday.cal',b'six-days.cal'}}
    if args.guided_regex:
        import struct
        def part(value):return struct.pack('<H',len(value))+value
        legacy=b'FILESSCH'+struct.pack('<H',1)+bytes([1])+struct.pack('<QH',2,1)
        legacy+=struct.pack('<Q',1)+part(b'Literal previo')+part(os.fsencode(source))+part(b'ext:rxt')+part(b'a.*b')+part(b'')*4+bytes([2,0])
        (config/'searches.bin').write_bytes(legacy);os.chmod(config/'searches.bin',0o600)
    before={p.relative_to(source).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in source.rglob('*') if p.is_file() and not p.is_symlink()}
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
        s=wait(current,'painted state absent, seq '+str(seq));assert not s['error'],s['error'];report['states'].append(s);return s
    def command(action):
        global seq
        offset=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);pending.replace(raw.with_suffix('.command'))
        match=wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',log.read_text()[offset:],re.M),'command absent '+action,action=='callback')
        seq=int(match[1]);return offset
    def act(action,predicate=lambda s:True):return state(command(action),predicate)
    def results(s):return {os.fsencode(h['relative']) if isinstance(h['relative'],str) else bytes(h['relative']) for h in s['search']['report']['hits']}
    def finished(s):return s['search']['report'] is not None and not s['search']['running'] and s['search']['header_painted']
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
        if args.calendar:env['TZ']='America/Santiago'
        with log.open('w') as output:
            process=subprocess.Popen([str(BINARY),str(source),'--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','120'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
        state();act('list');act('recursive-open')
        if args.guided_regex:
            pattern='(?m)^error[0-9]+$'
            act('recursive-key:ctrlf');act('recursive-text:ext:rxt');act('recursive-content-hit');act('recursive-content-input:'+pattern)
            helpers_before=len(re.findall(r'FILES_PREVIEW_HELPER pid=',log.read_text()))
            act('path-paste-prepare');s=act('recursive-regex-hit');assert s['search']['content_regex'] and s['search']['content']==pattern
            offset=command('path-paste-deliver');s=state(offset);assert 'FILES_PATH_PASTE accepted=false' in log.read_text()[offset:] and s['search']['content']==pattern
            assert not s['search']['running'] and len(re.findall(r'FILES_PREVIEW_HELPER pid=',log.read_text()))==helpers_before
            capture('guided-regex-selector-and-raw-pattern')
            def mode_geometry(s):
                rects=s['search']['regex_rects'];assert len(rects)==1
                a=rects[0];assert a[2]>0 and a[3]>0 and a[0]>=12 and a[0]+a[2]<=s['width']-12
                for b in s['search']['regex_peer_rects']:assert a[0]+a[2]<=b[0] or b[0]+b[2]<=a[0] or a[1]+a[3]<=b[1] or b[1]+b[3]<=a[1]
                return True
            s=act('fixture-minimum',lambda s:s['width']==440);mode_geometry(s);capture('guided-regex-selector-440')
            s=act('recursive-key:ctrlf6');assert not s['search']['content_regex'] and s['search']['content']==pattern and s['search']['report']is None
            capture('guided-literal-selector-440');s=act('recursive-key:ctrlf6');assert s['search']['content_regex'] and s['search']['content']==pattern
            s=act('recursive-start-hit',finished);assert results(s)=={b'good.rxt',rawdir+b'/'+regex_raw};capture('guided-regex-results-440')
            s=act('recursive-pages',lambda s:finished(s) and s['search_pages']['mode'] and s['search_pages']['ended']);mode_geometry(s);capture('guided-regex-page-controls-440')
            act('fixture-wide',lambda s:s['width']==1000)
            s=act('saved-key:save',lambda s:s['saved_searches']['panel']is not None and s['saved_searches']['loaded'] and not s['saved_searches']['busy'])
            assert s['saved_searches']['panel']['editing'] and len(s['saved_searches']['rows'])==1 and not s['saved_searches']['rows'][0]['content_regex']
            act('saved-label:Errores regex');capture('guided-regex-save-review')
            s=act('saved-commit',lambda s:not s['saved_searches']['busy'] and len(s['saved_searches']['rows'])==2 and not s['saved_searches']['panel']['editing'])
            assert not s['saved_searches']['error'];assert int.from_bytes((config/'searches.bin').read_bytes()[8:10],'little')==2
            regex_id=next(row['id'] for row in s['saved_searches']['rows'] if row['label']=='Errores regex');assert next(row for row in s['saved_searches']['rows'] if row['id']==regex_id)['content_regex']
            act('saved-close');act('recursive-key:ctrlf6')
            previous=len(re.findall(r'FILES_PREVIEW_HELPER pid=',log.read_text()))
            act('saved-open');act('saved-reload',lambda s:not s['saved_searches']['busy'] and len(s['saved_searches']['rows'])==2)
            act('saved-select:1');s=act('saved-load',lambda s:s['saved_searches']['panel']is None)
            assert not s['search']['content_regex'] and s['search']['content']=='a.*b' and not s['search']['running']
            assert len(re.findall(r'FILES_PREVIEW_HELPER pid=',log.read_text()))==previous;capture('guided-loaded-legacy-literal-v1')
            s=act('recursive-start-hit',finished);assert results(s)=={b'literal.rxt'}
            act('saved-open');act('saved-select:'+str(regex_id));s=act('saved-load',lambda s:s['saved_searches']['panel']is None)
            assert s['search']['content_regex'] and s['search']['content']==pattern and s['search']['query']=='ext:rxt' and not s['search']['running'] and s['search']['report']is None
            capture('guided-loaded-regex-v2-no-auto-search');s=act('recursive-start-hit',finished);assert results(s)=={b'good.rxt',rawdir+b'/'+regex_raw}
            capture('guided-loaded-regex-real-results');act('recursive-close');act('recursive-open')
            report['guided_regex_scope']='explicit painted selector and CtrlF6 preserve raw pattern, stale paste rejected, no automatic helper, min440 control bounds and nonoverlap including pages, loaded legacy literalv1 retained beside regexv2, actual AuxIoSave/Reload/Load mode and independent results, no search on load; private handlers not OS input'
        if args.regex:
            regex_relative=rawdir+b'/'+regex_raw
            s=act('recursive-query:ext:rxt regex:"(?m)^error[0-9]+$"',finished)
            r=s['search']['report'];assert results(s)=={b'good.rxt',regex_relative} and r['content']['regex'] and not r['complete']
            assert r['content']['binary']==2 and r['content']['limited']==1
            assert all(h['content']['regex_end']>h['content']['byte'] and h['snippet'] for h in r['hits'])
            capture('regex-content-lines-partial')
            act('fixture-minimum',lambda s:s['width']==440);capture('regex-content-lines-440')
            act('fixture-wide',lambda s:s['width']==1000)
            s=act('recursive-query:ext:rxt regex:"(?s)á.*error[0-9]+"',finished)
            assert results(s)=={b'good.rxt'} and s['search']['report']['hits'][0]['content']['byte']==32767
            capture('regex-multiline-cross-chunk')
            s=act('recursive-query:ext:rxt content:"a.*b"',finished);assert results(s)=={b'literal.rxt'} and not s['search']['report']['content'].get('regex',False)
            s=act('recursive-query:ext:rxt regex:'+chr(92)+'z',finished)
            assert results(s)=={b'empty.rxt',b'good.rxt',b'literal.rxt',regex_relative}
            assert all(h['content']['regex_end']==h['content']['byte'] and h.get('snippet') is None for h in s['search']['report']['hits'])
            capture('regex-empty-and-eof-no-fabricated-highlight')
            act('fixture-minimum',lambda s:s['width']==440);capture('regex-empty-and-eof-440')
            act('fixture-wide',lambda s:s['width']==1000)
            previous=len(re.findall(r'FILES_PREVIEW_HELPER pid=',log.read_text()))
            s=act('recursive-query:regex:(',lambda s:bool(s['search']['error']));assert not s['search']['running']
            assert s['search']['error'].startswith('Regex:') and 'unclosed group' in s['search']['error']
            assert len(re.findall(r'FILES_PREVIEW_HELPER pid=',log.read_text()))==previous;capture('regex-invalid-before-helper')
            act('recursive-query:ext:rxt regex:'+chr(92)+'z',finished)
            s=act('recursive-pages',lambda s:finished(s) and s['search_pages']['mode'] and s['search_pages']['current']==0 and s['search_pages']['ended'])
            assert results(s)=={b'empty.rxt',b'good.rxt',b'literal.rxt',regex_relative} and s['search']['report']['content']['regex']
            capture('regex-paged-empty-eof-decoder');act('recursive-close');act('recursive-open')
            s=act('recursive-query:ext:rxt name:regex- regex:'+chr(92)+'z',finished);assert results(s)=={regex_relative}
            s=act('recursive-locate',lambda s:bytes(s['path_raw'])==rawparent and s['selected']==regex_raw.hex() and not s['search']['visible'])
            capture('regex-locate-exact-raw-file');act('back');act('recursive-open')
            report['regex_scope']='regex query compiled once per helper scan, real batch and page IPC, multiline across32KiB boundary, UTF8 spans, invalid/NUL/file limit explicit partial, literal contrast, empty/EOF no invented highlight, invalid pattern no helper, exact raw Locate and unchanged fixture hashes; no native OS input'
        if args.calendar:
            act('recursive-key:ctrlf');act('recursive-text:ext:cal type:file')
            act('path-paste-prepare')
            s=act('recursive-calendar',lambda s:s['search']['calendar_open']and s['search']['calendar_controls']==6)
            capture('search-calendar-choices')
            s=act('path-paste-deliver');assert s['search']['query']=='ext:cal type:file'
            act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);capture('search-calendar-choices-440')
            spawn_before=log.read_text().count('FILES_SEARCH_HELPER_SPAWN')
            s=act('recursive-preset:today',lambda s:not s['search']['date_busy']and s['search']['date_range']is not None and not s['search']['calendar_open'])
            assert not s['search']['running']and s['search']['report']is None
            assert log.read_text().count('FILES_SEARCH_HELPER_SPAWN')==spawn_before
            bounds=s['search']['date_range'];reference=dt.datetime.fromtimestamp(bounds['reference'],zone);day=reference.date()
            assert day==today and bounds['start']==midnight(day)and bounds['end']==midnight(day+dt.timedelta(days=1))
            assert s['search']['fields'][2:]==[bounds['after'],bounds['before']]
            report['calendar_reference']=bounds
            capture('search-calendar-fixed-utc-fields-440')
            s=act('recursive-start-hit',finished);assert results(s)==calendar_expected['today']and s['search']['report']['complete']and s['search']['report']['metadata_reads']==7
            assert s['search']['date_range']==bounds;capture('search-calendar-today-raw-boundaries-440')
            act('fixture-wide',lambda s:s['width']==1000 and s['height']==560)
            act('recursive-key:f8',lambda s:s['search']['calendar_open']and s['search']['calendar_controls']==6)
            s=act('recursive-key:yesterday',lambda s:not s['search']['date_busy']and s['search']['date_range']['preset']=='Yesterday')
            assert not s['search']['running'];s=act('recursive-start-hit',finished);assert results(s)==calendar_expected['yesterday'];capture('search-calendar-yesterday')
            act('recursive-calendar',lambda s:s['search']['calendar_open']and s['search']['calendar_controls']==6)
            act('recursive-preset:seven',lambda s:not s['search']['date_busy']and s['search']['date_range']['preset']=='LastSevenDays')
            s=act('recursive-start-hit',finished);assert results(s)==calendar_expected['seven'];capture('search-calendar-seven-local-days')
            act('recursive-content-hit');act('recursive-text:raw calendar')
            act('recursive-calendar',lambda s:s['search']['calendar_open']and s['search']['calendar_controls']==6)
            s=act('recursive-preset:today',lambda s:not s['search']['date_busy']and not s['search']['calendar_open']and s['search']['date_range']['preset']=='Today')
            assert s['search']['field']==5 and s['search']['content']=='raw calendar'
            s=act('recursive-text: bytes');assert s['search']['content']=='raw calendar bytes'and s['search']['query']=='ext:cal type:file'
            s=act('recursive-start-hit',finished);assert results(s)=={raw_calendar}and s['search']['report']['content']is not None and s['search']['report']['metadata_requested']
            capture('search-calendar-content-and-raw-name')
            act('recursive-key:ctrlf');act('recursive-key:ctrla');act('recursive-key:backspace');act('recursive-content-hit')
            # Mixed date-only/timestamp bounds compare instants, not text length.
            act('recursive-filters');act('recursive-field:3');act('recursive-text:1970-01-01');act('recursive-field:4');act('recursive-text:1970-01-01T00:00:00Z')
            spawn_before=log.read_text().count('FILES_SEARCH_HELPER_SPAWN')
            s=act('recursive-start-hit',lambda s:bool(s['search']['error']));assert not s['search']['running']and log.read_text().count('FILES_SEARCH_HELPER_SPAWN')==spawn_before
            capture('search-calendar-equal-mixed-bounds-rejected')
            act('recursive-clear-filters');act('recursive-close');act('recursive-open')
            # Elapsed windows use the captured second, independent of local-midnight/DST.
            act('recursive-query:ext:roll type:file',finished)
            act('recursive-filters');act('recursive-field:1');act('recursive-text:1B');act('recursive-filters')
            act('recursive-content-hit');act('recursive-content-input:rolling owned')
            rolling=[]
            for preset,choice_action,span in [('LastHour','recursive-key:last-hour',3600),('Last24Hours','recursive-preset:24hours',86400),('Last168Hours','recursive-key:last-168hours',604800)]:
                act('recursive-calendar',lambda s:s['search']['calendar_open']and s['search']['calendar_controls']==6)
                if span==3600:
                    act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);capture('search-elapsed-choices-440')
                spawns=log.read_text().count('FILES_SEARCH_HELPER_SPAWN')
                s=act(choice_action,lambda s:not s['search']['date_busy']and not s['search']['calendar_open']and s['search']['date_range']['preset']==preset)
                bounds=s['search']['date_range'];assert bounds['end']==bounds['reference']and bounds['end']-bounds['start']==span
                assert s['search']['fields'][0]=='1B'and s['search']['content']=='rolling owned'and s['search']['query']=='ext:roll type:file'
                assert not s['search']['running']and log.read_text().count('FILES_SEARCH_HELPER_SPAWN')==spawns
                start=bounds['start']*1000000000;end=bounds['end']*1000000000
                stamps={b'before.roll':start-1,b'rolling-\xff.roll':start,b'inside.roll':start+1000000000,b'end-before.roll':end-1,b'end.roll':end,b'future.roll':end+1000000000}
                for name,stamp in stamps.items():
                    file=os.fsencode(source)+b'/'+name
                    with open(file,'wb') as f:f.write(b'rolling owned')
                    os.utime(file,ns=(stamp,stamp))
                s=act('recursive-start-hit',finished)
                assert results(s)=={b'rolling-\xff.roll',b'inside.roll',b'end-before.roll'}and s['search']['report']['metadata_reads']==6 and s['search']['report']['complete']
                assert s['search']['date_range']==bounds;rolling.append(bounds);capture('search-elapsed-'+str(span)+'-seconds')
            for name in stamps:os.unlink(os.fsencode(source)+b'/'+name)
            report['elapsed_ranges']=rolling
            report['elapsed_scope']='PASS actual AuxIo+SearchBatches;1/24/168h exact spans ending captured floored reference, inclusive start/exclusive end/subseconds/raw paths and AND size+literal criteria. Key4/key6 and painted24h;6metadata attempts per scan, no auto helper on choice;440px. No native input/clock snapshots/resource claim.'
            act('recursive-close');act('recursive-open')
            report['calendar_gate']='PASS'
            report['calendar_scope']='actual AuxIo libc local calendar conversion and SearchBatches helper; fixed reference/preset UTC fields; today/yesterday/seven calendar days, exact inclusive/exclusive+subsecond boundaries/raw path/type:no-follow and metadata reads; no automatic helper admission; mixed date/timestamp equal range rejects before helper; private keyboard/hits, no native input'
        if args.content:
            s=act('recursive-query:ext:ctxt',finished);assert len(results(s))==4 and s['search']['report']['content'] is None
            act('recursive-key:f4');act('recursive-field:1');act('recursive-text:1B');act('recursive-key:f4')
            s=act('recursive-content-hit');assert s['search']['field']==5 and s['search']['query']=='ext:ctxt'
            s=act('recursive-content-input:'+content_needle);assert s['search']['content']==content_needle
            act('recursive-point:5:0');s=act('recursive-drag:5:4');assert s['search']['selected_text']=='Hola' and s['search']['field']==5
            act('recursive-key:end');capture('content-raw-text-input')
            s=act('recursive-content-input:'+'x'*1025);assert s['search']['content']==content_needle and '1024' in s['search']['error'] and not s['search']['running']
            act('path-paste-prepare');act('recursive-content-hit');offset=command('path-paste-deliver');s=state(offset)
            assert 'FILES_PATH_PASTE accepted=false' in log.read_text()[offset:] and s['search']['query']=='ext:ctxt' and s['search']['content']==content_needle
            act('recursive-content-hit');s=act('recursive-start-hit',finished)
            r=s['search']['report'];assert results(s)=={b'good.ctxt'} and not r['complete'] and r['content']['binary']==2 and r['content']['limited']==1
            assert r['hits'][0]['content']['line']==2 and r['hits'][0]['content']['byte']==5
            snippet=r['hits'][0]['snippet'];encoded=snippet['text'].encode()
            assert encoded[snippet['start']:snippet['end']].decode()==content_needle
            assert len(encoded)<=256 and not any(c in snippet['text'] for c in '\n\r\t')

            capture('content-partial-binary-and-file-limit')
            s=act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);capture('content-results-440')
            act('fixture-wide',lambda s:s['width']==1000 and s['height']==560)
            act('recursive-key:f6');s=act('recursive-query:name:good ext:ctxt',finished)
            assert results(s)=={b'good.ctxt'} and s['search']['report']['complete'];capture('content-name-and-text-complete')
            act('recursive-content-hit');act('recursive-key:ctrla');s=act('recursive-key:backspace');assert not s['search']['content']
            act('recursive-content-hit')
            query='content:'+json.dumps(content_needle,ensure_ascii=False)+' name:needle'
            s=act('recursive-query:'+query,finished);assert results(s)=={rawdir+b'/'+rawname} and s['search']['report']['hits'][0]['content']['line']==2
            capture('content-advanced-raw-name')
            s=act('recursive-query:content:""',lambda s:bool(s['search']['error']));assert not s['search']['running'];capture('content-invalid-empty')
            s=act('recursive-query:name:context.snip content:"aguja"',finished)
            hit=s['search']['report']['hits'][0];snippet=hit['snippet'];encoded=snippet['text'].encode()
            assert results(s)=={b'context.snip'} and hit['content']=={'line':2,'byte':len(('ñ'*200+'\nbefore\t').encode())}
            assert encoded[snippet['start']:snippet['end']]==b'aguja' and snippet['leading'] and snippet['trailing'] and not snippet['match_clipped']
            assert len(encoded)<=256 and not any(c in snippet['text'] for c in '\n\r\t')
            capture('content-unicode-context-highlight')
            act('fixture-minimum',lambda s:s['width']==440);capture('content-unicode-context-highlight-440')
            act('fixture-wide',lambda s:s['width']==1000 and s['height']==560)
            s=act('recursive-query:name:long.snip content:'+json.dumps('ñ'*512,ensure_ascii=False),finished)
            snippet=s['search']['report']['hits'][0]['snippet'];encoded=snippet['text'].encode()
            assert snippet['match_clipped'] and snippet['trailing'] and not snippet['leading'] and snippet['start']==0 and snippet['end']==len(encoded)<=256
            assert encoded.decode()=='ñ'*128;capture('content-long-literal-clipped-fragment')
            report['snippet_scope']='<=256B normalized controls/whitespace; exact UTF8 byte-range highlight, Unicode leading/trailing context at440, full1024B literal match with clipped256B fragment; binary/invalid/limited files no snippets'

            act('recursive-close');act('recursive-open')
        if args.editing:
            act('recursive-text:日本 ext:rx')
            act('recursive-key:end');s=act('recursive-key:shiftleft');assert s['search']['selected_text']=='x'
            s=act('recursive-text:s');assert s['search']['query']=='日本 ext:rs'
            act('recursive-key:home');s=act('recursive-key:shiftright');assert s['search']['selected_text']=='日'
            capture('search-edit-unicode-selection')
            s=act('recursive-key:delete');assert s['search']['query']=='本 ext:rs'
            s=act('recursive-key:undo');assert s['search']['query']=='日本 ext:rs'
            s=act('recursive-key:redo');assert s['search']['query']=='本 ext:rs'
            act('recursive-key:undo')
            act('path-paste-prepare');act('recursive-key:right');offset=command('path-paste-deliver');s=state(offset)
            assert s['search']['query']=='日本 ext:rs' and 'FILES_PATH_PASTE accepted=false' in log.read_text()[offset:]
            act('recursive-key:ctrlf');act('recursive-text:ext:rx')
            act('recursive-point:0:5');s=act('recursive-drag:0:6');assert s['search']['selected_text']=='x'
            capture('search-edit-pointer-selection')
            s=act('recursive-text:s');assert s['search']['query']=='ext:rs'
            s=act('recursive-key:enter',finished);assert len(results(s))==4 and not s['search']['editing_input']
            act('recursive-key:ctrlf');s=act('recursive-key:home');assert len(results(s))==4 and s['search']['caret']==0
            s=act('recursive-key:tab');assert not s['search']['editing_input']
            s=act('recursive-key:tab');assert s['search']['editing_input'] and s['search']['selected_text']=='ext:rs'
            s=act('recursive-key:down');assert not s['search']['editing_input']
            s=act('recursive-key:end');assert s['search']['selected']==3
            s=act('recursive-key:home');assert s['search']['selected']==0
            capture('search-edit-results-kept-while-moving-caret')
            act('recursive-key:f4');act('recursive-text:12MiB');act('recursive-key:home');act('recursive-key:delete')
            s=act('recursive-text:2');assert s['search']['fields'][0]=='22MiB'
            s=act('recursive-key:undo');assert s['search']['fields'][0]=='2MiB'
            s=act('recursive-key:redo');assert s['search']['fields'][0]=='22MiB'
            capture('search-edit-criterion-middle')
            act('recursive-clear-filters');act('recursive-key:escape') if state()['search']['filters_open'] else None
            act('recursive-key:ctrlf');s=act('recursive-text:name:'+'a'*4091);assert len(s['search']['query'])==4096
            act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);capture('search-edit-long-query-end-440')
            act('recursive-key:home');capture('search-edit-long-query-start-440')
            act('recursive-key:ctrlf');act('recursive-text:ext:rs')
            act('fixture-wide',lambda s:s['width']==1000 and s['height']==560)

        if args.progress:
            s=act('recursive-query:ext:dat size:>10MiB',lambda s:s['search']['progress'] is not None and s['search']['progress']['visited']>0 and s['search']['running'] and s['search']['report'] is None)
            report['observed_positive_progress']=s['search']['progress'];assert s['search']['row_hits']==0
            capture('search-live-counts-no-unvalidated-rows')
            offset=command('recursive-key:enter');s=state(offset)
            phase=re.search(r'FILES_SEARCH_ENTER current_before=(true|false) form_before=(true|false) hits_before=(\d+) input_changed=(true|false) current_after=(true|false) had_summary=(true|false)',log.read_text()[offset:]);assert phase
            report['enter_phase']=dict(current_before=phase[1],form_before=phase[2],hits_before=int(phase[3]),input_changed=phase[4],current_after=phase[5],had_summary=phase[6])
            assert phase[2]=='false' and int(phase[3])==0
            if phase[1]=='true':assert phase[4]=='false' # No restart while genuinely active.
            else:assert phase[4]=='true' and phase[5]=='true' # Completed empty result: explicit refresh.

            s=act('recursive-cancel',lambda s:not s['search']['running'])
            report['progress_cancel_outcome']='finished_before_cancel' if s['search']['report'] is not None else 'cancelled_before_ack'
            assert s['search']['progress'] is None
            s=act('recursive-query:needle ext:rs',finished);assert results(s)=={rawdir+b'/'+rawname}
            capture('search-after-progress-cancel-latest-query')

        if args.controls:
            act('recursive-key:ctrlf')
            act('recursive-text:ext:bin')
            s=act('recursive-filters');assert s['search']['filters_open'] and s['search']['field']==1
            act('recursive-text:1KiB');s=act('recursive-key:tab');assert s['search']['field']==2
            act('recursive-text:3KiB');s=act('recursive-key:tab');assert s['search']['field']==3
            act('recursive-text:1970-01-01');s=act('recursive-key:tab');assert s['search']['field']==4
            act('recursive-text:1970-01-03');s=act('recursive-key:backtab');assert s['search']['field']==3
            s=act('recursive-key:tab');assert s['search']['field']==4
            capture('search-controls-four-criteria')
            act('path-paste-prepare');act('recursive-field:2');offset=command('path-paste-deliver');s=state(offset)
            assert s['search']['fields'][1]=='3KiB' and 'FILES_PATH_PASTE accepted=false' in log.read_text()[offset:]
            s=act('fixture-short',lambda s:s['width']==800 and s['height']==440);capture('search-controls-800x440')
            s=act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);capture('search-controls-440x440')
            s=act('recursive-key:enter',finished)
            assert results(s)=={b'old-large.bin'} and not s['search']['filters_open']
            assert s['search']['query']=='ext:bin' and s['search']['fields']==['1KiB','3KiB','1970-01-01','1970-01-03']
            capture('search-controls-results-440x440')
            s=act('recursive-key:f4');assert s['search']['row_hits']==0 and len(s['search']['report']['hits'])==1
            capture('search-controls-retained-results-drawer-440x440')
            act('recursive-key:escape')
            act('fixture-short',lambda s:s['width']==800 and s['height']==440)
            act('recursive-key:f4');act('recursive-field:1');act('recursive-text:4MiB')
            s=act('recursive-start-hit',lambda s:bool(s['search']['error']))
            assert not s['search']['running'] and s['search']['filters_open'];capture('search-controls-invalid-range')
            s=act('recursive-clear-filters');assert s['search']['fields']==['']*4 and s['search']['query']=='ext:bin'
            s=act('recursive-start-hit',finished);assert results(s)=={b'old-large.bin',b'recent-small.bin'}
            capture('search-controls-clear-preserves-name')
            # Keyboard Escape closes the drawer first and retains the search/results.
            act('recursive-key:f4');s=act('recursive-key:escape');assert s['search']['visible'] and not s['search']['filters_open']
            act('fixture-wide',lambda s:s['width']==1000 and s['height']==560)

        s=act('recursive-query:ext:rs type:file',finished)
        expected={b'app/src/main.rs',b'app/tests/main.rs',b'rootnote.rs',rawdir+b'/'+rawname}
        assert results(s)==expected and s['search']['report']['complete'];assert not s['search']['error'];capture('recursive-results-and-parents')
        assert s['search']['report']['metadata_reads']==0 and not s['search']['report']['metadata_requested']
        s=act('recursive-query:ext:bin size:>=1KiB before:1970-01-03',finished)
        assert results(s)=={b'old-large.bin'} and s['search']['report']['metadata_reads']==2 and s['search']['report']['complete']
        capture('recursive-size-date-facets')
        s=act('recursive-query:ext:bin !size:>=1KiB after:2026-01-01',finished)
        assert results(s)=={b'recent-small.bin'}
        capture('recursive-size-exclusion-date')
        s=act('recursive-query:size:wat',lambda s:bool(s['search']['error']))
        assert not s['search']['running']
        capture('recursive-invalid-size')

        s=act('recursive-hidden');assert not s['show_hidden'] and s['search']['hidden']
        s=act('recursive-query:ext:rs type:file',finished);assert results(s)==expected|{b'.secret/private.rs'};capture('recursive-hidden-local-scope')
        act('recursive-hidden')
        s=act('recursive-query:needle ext:rs',finished);assert results(s)=={rawdir+b'/'+rawname}
        capture('recursive-raw-result')
        s=act('recursive-locate',lambda s:bytes(s['path_raw'])==rawparent and s['selected']==rawname.hex() and s['selected'] in s['visible'] and not s['search']['visible'])
        assert not s['search']['visible'] and s['selected'] in s['visible'];capture('recursive-located-raw-file')
        act('back');act('recursive-open');s=act('recursive-query:ext:txt',finished)
        assert len(s['search']['report']['hits'])==500 and not s['search']['report']['complete'] and s['search']['report']['reason'];capture('recursive-partial-limit')
        s=act('fixture-short',lambda s:s['width']==800 and s['height']==440);capture('recursive-small-window')
        s=act('recursive-query:does-not-exist',finished);assert not results(s) and s['search']['report']['complete']
        s=act('recursive-query:type:invalid',lambda s:bool(s['search']['error']));assert not s['search']['running'];capture('recursive-invalid-filter')
        s=act('recursive-query:type:dir name:app',finished);assert results(s)=={b'app'}
        s=act('recursive-enter',lambda s:s['path']==str(source/'app') and not s['search']['visible']);capture('recursive-open-directory')
        act('back');act('recursive-open')
        command('recursive-query:never-match')
        s=act('recursive-cancel',lambda s:not s['search']['running'])
        if s['search']['report'] is not None: assert not results(s) and s['search']['report']['complete']
        report['cancel_outcome']='cancelled_before_ack' if s['search']['report'] is None else 'finished_before_cancel'
        # Latest intent and root close still use actual helper IPC; active-at-close is not asserted.
        command('recursive-query:ext:rs')
        command('callback');wait(lambda:process.poll() is not None,'normal close missing',True)
        text=log.read_text();assert process.returncode==0 and 'FILES_CLOSE_DRAINED' in text
        identities=set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=(?:search_stream|search_batches|search_pages)',text));assert identities
        for pid,start in identities:
            try:
                parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
            except FileNotFoundError:continue
            assert parts[19]!=start,'helper identity still exists, including zombie: '+pid
        for name,digest in before.items():assert hashlib.sha256((source/name).read_bytes()).hexdigest()==digest
        assert (outside/'external.rs').read_bytes()==b'must not enter\n' and (source/'app/src/cycle').is_symlink()
        report['content_search']=args.content
        if args.content:report['content_scope']='plain UTF8 content field preserves name AND filter, quotes/backslash raw/pointer geometry5, stale paste rejected on mode switch, bounded binary/file-limit partial vs restricted complete, line2/byte5, advanced escaped literal and raw name, empty criterion rejects; rest is existing recursive regression; no native input'
        report['query_editing']=args.editing
        if args.editing:report['editing_scope']='UTF8 middle replacement, selection Delete, undo/redo, private caret-stale paste rejected, pointer/drag from own painted byte positions, real query execution, cursor motion retains final report, result vs edit HomeEnd, criteria middle edit/undo, max4096B horizontal viewport at440; no native input/clipboard/IME/shaped grapheme claim'
        report['streamed_progress']=args.progress
        report['guided_controls']=args.controls
        if args.controls:report['guided_scope']='four painted fields, F4/Tab/ShiftTab/Esc/Enter shared policy, raw query AND criteria, invalid range no helper, clear preserves name query, stale text callback rejected on focus switch, 800x440/440x440 layouts, retained results hidden without row targets when drawer leaves less than a row; private handlers, not native OS input'
        report.update(functional_gate='PASS',normal_exit=True,logged_helper_identities=sorted(identities),helper_cleanup='logged PID/start identities absent or reused; Z not accepted; not lifetime coverage',scope='size/date UTC facets/negation/lazy metadata/no-clobber observation/recursive names/duplicate parents/raw locate/local hidden/cycle exclusion/500 explicit partial/no matches/invalid filter/800x440/directory enter/latest intent/callback drain; no native input, mounted-FS runtime fixture, live progress streaming, active helper at close or performance claim');normal=True
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['log']=log.read_text() if log.exists() else ''
        if process is not None and process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=hashlib.sha256(((Path.home()/'.local/bin'/name) if args.installed else ROOT/'target/release'/name).read_bytes()).hexdigest()
        target=OUT/(('search-calendar-ui-' if args.calendar else 'content-search-ui-' if args.content else 'search-editing-ui-' if args.editing else 'search-progress-ui-' if args.progress else 'search-controls-ui-' if args.controls else 'recursive-search-ui-')+str(time.time_ns())+'.json')
        with target.open('x') as output:json.dump(report,output,ensure_ascii=False,indent=2)
        print(target)
