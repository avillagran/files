#!/usr/bin/env python3
"""Nautilus floor, block 1, in a real Files window (internal handlers, no OS
input, private fixtures and a private trash via XDG_DATA_HOME):

- Shift+Delete asks first, then deletes files and whole trees permanently
  without following symlinks out of the tree.
- Create link (Shift+Ctrl+M) next to the original, never replacing.
- A file added inside a visible subfolder updates its "N directas" row.
- Trash catalog: restore an item, delete one permanently, empty the trash.
- Grouped context menu (section titles) and the Groups "Agregar nueva".
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / 'docs/evidence/nautilus-floor-20261104'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/files')
    parser.add_argument('--label', default='source')
    args = parser.parse_args()
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    report = {'functional_gate': 'FAIL', 'label': args.label, 'binary_sha256': digest(args.binary),
              'helper_sha256': digest(args.binary.parent / 'files-helper'),
              'html_helper_sha256': digest(args.binary.parent / 'files-html-helper') if (args.binary.parent / 'files-html-helper').is_file() else None,
              'method': 'internal Actions + real FLTK window/framebuffer, private fixtures and trash; no OS input',
              'resource_gate': 'NOT_MEASURED', 'startup_gate': 'NOT_MEASURED', 'checks': {}, 'captures': []}
    if os.environ.get('WAYLAND_DISPLAY'):
        instances = json.loads(subprocess.check_output(['hyprctl', 'instances', '-j'], timeout=2))
        match = [i for i in instances if i.get('wl_socket') == os.environ.get('WAYLAND_DISPLAY')]
        if len(match) == 1:
            os.environ['HYPRLAND_INSTANCE_SIGNATURE'] = match[0]['instance']
    out = EVIDENCE / f'{args.label}-{time.time_ns()}.json'
    base_root = Path.home() / '.cache/files-test'
    base_root.mkdir(parents=True, exist_ok=True)
    process = None
    try:
        with tempfile.TemporaryDirectory(prefix='nautilus-floor-', dir=base_root) as tmp:
            base = Path(tmp)
            listing = base / 'listing'
            listing.mkdir()
            outside = base / 'outside'
            outside.mkdir()
            (outside / 'keep.txt').write_bytes(b'outside must survive\n')
            (listing / 'keep.txt').write_bytes(b'keep\n')
            (listing / 'extra.txt').write_bytes(b'extra\n')
            (listing / 'doomed.txt').write_bytes(b'doomed\n')
            (listing / 'linkme.txt').write_bytes(b'link target\n')
            (listing / 'zeta.ab').write_bytes(b'z')
            tree = listing / 'doomtree'
            (tree / 'a/b/c').mkdir(parents=True)
            (tree / 'a/b/c/deep.bin').write_bytes(os.urandom(4096))
            (tree / 'a/top.txt').write_bytes(b'x')
            (tree / 'escape').symlink_to(outside)
            watched = listing / 'watched'
            watched.mkdir()
            for n in ['one', 'two']:
                (watched / n).write_bytes(b'w')
            data = base / 'data'
            data.mkdir(mode=0o700)
            templates = base / 'Plantillas'
            templates.mkdir()
            (templates / 'Informe.md').write_text('# Informe\n')
            xdg_config = base / 'xdg-config'
            xdg_config.mkdir()
            (xdg_config / 'user-dirs.dirs').write_text(f'XDG_TEMPLATES_DIR="{templates}"\n')
            capture = base / 'frame.ppm'
            # Kept outside the temporary tree so a failure keeps its log.
            log = EVIDENCE / f'{args.label}-{time.time_ns()}-app.log'
            env = {**os.environ, 'FILES_CONFIG_DIR': str(base / 'config'), 'XDG_DATA_HOME': str(data),
                   'FILES_TEST_CAPTURE': str(capture), 'FILES_TEST_PREVIEW': '1', 'FILES_TEST_FIXED_SIZE': '1',
                   'FILES_TEST_ISOLATE_INPUT': '1', 'FILES_CHILD_WATCH_TRACE': '1', 'XDG_CONFIG_HOME': str(xdg_config), 'TMPDIR': str(base), 'MAGICK_TEMPORARY_PATH': str(base)}
            with log.open('w') as output:
                process = subprocess.Popen([str(args.binary), str(listing), '--size', '1000x650', '--scale', '1',
                                            '--no-session', '--ui-smoke', '--ui-scene', 'folder', '--exit-after', '90'],
                                           env=env, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)

            def text():
                return log.read_text()

            def wait(check, reason, timeout=15):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    value = check()
                    if value:
                        return value
                    if process.poll() is not None:
                        raise AssertionError(f'{reason}; app exit {process.returncode}')
                    time.sleep(.03)
                raise AssertionError(reason)

            def command(action):
                deadline = time.monotonic() + 8
                while True:
                    after = len(text())
                    pending = capture.with_suffix('.pending')
                    pending.write_text(action)
                    os.replace(pending, capture.with_suffix('.command'))
                    done = wait(lambda: re.search(r'FILES_PREVIEW_COMMAND_(REV action=' + re.escape(action) + r' |ERROR ' + re.escape(action) + r': (.*))', text()[after:]), 'command ' + action)
                    if done.group(0).startswith('FILES_PREVIEW_COMMAND_REV'):
                        return after
                    if 'falta' not in (done.group(2) or '') or time.monotonic() > deadline:
                        raise AssertionError(f'{action}: {done.group(2)}')
                    time.sleep(.2)  # listing still reloading

            def marker(after, pattern, reason, timeout=15):
                return wait(lambda: re.search(pattern, text()[after:]), reason, timeout)

            def state(after_cmd='nf-state'):
                after = command(after_cmd)
                return marker(after, r'FILES_NF_STATE .*', 'state').group(0)

            def trash_state():
                after = command('nf-trash-state')
                return marker(after, r'FILES_NF_TRASH .*', 'trash state').group(0)

            def shot(label):
                capture.unlink(missing_ok=True)
                capture.with_suffix('.request').touch()

                def complete():
                    if not capture.exists():
                        return False
                    parts = capture.read_bytes().split(b'\n', 3)
                    if len(parts) != 4:
                        return False
                    dims = [int(v) for v in parts[1].split()]
                    return len(dims) == 2 and len(parts[3]) == dims[0] * dims[1] * 3
                wait(complete, 'capture ' + label)
                png = EVIDENCE / f'{args.label}-{label}-{time.time_ns()}.png'
                subprocess.run(['magick', str(capture), str(png)], check=True, env=env, timeout=20)
                report['captures'].append(str(png))

            wait(lambda: 'FILES_UI_SMOKE_OK' in text(), 'smoke ready')
            wait(lambda: 'watched=2 directas' in state(), 'initial watched count', 20)
            shot('groups-add-new')

            # Grouped context menu on a file.
            # The folder watch's first reload can drop the selection between
            # two commands: select again until the file menu opens.
            for _ in range(10):
                command('nf-select:keep.txt')
                after = command('nf-context')
                menu = marker(after, r'FILES_NF_CONTEXT (.*)', 'context').group(1)
                if menu.startswith('#ABRIR'):
                    break
                report.setdefault('context_retries', 0)
                report['context_retries'] += 1
                command('nf-context-close')
                time.sleep(.3)
            sections = [s for s in menu.split(' | ') if s.startswith('#')]
            assert sections[:3] == ['#ABRIR', '#EDITAR', '#ORGANIZAR'], menu
            assert any('Eliminar permanentemente' in s for s in menu.split(' | ')), menu
            shot('context-grouped')
            command('nf-context-close')
            report['checks']['grouped_context_sections'] = sections

            # Shift+Delete: confirm first, then the tree goes, the outside stays.
            command('nf-select:doomed.txt|doomtree')
            after = command('nf-delete')
            marker(after, r'FILES_NF_CONFIRM open=true', 'confirmation not shown')
            assert (listing / 'doomed.txt').exists(), 'deleted before confirming'
            shot('delete-confirm')
            after = command('nf-confirm')
            done = marker(after, r'FILES_DELETE_DONE target=Paths.*result=(.*)', 'delete done').group(1)
            assert not (listing / 'doomed.txt').exists() and not tree.exists(), done
            assert (outside / 'keep.txt').read_bytes() == b'outside must survive\n'
            report['checks']['permanent_delete'] = done

            # Create link.
            command('nf-select:linkme.txt')
            after = command('nf-links')
            done = marker(after, r'FILES_DELETE_DONE target=Links.*result=(.*)', 'links done').group(1)
            link = listing / 'Enlace a linkme.txt'
            assert link.is_symlink() and os.readlink(link) == str(listing / 'linkme.txt'), done
            report['checks']['create_link'] = done

            # New document from a template (free names), folder with selection,
            # sort by type.
            for expected in ['Informe.md', 'Informe (2).md']:
                after = command('nf-template:0')
                marker(after, r'FILES_DELETE_DONE target=Template.*result=Creado ' + re.escape(expected), 'template ' + expected)
                assert (listing / expected).read_text() == '# Informe\n'
            command('nf-select:Informe.md|Informe (2).md')
            after = command('nf-folder-with')
            done = marker(after, r'FILES_DELETE_DONE target=FolderWith.*result=(.*)', 'folder with').group(1)
            group = listing / 'Carpeta nueva'
            assert sorted(p.name for p in group.iterdir()) == ['Informe (2).md', 'Informe.md'], done
            command('nf-sort-type')
            def order():
                after = command('nf-order')
                return marker(after, r'FILES_NF_ORDER (.*)', 'order').group(1).split('|')
            wait(lambda: (o := order()) and 'zeta.ab' in o and o.index('zeta.ab') < o.index('extra.txt'), 'type order')
            report['checks']['templates_group_sort'] = {'group': done, 'type_order': order()}

            # Ctrl+S select by pattern; Ctrl+M link to what was copied.
            after = command('nf-select-pattern:*.txt')
            sel = int(marker(after, r'FILES_SELECT_PATTERN selected=(\d+)', 'pattern').group(1))
            assert sel == 4, sel  # keep, extra, linkme, Enlace a linkme
            command('nf-select:zeta.ab')
            command('nf-copy')
            for _ in range(40):
                after = command('nf-link-copied')
                time.sleep(.15)
                if re.search(r'FILES_DELETE_DONE target=Links .*zeta\.ab', text()[after:]):
                    break
            else:
                raise AssertionError('Ctrl+M link not created')
            wait(lambda: (listing / 'Enlace a zeta.ab').is_symlink(), 'ctrl+m link')
            assert os.readlink(listing / 'Enlace a zeta.ab') == str(listing / 'zeta.ab')
            report['checks']['pattern_and_ctrl_m'] = {'pattern_selected': sel, 'link': 'Enlace a zeta.ab'}

            # Files added inside a visible subfolder update its row.
            for n in ['three', 'four', 'five']:
                (watched / n).write_bytes(b'w')
            marker(0, r'FILES_CHILD_REFRESH name="watched"', 'child refresh', 20)
            wait(lambda: 'watched=5 directas' in state(), 'watched count not updated', 20)
            report['checks']['child_refresh'] = 'watched 2 -> 5 directas'

            # Rubber band from the empty space below the rows, up over the
            # last two rows; then additive over one more; a plain click clears.
            after = command('nf-hits')
            hits = marker(after, r'FILES_NF_HITS listing=(\d+),(\d+),(\d+),(\d+) (.*)', 'hits')
            lx, ly, lw, lh = map(int, hits.groups()[:4])
            rows = []
            for item in hits.group(5).split(';'):
                name, geo = item.rsplit('@', 1)
                x, y, w, h = map(int, geo.split(','))
                rows.append((y, x, w, h, name))
            files_rows = sorted(r for r in rows if r[3] < 60)  # list rows, not folder tiles
            assert len(files_rows) >= 3, rows
            last, prev, third = files_rows[-1], files_rows[-2], files_rows[-3]
            empty_y = last[0] + last[3] + 12
            assert empty_y < ly + lh, ('no empty space below rows', empty_y, ly + lh)
            x0 = last[1] + 30
            after = command(f'nf-marquee:{x0},{empty_y},{x0 + 120},{prev[0] + 5},0')
            got = marker(after, r'FILES_NF_SELECTION (.*)', 'band').group(1).split('|')
            assert sorted(got) == sorted([last[4], prev[4]]), (got, last, prev)
            after = command(f'nf-marquee:{x0},{empty_y},{x0 + 120},{third[0] + 5},1')
            got3 = marker(after, r'FILES_NF_SELECTION (.*)', 'band additive').group(1).split('|')
            assert sorted(got3) == sorted([last[4], prev[4], third[4]]), got3
            after = command(f'nf-marquee:{x0},{empty_y},{x0},{empty_y},0')
            assert marker(after, r'FILES_NF_SELECTION (.*)', 'click clears').group(1) == ''
            report['checks']['rubber_band'] = {'two': got, 'additive': got3, 'click_empty_clears': True}

            # Trash: restore, purge one, empty.
            command('nf-select:keep.txt')
            command('nf-trash')
            wait(lambda: not (listing / 'keep.txt').exists(), 'trash move')
            command('nf-open-trash')
            wait(lambda: 'ready=true count=1' in trash_state(), 'trash catalog')
            shot('trash-catalog')
            after = command('nf-trash-restore')
            marker(after, r'FILES_DELETE_DONE target=TrashRestore', 'restore done')
            assert (listing / 'keep.txt').read_bytes() == b'keep\n'
            wait(lambda: 'ready=true count=0' in trash_state(), 'catalog after restore')
            command('nf-trash-close')
            command('nf-select:keep.txt|extra.txt')
            command('nf-trash')
            wait(lambda: not (listing / 'keep.txt').exists() and not (listing / 'extra.txt').exists(), 'trash two')
            command('nf-open-trash')
            wait(lambda: 'ready=true count=2' in trash_state(), 'catalog two')
            after = command('nf-trash-purge')
            marker(after, r'', 'purge')
            command('nf-confirm')
            wait(lambda: 'ready=true count=1' in trash_state(), 'purge one')
            command('nf-trash-empty')
            shot('trash-empty-confirm')
            command('nf-confirm')
            wait(lambda: 'ready=true count=0' in trash_state(), 'empty trash')
            trash = data / 'Trash'
            assert not any((trash / 'files').iterdir()) and not any((trash / 'info').iterdir())
            shot('trash-emptied')
            report['checks']['trash'] = 'restore, purge one, empty; files/ and info/ empty'
            report['functional_gate'] = 'PASS'
    except Exception as e:
        report['error'] = repr(e)
    finally:
        if process and process.poll() is None:
            process.kill()
            process.wait()
    if report['functional_gate'] == 'PASS':
        for stale in EVIDENCE.glob(f'{args.label}-*-app.log'):
            stale.unlink(missing_ok=True)
    out.write_text(json.dumps(report, indent=2) + '\n')
    print(report['functional_gate'], out)
    return 0 if report['functional_gate'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
