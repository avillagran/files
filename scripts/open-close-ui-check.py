#!/usr/bin/env python3
"""Functional open/callback evidence; no OS input, resource or startup sampling.

Real missing `open` is checked under the inherited host PATH. Fixture commands
are then supplied ONLY to each app child's PATH; fake exit0 proves launcher
handling/argv, never that an actual system viewer displayed the file. CUA and
Hyprland are queried read-only while the first app lives. A missing driver/target
does not authorize alternative native control. Callback testing invokes the
registered FLTK callback internally; it does not claim a compositor close event.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "target/release/files"
BASE = Path.home() / ".cache/files-test"
EVIDENCE = ROOT / "docs/evidence/0.0.4"
READY = re.compile(r'^FILES_OPEN_READY seq=(\d+) path=("(?:[^"\\]|\\.)*") error=("(?:[^"\\]|\\.)*")$')

# Only Driver/list_windows/stop are used. No choose/execute/observe/control call.
# stdout contains count and whitelisted metadata belonging to the requested PID.
CUA_PROBE = r'''
import importlib.util, json, sys
target=int(sys.argv[1]);driver=None
report={'method':'jev Driver/list_windows read-only','status':'ERROR','count':None,'matching_windows':[]}
try:
    spec=importlib.util.spec_from_file_location('files_cua_readonly','/home/avillagran/.codex/skills/jev-computer-use/scripts/jev_gui_agent.py')
    module=importlib.util.module_from_spec(spec);sys.modules[spec.name]=module;spec.loader.exec_module(module)
    driver=module.Driver()
    tool=driver.tool;valid={'response':False}
    def checked(name,args,timeout=90.0):
        response=tool(name,args,timeout)
        result=response.get('result',{})
        structured=result.get('structuredContent') or {}
        valid['response']=not response.get('error') and not result.get('isError') and isinstance(structured.get('windows'),list)
        return response
    driver.tool=checked
    windows=module.list_windows(driver)
    if valid['response']:
        report.update(status='OBSERVED',count=len(windows),matching_windows=[{key:w[key] for key in ('pid','window_id','id','is_on_screen','on_current_space') if key in w} for w in windows if w.get('pid')==target])
    else:report['error']='Driver list_windows response unavailable/invalid'
except Exception as error:
    report['error']=type(error).__name__
finally:
    if driver is not None:
        driver.stop()
        try:driver.proc.wait(timeout=2)
        except Exception:
            driver.proc.kill()
            try:driver.proc.wait(timeout=2)
            except Exception:pass
print(json.dumps(report))
'''


def stop_owned_group(process):
    """Emergency cleanup of this test's own app/fake/driver group only, never PASS."""
    if process.poll() is None:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=3)


