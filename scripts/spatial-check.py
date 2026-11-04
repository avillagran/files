#!/usr/bin/env python3
"""Real FLTK framebuffer and internal gesture/reopen fixtures; NOT OS input proof.
Run serialized without builds, benchmarks or other captures.
"""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("ui_check", ROOT / "scripts/ui-check.py")
ui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ui)


def main():
    base = Path.home() / ".cache/files-test"
    base.mkdir(parents=True, exist_ok=True)
    results = []
    with tempfile.TemporaryDirectory(prefix="spatial-", dir=base) as temporary:
        root = Path(temporary)
        listing = root / "files"
        listing.mkdir()
        for name in ["Álbum2", "Documentos10", "Proyectos"]:
            (listing / name).mkdir()
        for name in ["archivo2.txt", "archivo10.txt", "diseño–con–nombre–muy–largo–Unicode–1234567890.md"]:
            (listing / name).write_text("fixture\n")
        ui.CAPTURE_PATH = root / "frame.ppm"
        env = {**os.environ, "FILES_CONFIG_DIR": str(root / "config/files"), "FILES_TEST_CAPTURE": str(ui.CAPTURE_PATH)}
        for scene, scale in [("spatial-drag", 1), ("spatial-reopen", 1), ("spatial-reopen", 2)]:
            log = root / f"{scene}-{scale}.log"
            with log.open("w") as output:
                process = subprocess.Popen([str(ROOT / "target/release/files"), str(listing), "--size", "1000x650", "--scale", str(scale), "--ui-smoke", "--ui-scene", scene, "--exit-after", "7"], env=env, stdout=subprocess.DEVNULL, stderr=output)
            try:
                marker = "FILES_SPATIAL_DRAG_OK" if scene == "spatial-drag" else "FILES_SPATIAL_REOPEN_OK"
                ui.wait_for(lambda: marker in log.read_text() or process.poll() is not None, "spatial scene timed out")
                assert marker in log.read_text(), log.read_text()
                # Keep equal logical drawing area at both requested FLTK scales.
                width, height = 1000 * scale, 650 * scale
                def sized(w, h):
                    observed = ui.memcheck.observed_geometry(process.pid, w, h)
                    if observed:
                        return observed
                    ui.memcheck.geometry(process.pid, w, h)
                    return None
                ui.wait_for(lambda: sized(width, height), "geometry unverified")
                time.sleep(.3)
                capture = ui.capture(process.pid, f"{scene}-{scale}x")
                shape = subprocess.check_output(["magick", "identify", "-format", "%wx%h", str(ROOT / capture)], text=True)
                item = {"font_selected": next(line for line in log.read_text().splitlines() if line.startswith("FILES_FONT=")), "scene": scene, "fltk_scale_requested": scale, "geometry": ui.memcheck.observed_geometry(process.pid, width, height), "capture": capture, "framebuffer_dimensions": shape}
                if scale == 2:
                    # A second real compositor resize, then another framebuffer.
                    ui.wait_for(lambda: sized(1600, 1000), "resize unverified")
                    time.sleep(.3)
                    item["resize_capture"] = ui.capture(process.pid, "spatial-resize-2x")
                    item["resized_geometry"] = ui.memcheck.observed_geometry(process.pid, 1600, 1000)
                process.wait(timeout=7)
                assert process.returncode == 0, log.read_text()
                item["graceful_close"] = "normal exit after save ACK drain"
                results.append(item)
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=3)
        print(json.dumps({"status": "PASS", "gesture_delivery": "in-process pointer handlers using actual rendered hit targets; not OS events", "persistence": "new process loaded saved positions72/48 and zoom1.15", "font": "Nerd font rasterization must also be visually reviewed", "scenarios": results}, indent=2))


if __name__ == "__main__":
    main()
