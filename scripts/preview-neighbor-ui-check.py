#!/usr/bin/env python3
"""Preview buttons: raw names, hidden filtering, boundaries, errors, latest-wins.
Internal Action/hit targets and actual FLTK framebuffer. No OS input or metrics.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import tempfile
import time
import zlib
from memcheck import geometry, observed_geometry, stop_tree

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--rotation", action="store_true", help="Asymmetric image/PDF/video/GIF rotation, view only")
args = parser.parse_args()
ROOT = Path(__file__).resolve().parent.parent
BASE = Path.home() / '.cache/files-test'
BASE.mkdir(parents=True, exist_ok=True)
EVIDENCE = ROOT / 'docs/evidence/0.0.4'
report = {'rotation_fixture': args.rotation, 'functional_gate': 'FAIL', 'resource_gate': 'NOT_MEASURED',
          'startup_gate': 'NOT_MEASURED', 'captures': [],
          'method': 'shared UI Action and drawn hit targets; actual FLTK framebuffer, no native OS input/clipboard or resource sampling'}
instances = json.loads(subprocess.check_output(['hyprctl', 'instances', '-j'], timeout=2))
matches = [i for i in instances if i.get('wl_socket') == os.environ.get('WAYLAND_DISPLAY')]
assert len(matches) == 1, 'unique compositor required'
os.environ['HYPRLAND_INSTANCE_SIGNATURE'] = matches[0]['instance']

def png(path, rgb):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)
    rows = (b'\0' + bytes(rgb) * 640) * 480 if rgb else b''.join(b'\0' + bytes((255,0,0) if y<240 else (0,0,255))*320 + bytes((0,255,0) if y<240 else (255,255,0))*320 for y in range(480))
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 640, 480, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))

try:
    with tempfile.TemporaryDirectory(prefix='preview-neighbor-', dir=BASE) as temporary:
        test = Path(temporary)
        folder = test / 'files'; folder.mkdir()
        private = test / 'tmp'; private.mkdir()
        env = {**os.environ, 'TMPDIR': str(private), 'MAGICK_TEMPORARY_PATH': str(private),
               'FILES_CONFIG_DIR': str(test / 'config'), 'FILES_TEST_PREVIEW': '1',
               'FILES_TEST_FIXED_SIZE': '1', 'FILES_TEST_ISOLATE_INPUT': '1'}
        png(folder / 'a.png', (220, 20, 40))
        blue = test / 'blue.png'; png(blue, (20, 40, 220))
        subprocess.run(['magick', str(blue), '-define', 'webp:lossless=true', str(folder / 'b.webp')], env=env, check=True, timeout=5)
        png(folder / os.fsdecode(b'c-\xff.png'), (20, 220, 40))
        png(folder / '.hidden.png', (220, 20, 40))
        (folder / 'd-bad.png').write_bytes(b'not PNG')
        if args.rotation:
            png(folder / 'quadrants.png', None)
            for name, codec in [('clip.mkv','ffv1'),('clip.gif','gif')]:
                cmd=['ffmpeg','-v','error','-nostdin','-loop','1','-i',str(folder/'quadrants.png'),'-t','4','-r','5','-c:v',codec,'-threads','1']
                if codec=='gif':cmd += ['-loop','0']
                subprocess.run([*cmd,str(folder/name)],env=env,check=True,timeout=15)
            stream=b'1 0 0 rg 0 120 160 120 re f 0 1 0 rg 160 120 160 120 re f 0 0 1 rg 0 0 160 120 re f 1 1 0 rg 160 0 160 120 re f'
            objects=[b'<< /Type /Catalog /Pages 2 0 R >>',b'<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>']
            for index in (4,6):
                objects += [b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 240] /Resources << >> /Contents '+str(index).encode()+b' 0 R >>',b'<< /Length '+str(len(stream)).encode()+b' >>\nstream\n'+stream+b'\nendstream']
            data=b'%PDF-1.4\n';offsets=[]
            for index,obj in enumerate(objects,1):
                offsets.append(len(data));data+=str(index).encode()+b' 0 obj\n'+obj+b'\nendobj\n'
            xref=len(data);data+=b'xref\n0 7\n0000000000 65535 f \n'+b''.join(f'{offset:010} 00000 n \n'.encode() for offset in offsets)+b'trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n'+str(xref).encode()+b'\n%%EOF\n';(folder/'pages.pdf').write_bytes(data)
        original_sources = {os.fsencode(path.name).hex():hashlib.sha256(path.read_bytes()).hexdigest() for path in folder.iterdir()}
        raw = test / 'capture.ppm'; log = test / 'app.log'
        env['FILES_TEST_CAPTURE'] = str(raw)
        with log.open('w') as output:
            process = subprocess.Popen([str(ROOT / 'target/release/files'), str(folder), '--size', '1000x650', '--scale', '1', '--ui-smoke', '--ui-scene', 'image', '--exit-after', '60'], env=env, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
        normal = False
        def wait(check, reason, allow_exit=False):
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                value = check()
                if value: return value
                if process.poll() is not None and not allow_exit: raise AssertionError(reason + '; root exited')
                time.sleep(.025)
            raise AssertionError(reason)
        def command(action):
            after = len(log.read_text())
            pending = raw.with_suffix('.pending'); pending.write_text(action)
            os.replace(pending, raw.with_suffix('.command'))
            match = wait(lambda: re.search(r'FILES_PREVIEW_COMMAND_REV action=' + re.escape(action) + r' revision=(\d+)', log.read_text()[after:]), 'command failed: ' + action, action == 'callback')
            return int(match[1]), after
        def ready(action, kind="IMAGE", contains=None):
            revision, after = command(action)
            wait(lambda: any(r.startswith('FILES_PREVIEW_'+kind+'_READY ') and f'revision={revision} ' in r and (contains is None or contains in r) for r in log.read_text()[after:].splitlines()), 'fresh image Ready missing: ' + action)
            return revision
        def capture(name):
            raw.unlink(missing_ok=True); raw.with_suffix('.request').touch()
            def complete():
                if not raw.exists(): return False
                parts = raw.read_bytes().split(b'\n', 3)
                return len(parts) == 4 and len(parts[3]) == int(parts[1].split()[0]) * int(parts[1].split()[1]) * 3
            wait(complete, 'framebuffer incomplete')
            path = EVIDENCE / (name + '-' + str(time.time_ns()) + '.png')
            subprocess.run(['magick', str(raw), str(path)], env=env, check=True, timeout=5)
            report['captures'].append(str(path))
        try:
            wait(lambda: 'FILES_UI_SMOKE_OK' in log.read_text(), 'UI smoke missing')
            wait(lambda: geometry(process.pid, 1000, 650) and observed_geometry(process.pid, 1000, 650), 'geometry absent')
            report['geometry'] = observed_geometry(process.pid, 1000, 650)
            if args.rotation:
                report['quadrant_checks'] = []
                def quadrants(turns, name):
                    capture(name)
                    _, after = command('rotation-state')
                    marker=re.search(r'FILES_PREVIEW_ROTATION turns=(\d+) zoom=([0-9.]+) body=(-?\d+),(-?\d+),(\d+),(\d+)',log.read_text()[after:])
                    assert marker and int(marker[1])==turns and float(marker[2])==1., 'unexpected transform'
                    x,y,w,h=map(int,marker.group(3,4,5,6))
                    ow,oh=(480,640) if turns%2 else (640,480)
                    scale=min(w/ow,h/oh,1.);dw,dh=ow*scale,oh*scale
                    left,top=x+max(0.,w-dw)/2,y+max(0.,h-dh)/2
                    header=raw.read_bytes().split(b'\n',3);fw,fh=map(int,header[1].split());pixels=header[3]
                    order=[[(255,0,0),(0,255,0),(0,0,255),(255,255,0)],[(0,0,255),(255,0,0),(255,255,0),(0,255,0)],[(255,255,0),(0,0,255),(0,255,0),(255,0,0)],[(0,255,0),(255,255,0),(255,0,0),(0,0,255)]][turns]
                    actual=[]
                    for (fx,fy),expected in zip([(.25,.25),(.75,.25),(.25,.75),(.75,.75)],order):
                        px,py=int(left+dw*fx),int(top+dh*fy)
                        assert 0<=px<fw and 0<=py<fh
                        rgb=list(pixels[(py*fw+px)*3:(py*fw+px)*3+3]);actual.append(rgb)
                        assert all(abs(a-b)<=8 for a,b in zip(rgb,expected)), (name,turns,rgb,expected)
                    report['quadrant_checks'].append({'capture':name,'quarter_turns':turns,'actual_framebuffer_samples':actual,'body':[x,y,w,h]})
                ready('rotation-open');quadrants(0,'preview-rotation-image-original')
                command('rotation-right');quadrants(1,'preview-rotation-image-clockwise')
                command('rotation-left');command('rotation-left');quadrants(3,'preview-rotation-image-counterclockwise')
                command('rotation-reset');quadrants(0,'preview-rotation-image-reset')
                for action in ('video','gif'):
                    ready(action,'VIDEO');command('rotation-right');quadrants(1,'preview-rotation-'+action+'-snapshot')
                    revision,after=command('video-play')
                    def played():
                        return next((r for r in reversed(log.read_text()[after:].splitlines()) if r.startswith('FILES_PREVIEW_VIDEO_READY ') and f'revision={revision} ' in r and 'playing=true ' in r and re.search(r'sequence=Some\(([4-9]|[1-9][0-9]+)\)',r)),False)
                    wait(played,'rotated stream frames missing');quadrants(1,'preview-rotation-'+action+'-playing')
                    _,after=command('video-pause')
                    wait(lambda:any(r.startswith('FILES_PREVIEW_VIDEO_READY ') and 'playing=false ' in r and 'drained=true ' in r for r in log.read_text()[after:].splitlines()),'pause ACK missing')
                    ready('video-mid','VIDEO');quadrants(1,'preview-rotation-'+action+'-seek')
                ready('pdf','PDF','page=1 ');command('rotation-right');quadrants(1,'preview-rotation-pdf-first')
                ready('pdf-next','PDF','page=2 ');quadrants(1,'preview-rotation-pdf-second')
                ready('pdf-text','PDF_TEXT');ready('pdf-text','PDF','page=2 ');quadrants(1,'preview-rotation-pdf-text-return')
                ready('rotation-open');quadrants(0,'preview-rotation-new-source-reset')
                report['source_bytes_unchanged'] = original_sources == {os.fsencode(path.name).hex():hashlib.sha256(path.read_bytes()).hexdigest() for path in folder.iterdir()}
                assert report['source_bytes_unchanged']
            else:
                first = ready('neighbor-first'); command('neighbor-assert-red'); capture('preview-neighbor-first')
                before = log.read_text().count('FILES_PREVIEW_HELPER ')
                boundary, _ = command('neighbor-back')
                assert boundary == first, 'boundary unnecessarily reopened current item'
                command('neighbor-assert-red')
                assert log.read_text().count('FILES_PREVIEW_HELPER ') == before, 'boundary launched helper'
                ready('neighbor-next'); command('neighbor-assert-blue'); capture('preview-neighbor-next-webp')
                ready('neighbor-next'); command('neighbor-assert-raw-green'); capture('preview-neighbor-raw-name')
                _, after = command('neighbor-next')
                wait(lambda: any(r.startswith('FILES_PREVIEW_ERROR ') and '/d-bad.png"' in r for r in log.read_text()[after:].splitlines()), 'fresh corrupt-file error absent')
                capture('preview-neighbor-error-controls')
                ready('neighbor-back'); command('neighbor-assert-raw-green')
                ready('neighbor-rapid'); command('neighbor-assert-blue'); capture('preview-neighbor-latest-wins')
            command('callback')
            wait(lambda: process.poll() is not None, 'normal close failed', True)
            text = log.read_text()
            assert process.returncode == 0 and 'FILES_PREVIEW_DRAINED' in text, 'exit0 with ACK required'
            identities = sorted(set((int(p), int(s)) for p, s in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=', text)))
            assert identities, 'helper identities missing'
            for pid, start in identities:
                proc = Path(f'/proc/{pid}')
                for attempt in range(2):
                    try:
                        current = (proc / 'stat').read_text().rsplit(')', 1)[1].split()
                        assert int(current[19]) != start, f'known helper survived or zombie: {pid}'
                        break
                    except (FileNotFoundError, ProcessLookupError):
                        if not proc.exists(): break
                        if attempt: raise AssertionError('known PID directory with missing stat')
            normal = True
            if not args.rotation:report.update(boundary_no_decode=True,hidden_excluded=True,raw_name_and_effective_source=True,drawn_buttons_and_names=True,latest_wins_and_error_recovery=True)
            report.update(functional_gate='PASS', normal_callback_exit0_after_ack=True,
                          logged_helper_cleanup={'complete': True, 'identities': identities, 'zombies_accepted': False, 'scope': 'logged PID/start identities after exit, not lifetime coverage'}, log=text)
        finally:
            if not normal: stop_tree(process, None)
            report.setdefault('log', log.read_text())
except BaseException as error:
    report['error'] = repr(error)
    raise
finally:
    for name, key in [('files', 'binary_sha256'), ('files-helper', 'helper_sha256'), ('files-html-helper', 'html_helper_sha256')]:
        report[key] = hashlib.sha256((ROOT / 'target/release' / name).read_bytes()).hexdigest()
    path = EVIDENCE / (('preview-rotation-ui-' if args.rotation else 'preview-neighbor-ui-') + str(time.time_ns()) + '.json')
    with path.open('x') as output: json.dump(report, output, indent=2)
    print(json.dumps({'report': str(path), 'functional_gate': report['functional_gate'], 'resource_gate': 'NOT_MEASURED'}))