def native_presence(root_pid):
    result = {"method": "read-only inventory; no native action", "native_input_gate": "NOT_ATTEMPTED"}
    probe = subprocess.Popen(
        [sys.executable, "-c", CUA_PROBE, str(root_pid)],
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    try:
        stdout, _ = probe.communicate(timeout=15)
        if probe.returncode != 0 or len(stdout) > 16384:
            result["cua"] = {"status": "ERROR", "reason": "probe failed/bounded output exceeded"}
        else:
            # Invalid/unfiltered stdout is never copied into the evidence.
            result["cua"] = json.loads(stdout)
    except (subprocess.TimeoutExpired, ValueError):
        result["cua"] = {"status": "UNAVAILABLE", "reason": "probe timeout/invalid response"}
    finally:
        stop_owned_group(probe)
    if not result["cua"].get("matching_windows"):
        result["native_input_gate"] = "PENDING_DRIVER_TARGET"
    try:
        clients = json.loads(subprocess.run(
            ["hyprctl", "clients", "-j"], check=True, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, timeout=3,
        ).stdout)
        result["compositor"] = {
            "method": "hyprctl clients -j read-only; no dispatch",
            "matching_clients": [
                {"pid": client["pid"], "address": client.get("address")}
                for client in clients if client.get("pid") == root_pid
            ],
        }
    except (OSError, ValueError, subprocess.SubprocessError):
        result["compositor"] = {"status": "UNAVAILABLE"}
    return result


def main():
    # 2026-11-04: Linux no longer runs a system `open` (Omarchy's is a shell
    # function); it resolves the default app and launches it through UWSM.
    # Running this scene would start the user's real default application.
    # Superseded on Linux by scripts/open-with-ui-check.py (recorder launcher).
    if sys.platform.startswith("linux"):
        print("SKIPPED: superseded on Linux by scripts/open-with-ui-check.py")
        return 0
    BASE.mkdir(parents=True, exist_ok=True)
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    report = {
        "method": "visible Open hit handlers + FLTK framebuffer; internal win.do_callback through registered close callback",
        "native_input_gate": "NOT_ATTEMPTED",
        "native_close_gate": "PENDING_OS_INPUT",
        "resource_gate": "NOT_MEASURED",
        "startup_gate": "NOT_MEASURED",
        "cleanup_scope": "root exit0 plus close drain markers; no descendant sampler in this functional phase",
        "runs": [], "captures": [], "functional_gate": "IN_PROGRESS",
        "known_pending": ["actual installed system open/viewer success", "native compositor close shortcut/event", "resource/startup phase frozen"],
    }
    failure = None
    try:
        host_open = shutil.which("open")
        report["host_open"] = {"available": host_open is not None, "path": host_open}
        if host_open is not None:
            raise RuntimeError("Host capability changed: real missing-open scenario cannot be validated; no viewer launched")
        if not BINARY.is_file():
            raise RuntimeError("Release files binary is missing")
        with tempfile.TemporaryDirectory(prefix="open-close-ui-", dir=BASE) as temporary:
            test = Path(temporary)
            for case in ("host-unavailable", "fake-open", "fake-exit7"):
                run = {"case": case, "functional_gate": "IN_PROGRESS"}
                report["runs"].append(run)
                case_root = test / case
                folder = case_root / "files"
                folder.mkdir(parents=True)
                source = folder / "example-日本語 $().txt"
                original = "contenido Unicode 漢字\t\r\n$() preservado\r\n".encode()
                source.write_bytes(original)
                outside = case_root / "outside.txt"
                outside.write_bytes(b"link target preserved")
                link = folder / "enlace"
                link.symlink_to(outside)
                child = folder / "childdir"
                child.mkdir()
                (child / "inside.txt").write_bytes(b"directory content preserved")
                raw = case_root / "capture.ppm"
                log = case_root / "app.log"
                records = case_root / "launcher.jsonl"
                env = {
                    **os.environ,
                    "FILES_CONFIG_DIR": str(case_root / "config"),
                    "FILES_TEST_CAPTURE": str(raw),
                    "FILES_TEST_OPEN": "1", "FILES_CLOSE_TRACE": "1",
                }
                if case != "host-unavailable":
                    fake_bin = case_root / "bin"
                    fake_bin.mkdir()
                    fake = fake_bin / "open"
                    fake.write_text(
                        "#!/usr/bin/python3\nimport json,os,sys\n"
                        "with open(os.environ['FILES_OPEN_RECORD'],'a',encoding='ascii') as output:\n"
                        "    output.write(json.dumps({'argv_hex':[os.fsencode(arg).hex() for arg in sys.argv[1:]]})+'\\n')\n"
                        "raise SystemExit(int(os.environ['FILES_FAKE_OPEN_EXIT']))\n"
                    )
                    fake.chmod(0o700)
                    env.update(
                        PATH=str(fake_bin) + os.pathsep + os.environ.get("PATH", ""),
                        FILES_OPEN_RECORD=str(records),
                        FILES_FAKE_OPEN_EXIT="7" if case == "fake-exit7" else "0",
                    )
                    run["command_scope"] = "fixture fake executable; PATH changed only for app child; no actual viewer"
                else:
                    run["command_scope"] = "real absent host command under inherited PATH; no fallback"
                process = None
                normal = False
                try:
                    with log.open("w") as output:
                        process = subprocess.Popen(
                            [str(BINARY), str(folder), "--size", "1000x650", "--scale", "1", "--ui-smoke", "--ui-scene", "open", "--exit-after", "60"],
                            env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=output, start_new_session=True,
                        )

                    def wait(check, reason, allow_exit=False, timeout=12):
                        deadline = time.monotonic() + timeout
                        while time.monotonic() < deadline:
                            value = check()
                            if value:
                                return value
                            if process.poll() is not None and not allow_exit:
                                raise AssertionError(reason + "; root exit=" + str(process.returncode))
                            time.sleep(.02)
                        raise AssertionError(reason)

                    def ready(sequence, after=0):
                        def fresh():
                            for line in log.read_text()[after:].splitlines():
                                match = READY.fullmatch(line)
                                if match and int(match[1]) == sequence:
                                    return {"seq": sequence, "path": json.loads(match[2]), "error": json.loads(match[3]), "line": line}
                            return None
                        return wait(fresh, f"fresh painted Open READY seq={sequence} absent")

                    def command(action):
                        after = len(log.read_text())
                        pending = raw.with_suffix(".pending")
                        pending.write_text(action)
                        os.replace(pending, raw.with_suffix(".command"))
                        match = wait(
                            lambda: re.search(r"^FILES_OPEN_COMMAND seq=(\d+) action=" + re.escape(action) + r"$", log.read_text()[after:], re.MULTILINE),
                            "Open command not accepted: " + action, allow_exit=action == "callback",
                        )
                        return int(match[1]), after

                    def capture(name):
                        raw.unlink(missing_ok=True)
                        raw.with_suffix(".request").touch()

                        def full_frame():
                            if not raw.exists() or raw.stat().st_size > 64 * 1024 * 1024:
                                return None
                            parts = raw.read_bytes().split(b"\n", 3)
                            if len(parts) != 4 or parts[0] != b"P6" or parts[2] != b"255":
                                return None
                            dimensions = parts[1].split()
                            if len(dimensions) != 2:
                                return None
                            width, height = map(int, dimensions)
                            return [width, height] if 0 < width <= 8192 and 0 < height <= 8192 and width * height <= 16 * 1024 * 1024 and len(parts[3]) == width * height * 3 else None

                        dimensions = wait(full_frame, "complete bounded framebuffer absent")
                        target = EVIDENCE / (name + "-" + str(time.time_ns()) + ".png")
                        subprocess.run(["magick", str(raw), str(target)], check=True, timeout=15)
                        entry = {"path": str(target), "framebuffer_dimensions": dimensions, "method": "FLTK framebuffer; compositor placement/geometry not controlled"}
                        report["captures"].append(entry)
                        return entry

                    initial = ready(0)
                    assert initial["path"] == str(folder)
                    assert "FILES_OPEN_OUTCOME=" not in log.read_text(), "scene must not open automatically"
                    if case == "host-unavailable":
                        run["native_presence"] = native_presence(process.pid)
                    expected = "unavailable" if case == "host-unavailable" else ("failed" if case == "fake-exit7" else "opened")
                    run["actions"] = []
                    for action, argument in (("file", source), ("link", link)):
                        menu_sequence, menu_after = command("actions")
                        ready(menu_sequence, menu_after)
                        sequence, after = command(action)
                        state = ready(sequence, after)
                        rows = log.read_text()[after:].splitlines()
                        assert rows.count("FILES_OPEN_OUTCOME=" + expected) == 1, rows
                        assert state["path"] == str(folder), "file/symlink must not navigate"
                        if expected == "unavailable":
                            assert "open" in state["error"] and "PENDIENTE" in state["error"]
                        elif expected == "failed":
                            assert "7" in state["error"] and "open" in state["error"]
                        else:
                            assert state["error"] == ""
                        if case != "host-unavailable":
                            recorded = [json.loads(line) for line in records.read_text().splitlines()]
                            assert len(recorded) == (1 if action == "file" else 2)
                            assert recorded[-1]["argv_hex"] == [os.fsencode(argument).hex()], "argv must preserve the literal absolute file/link path"
                        run["actions"].append({"action": action, "menu_seq": menu_sequence, "seq": sequence, "outcome": expected, "state": state, "literal_argv_verified": case != "host-unavailable"})
                        if action == "file" or case == "host-unavailable":
                            capture("open-" + case + "-" + action)
                    menu_sequence, menu_after = command("actions")
                    ready(menu_sequence, menu_after)
                    sequence, after = command("folder")
                    state = ready(sequence, after)
                    assert state["path"] == str(child)
                    assert "FILES_OPEN_OUTCOME=" not in log.read_text()[after:], "folder navigation must not invoke open"
                    if case != "host-unavailable":
                        assert len(records.read_text().splitlines()) == 2
                    run["folder_navigation_without_launcher"] = True
                    if case == "fake-open":
                        capture("open-folder-navigation")
                    callback_sequence, after = command("callback")
                    wait(lambda: process.poll() is not None, "FLTK callback close did not exit", allow_exit=True)
                    tail = log.read_text()[after:]
                    required = ("FILES_CLOSE_CALLBACK source=fltk_callback", "FILES_CLOSE_REQUEST callback=true", "FILES_CLOSE_DRAINED")
                    positions = [tail.find(marker) for marker in required]
                    assert all(position >= 0 for position in positions) and positions == sorted(positions), "fresh callback/request/drain markers missing or out of order"
                    assert process.returncode == 0, "callback close did not exit normally"
                    assert source.read_bytes() == original and outside.read_bytes() == b"link target preserved"
                    assert link.is_symlink() and os.readlink(link) == str(outside)
                    assert (child / "inside.txt").read_bytes() == b"directory content preserved"
                    normal = True
                    run.update(
                        functional_gate="PASS", root_exit=process.returncode,
                        registered_callback_exit=True, callback_sequence=callback_sequence,
                        callback_request_drained_order=True, fixture_bytes_names_links_preserved=True,
                        viewer_opened="NOT_CLAIMED", cleanup="normal root exit plus app-owned drain markers; launcher already acknowledged",
                    )
                except BaseException as error:
                    run.update(functional_gate="FAIL", error=repr(error))
                    raise
                finally:
                    # Preserve failure log BEFORE emergency cleanup, which cannot
                    # transform failed lifecycle validation into a passing close.
                    run["log_before_cleanup"] = log.read_text() if log.exists() else ""
                    if process is not None and not normal:
                        stop_owned_group(process)
                        run["emergency_cleanup"] = True
            report["functional_gate"] = "PASS"
    except BaseException as error:
        failure = error
        report.update(functional_gate="FAIL", error=repr(error))
    finally:
        if BINARY.is_file():
            report["binary_sha256"] = hashlib.sha256(BINARY.read_bytes()).hexdigest()
        target = EVIDENCE / ("open-close-ui-" + str(time.time_ns()) + ".json")
        with target.open("x") as output:
            json.dump(report, output, ensure_ascii=False, indent=2)
            output.write("\n")
        print(target)
    if failure is not None:
        raise failure


if __name__ == "__main__":
    main()
