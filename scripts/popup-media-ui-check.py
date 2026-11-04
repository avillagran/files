#!/usr/bin/env python3
"""Passive framebuffer observation during async inspector work; no OS input."""
from pathlib import Path
import argparse,json,os,re,signal,struct,subprocess,tempfile,time,zlib,hashlib
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=ROOT/'target/release/files');args=parser.parse_args()
OUT=ROOT/'docs/evidence/popup-media-20261104';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'FAIL','method':'FLTK right press/hold/release phases, passive capture without flush; async thumbnail and periodic refresh; NOT OS input','captures':[]}
with tempfile.TemporaryDirectory(prefix='popup-media-',dir=BASE) as tmp:
 b=Path(tmp);folder=b/'files';folder.mkdir();log=b/'app.log';raw=b/'frame.ppm'
 def chunk(k,d):return struct.pack('>I',len(d))+k+d+struct.pack('>I',zlib.crc32(k+d)&0xffffffff)
 w,h=3200,2400;pixels=zlib.compress((b'\0'+bytes([210,30,70])*w)*h)
 (folder/'red.png').write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',pixels)+chunk(b'IEND',b''))
 env={**os.environ,'FILES_CONFIG_DIR':str(b/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_TOOLKIT_POPUP':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_CLOSE_TRACE':'1','FILES_TEST_FIXED_SIZE':'1'}
 with log.open('w') as out:p=subprocess.Popen([str(args.binary),str(folder),'--size','1000x650','--no-session','--ui-smoke','--exit-after','30'],env=env,stderr=out,stdout=subprocess.DEVNULL,start_new_session=True)
 def wait(check,label,timeout=10):
  end=time.monotonic()+timeout
  while time.monotonic()<end:
   value=check()
   if value:return value
   if p.poll() is not None:raise AssertionError(f'{label}: exit{p.returncode}')
   time.sleep(.01)
  raise AssertionError(label)
 def command(text):
  before=len(log.read_text());f=raw.with_suffix('.pending');f.write_text(text);f.replace(raw.with_suffix('.popup-command'))
  wait(lambda:f'FILES_TOOLKIT_FIXTURE action={text}' in log.read_text()[before:],text)
  return log.read_text()[before:]
 seq=0
 def capture(label):
  global seq
  seq+=1;raw.unlink(missing_ok=True);raw.with_suffix('.request').write_text(json.dumps({'id':seq,'passive':True}))
  def ready():
   if not raw.exists():return None
   a=raw.read_bytes().split(b'\n',3)
   if len(a)!=4:return None
   width,height=map(int,a[1].split())
   return (width,height,a[3]) if len(a[3])==width*height*3 else None
  frame=wait(ready,'passive capture');name=OUT/f'{os.environ.get("FLTK_BACKEND","auto")}-{label}-{time.time_ns()}.ppm';name.write_bytes(raw.read_bytes());report['captures'].append(str(name));fw,fh,rgb=frame;py=140+max(38,min(98,(fh-424)//2))//2;start=(py*fw+fw-120)*3;report.setdefault('inspector_samples',[]).append(list(rgb[start:start+3]));return frame
 try:
  wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'smoke')
  command('media-inspector')
  for _ in range(80):
   ready=command('media-state')
   if 'watch_settled=true' in ready and '"painted":1' in ready and '"ready":[{' in ready:break
   time.sleep(.04)
  else:raise AssertionError('initial inspector image never became ready')
  for index in range(48):
   time.sleep(.06);capture(f'steady-{index}')
   assert report['inspector_samples'][-1]==[210,30,70],f'inspector blinked at steady sample {index}'
  report['steady_inspector_samples']=48
  text=command('right-hold');wait(lambda:'FILES_TOOLKIT_POPUP_OPEN kind=Context' in log.read_text(),'menu open')
  text=log.read_text();rect=list(map(int,re.findall(r'kind=Context rect=Some\(\((\d+), (\d+), (\d+), (\d+)\)\)',text)[-1]))
  time.sleep(.06);before=capture('held');x,y,cw,ch=rect
  def roi(f):
   width,height,pixels=f;assert x+cw<=width and y+ch<=height
   return b''.join(pixels[((y+i)*width+x)*3:((y+i)*width+x+cw)*3] for i in range(ch))
  # Release the original pushed canvas through normal FLTK routing.
  command('right-release');time.sleep(.06);after=capture('released');assert roi(before)==roi(after),'menu pixels changed after normal routed release'
  for index in range(5):
   time.sleep(.65);frame=capture(f'async-{index}');assert roi(before)==roi(frame),f'menu pixels changed during background work ({index})'
  command('media-state');state=log.read_text().split('FILES_POPUP_MEDIA ')[-1].split('\n')[0];report['state']=state
  assert 'menu=true' in state,'menu logically dismissed'
  assert '"ready":[]' not in state,'no ready thumbnail observed'
  assert '"painted":1' in state,'inspector not visibly painted'
  command('escape');command('context');time.sleep(.08);overlap=capture('overlap');rect=list(map(int,re.findall(r'kind=Context rect=Some\(\((\d+), (\d+), (\d+), (\d+)\)\)',log.read_text())[-1]));x,y,cw,ch=rect
  for index in range(4):
   time.sleep(.65);frame=capture(f'overlap-{index}');assert roi(overlap)==roi(frame),f'overlapping menu changed during async work ({index})'
  command('escape');command('close');p.wait(timeout=12);assert p.returncode==0;report['functional_gate']='PASS'
 except BaseException as e:
  report['error']=repr(e)
  if p.poll() is None:
   try:command('media-state')
   except BaseException:pass
 finally:
  if p.poll() is None:os.killpg(p.pid,signal.SIGTERM);p.wait(timeout=10)
  report['log']=log.read_text()
report['binary_path']=str(args.binary.resolve());report['binary_sha256']=hashlib.sha256(args.binary.read_bytes()).hexdigest();report['helper_sha256']=hashlib.sha256(args.binary.with_name('files-helper').read_bytes()).hexdigest();report['html_helper_sha256']=hashlib.sha256(args.binary.with_name('files-html-helper').read_bytes()).hexdigest()
path=OUT/f'report-{time.time_ns()}.json';path.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'report':str(path),'gate':report['functional_gate'],'error':report.get('error')}));raise SystemExit(0 if report['functional_gate']=='PASS' else 1)
