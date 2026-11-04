#!/usr/bin/env python3
"""Escenas reales Wayland; control interno de estados, sin simular clics del SO."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import importlib.util

ROOT = Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('memcheck',ROOT/'scripts/memcheck.py')
memcheck=importlib.util.module_from_spec(spec)
spec.loader.exec_module(memcheck)


def wait_for(check, message, timeout=5):
    start=time.monotonic()
    while time.monotonic()-start<timeout:
        result=check()
        if result:
            return result
        time.sleep(.03)
    raise RuntimeError(message)


def capture(pid,name):
    raw=CAPTURE_PATH
    raw.unlink(missing_ok=True)
    raw.with_suffix('.request').touch()
    wait_for(lambda: raw.exists() and raw.stat().st_size > 1000, 'No llegó la captura del framebuffer')
    time.sleep(.1)
    output=ROOT/os.environ.get('FILES_EVIDENCE_DIR','docs/evidence/0.0.2');output.mkdir(parents=True,exist_ok=True)
    path=output/f'{name}.png'
    subprocess.run(['magick',str(raw),str(path)],check=True)
    return str(path.relative_to(ROOT))


def main():
    base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
    evidence=[]
    active=Path.home()/'.local/state/omarchy/current/theme/colors.toml'
    hackerman=active.read_text()
    tokyo=(ROOT/'themes/tokyo-night.toml').read_text()
    with tempfile.TemporaryDirectory(prefix='ui-',dir=base) as folder:
        test=Path(folder);files=test/'files';files.mkdir()
        for name in ['Projects','Screenshots','Documents']:
            d=files/name;d.mkdir()
            for n in range(3):(d/f'note-{n}.txt').write_text('files\n'*100)
        for name,size,days in [('screenshot.png',774000,0),('brag.jpg',194000,0),('upgrade.md',18000,0),('install-arch.sh',6100,3),('release.zip',13800000,5),('notes.txt',101,14)]:
            p=files/name
            with p.open('wb') as f:f.truncate(size)
            stamp=time.time()-days*86400;os.utime(p,(stamp,stamp))
        for name,text in [('hackerman',hackerman),('tokyo',tokyo)]:
            (test/name).mkdir();(test/name/'colors.toml').write_text(text)
        (test/'current').mkdir();(test/'current/theme').symlink_to(test/'hackerman')
        global CAPTURE_PATH
        CAPTURE_PATH=test/'capture.ppm'
        env={**os.environ,'FILES_TEST_CAPTURE':str(CAPTURE_PATH),'FILES_THEME':str(test/'current/theme/colors.toml'),'FILES_CONFIG_DIR':str(test/'config/files')}
        def launch(scene=''):
            log=test/f'{scene or "groups"}.log'
            with log.open('w') as stream:
                args=[str(ROOT/'target/release/files'),str(files),'--size','1000x650','--scale','1','--ui-smoke']
                if scene:args+=['--ui-scene',scene]
                process=subprocess.Popen(args,stdout=subprocess.DEVNULL,stderr=stream,env=env)
            try:
                wait_for(lambda: 'FILES_UI_SMOKE_OK' in log.read_text(), 'Smoke no completó: '+str(log))
                wait_for(lambda: memcheck.geometry(process.pid,1000,650), 'Ventana ausente')
                time.sleep(2.1)
                for attempt in range(4):
                    memcheck.geometry(process.pid,1000,650)
                    time.sleep(.25)
                    client=next(c for c in json.loads(subprocess.check_output(['hyprctl','clients','-j'])) if c['pid']==process.pid)
                    if client['size']==[1000,650]:break
                assert client['size']==[1000,650], client['size']
                return process,log
            except BaseException:
                process.terminate();process.wait(timeout=3)
                raise
        (test/'config/files').mkdir(parents=True)
        (test/'config/files/config.toml').write_text('effects = false\n[future]\nvalue = 42\n')
        process,log=launch()
        try:
            evidence.append(capture(process.pid,'hackerman'))
            (test/'hackerman/colors.toml').write_text('background = "#bad')
            wait_for(lambda:'FILES_THEME_REJECTED' in log.read_text(),'No rechazó el TOML incompleto')
            assert 'FILES_THEME_REJECTED background=#05182e' in log.read_text()
            (test/'current/next').symlink_to(test/'tokyo');os.replace(test/'current/next',test/'current/theme')
            wait_for(lambda:'FILES_THEME_RELOADED background=#1a1b26' in log.read_text(),'No recargó el symlink')
            time.sleep(.3);evidence.append(capture(process.pid,'tokyo-night'))
            # Reemplazo atómico dentro del nuevo destino: comprueba reattachment.
            (test/'tokyo/new.toml').write_text(hackerman);os.replace(test/'tokyo/new.toml',test/'tokyo/colors.toml')
            wait_for(lambda:'FILES_THEME_RELOADED background=#05182e' in log.read_text(),'No vigila el nuevo destino')
        finally:
            process.terminate();process.wait(timeout=3)
        for scene in ['rounded','square','list','rail','spatial']:
            process,log=launch(scene)
            try:
                evidence.append(capture(process.pid,scene))
                if scene in ['rounded','square']:
                    import tomllib
                    config=tomllib.loads((test/'config/files/config.toml').read_text())
                    assert config['effects'] is False and config['future']['value']==42
                    assert f'corners = "{scene}"' in (test/'config/files/config.toml').read_text()
            finally:
                process.terminate();process.wait(timeout=3)
        print(json.dumps({'ui_smoke':'passed','theme_partial_write':'retained last valid palette','theme_symlink_swap':'passed','theme_atomic_file_replacement':'passed','corners_persisted':['rounded','square'],'captures':evidence,'capture_method':'FLTK live window framebuffer (no compositor overlays)','native_input_driver':'unavailable: CUA lists zero windows; scenes use in-process actions'},indent=2))

if __name__=='__main__':main()
