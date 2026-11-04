#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Pinned, isolated litehtml display-list experiment; not a files feature gate.

No compilation or execution occurs unless --build / --run is explicit. Sources
live in ~/.cache/files-test, never /tmp; no installation or Cargo is performed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import resource
import selectors
import shutil
import signal
import subprocess
import tarfile
import time
import urllib.request

COMMIT = "6fd7aaa63833c913cf1a0336e46fb2e027e4c656"
ARCHIVE_SHA = "82cfdc8347ebb61fbf308e29ebcb6ea5f82e894dd6a013c928deacc0446a471e"
URL = f"https://codeload.github.com/litehtml/litehtml/tar.gz/{COMMIT}"
ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "scripts/fixtures/html-engine"
CACHE = Path.home() / ".cache/files-test/html-engine"
MAX_ARCHIVE = 4 * 1024 * 1024
MAX_EXTRACTED = 16 * 1024 * 1024
MAX_FILES = 1000


def adapter_fingerprint() -> str:
    digest = hashlib.sha256()
    for name in ("CMakeLists.txt", "probe.cpp", "strtod_bridge.cpp", "exclude-strtod.patch"):
        digest.update(name.encode() + b"\0")
        digest.update(bytes.fromhex(sha(FIXTURES / name)))
    return digest.hexdigest()


def sha(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(65536), b""):
            digest.update(block)
    return digest.hexdigest()


def prepare() -> Path:
    CACHE.mkdir(parents=True, exist_ok=True)
    archive = CACHE / f"{COMMIT}.tar.gz"
    if not archive.exists():
        partial = CACHE / f"{COMMIT}.download-{os.getpid()}"
        try:
            # Immutable URL plus the independently fixed digest below.
            with urllib.request.urlopen(URL, timeout=20) as response, partial.open("xb") as output:
                used = 0
                while block := response.read(65536):
                    used += len(block)
                    if used > MAX_ARCHIVE:
                        raise ValueError("source archive exceeds byte limit")
                    output.write(block)
            if sha(partial) != ARCHIVE_SHA:
                raise ValueError("source archive SHA256 mismatch")
            # Preserve an existing archive even if a concurrent preparation won.
            try:
                os.link(partial, archive)
            except FileExistsError:
                pass
        finally:
            partial.unlink(missing_ok=True)
    if archive.stat().st_size > MAX_ARCHIVE or sha(archive) != ARCHIVE_SHA:
        raise ValueError("cached source archive SHA256 mismatch")

    source = CACHE / f"litehtml-{COMMIT}"
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        if len(members) > MAX_FILES or sum(m.size for m in members) > MAX_EXTRACTED:
            raise ValueError("source extraction exceeds limits")
        prefix = f"litehtml-{COMMIT}"
        for member in members:
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts or not path.parts or path.parts[0] != prefix:
                raise ValueError("unsafe source archive path")
            if not (member.isfile() or member.isdir()):
                raise ValueError("non-regular source archive member")
        if not source.exists():
            bundle.extractall(CACHE, filter="data")
        # Refuse modified cache files instead of silently building them.
        for member in members:
            if member.isfile():
                path = CACHE / member.name
                if path.is_symlink() or not path.is_file() or path.stat().st_size != member.size:
                    raise ValueError(f"cached source differs: {member.name}")
                expected = hashlib.sha256(bundle.extractfile(member).read()).hexdigest()
                if sha(path) != expected:
                    raise ValueError(f"cached source SHA mismatch: {member.name}")
    return source


def patched_copy(source: Path) -> Path:
    """Never modify verified originals; patch a disposable bounded cache copy."""
    if not shutil.which("patch"):
        raise RuntimeError("build prerequisite unavailable: patch")
    identity = adapter_fingerprint()
    # Reconstruct instead of trusting a mutable copy; preserve failed builds too.
    destination = CACHE / f"patched-{COMMIT}-{identity[:16]}-{time.time_ns()}"
    shutil.copytree(source, destination, symlinks=False)
    subprocess.run(["patch", "--batch", "--forward", "--fuzz=0", "-p1", "-i",
                    str(FIXTURES / "exclude-strtod.patch")],
                   cwd=destination, check=True, timeout=5, capture_output=True)
    shutil.copyfile(FIXTURES / "strtod_bridge.cpp", destination / "src/files_strtod_bridge.cpp")
    (destination / "files-patch-source.json").write_text(json.dumps({
        "archive_sha256": ARCHIVE_SHA, "patch_sha256": sha(FIXTURES / "exclude-strtod.patch"),
        "bridge_sha256": sha(FIXTURES / "strtod_bridge.cpp"),
        "original_strtod_retained_not_compiled": True,
    }, indent=2) + "\n")
    return destination


def build(source: Path) -> Path:
    for tool in ("cmake", "c++"):
        if not shutil.which(tool):
            raise RuntimeError(f"build prerequisite unavailable: {tool}")
    source = patched_copy(source)
    directory = CACHE / f"build-{COMMIT}"
    directory.mkdir(exist_ok=True)
    log = directory / "build.log"
    # One build process at a time and no tests/network ExternalProject target.
    commands = [
        ["cmake", "-S", str(FIXTURES), "-B", str(directory),
         "-DLITEHTML_SOURCE=" + str(source),
         "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=OFF",
         "-DLITEHTML_BUILD_TESTING=OFF", "-DLITEHTML_ENABLE_LINT=OFF"],
        ["cmake", "--build", str(directory), "--target", "html-engine-probe", "--parallel", "1"],
    ]
    with log.open("w") as output:
        for command in commands:
            output.write(json.dumps(command) + "\n"); output.flush()
            subprocess.run(command, stdout=output, stderr=subprocess.STDOUT, check=True, timeout=300)
    binary = directory / "html-engine-probe"
    (directory / "build-source.json").write_text(json.dumps({
        "archive_sha256": ARCHIVE_SHA, "adapter_sha256": adapter_fingerprint(),
        "patch_sha256": sha(FIXTURES / "exclude-strtod.patch"),
        "binary_sha256": sha(binary), "commands": commands,
    }, indent=2) + "\n")
    return binary


def font_family() -> dict:
    # Resolve a single configured family. No catalog enumeration/rasterization.
    try:
        family = subprocess.run(["fc-match", "-f", "%{family}\n", "monospace"],
                                capture_output=True, check=True, timeout=2).stdout
        if len(family) > 4096:
            raise ValueError("fc-match output too large")
        name = family.decode("utf-8").splitlines()[0].split(",")[0].strip()
        if not name or len(name.encode()) > 128 or any(ord(c) < 32 for c in name):
            raise ValueError("invalid family")
        return {"family": name, "nerd_family_name": "nerd" in name.lower(),
                "coverage_verified": False, "method": "single fc-match monospace"}
    except (OSError, ValueError, subprocess.SubprocessError, IndexError) as error:
        return {"family": "monospace", "nerd_family_name": False,
                "coverage_verified": False, "method": "readable fallback", "reason": str(error)}


def limits() -> None:
    # Crash containment only: virtual address limits ARE NOT a PSS budget.
    resource.setrlimit(resource.RLIMIT_AS, (128 * 1024 * 1024, 128 * 1024 * 1024))
    resource.setrlimit(resource.RLIMIT_CPU, (3, 3))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    resource.setrlimit(resource.RLIMIT_NOFILE, (32, 32))


def invoke(binary: Path, fixture: Path, width: int, family: str) -> dict:
    command = [str(binary), str(fixture), str(width), "720", family]
    started = time.monotonic()
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=True, preexec_fn=limits, close_fds=True)
    selector = selectors.DefaultSelector()
    outputs = {"stdout": bytearray(), "stderr": bytearray()}
    assert process.stdout and process.stderr
    for label, stream in (("stdout", process.stdout), ("stderr", process.stderr)):
        os.set_blocking(stream.fileno(), False)
        selector.register(stream, selectors.EVENT_READ, label)
    try:
        deadline = started + 5
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("HTML probe exceeded monotonic wall deadline")
            for key, _ in selector.select(min(remaining, 0.1)):
                block = os.read(key.fd, 65536)
                if not block:
                    selector.unregister(key.fileobj); key.fileobj.close(); continue
                target = outputs[key.data]
                cap = 2 * 1024 * 1024 + 16384 if key.data == "stdout" else 65536
                if len(block) > cap - len(target):
                    raise ValueError(f"probe {key.data} exceeds wire cap")
                target.extend(block)
        process.wait(timeout=max(0.001, deadline - time.monotonic()))
        if process.returncode:
            raise RuntimeError(f"probe exit {process.returncode}: " + outputs["stderr"].decode("utf-8", "replace"))
        payload = json.loads(outputs["stdout"])
        if payload.get("schema") != 1 or payload.get("status") != "PROBE_ONLY" or payload.get("numeric_contract") is not True:
            raise ValueError("unexpected display-list schema")
        commands = payload.get("commands")
        if not isinstance(commands, list) or len(commands) != payload.get("command_count") or len(commands) > 8192:
            raise ValueError("invalid display-list count")
        payload["fixture"] = fixture.name
        payload["fixture_sha256"] = sha(fixture)
        payload["wall_seconds_including_launch"] = time.monotonic() - started
        payload["stderr"] = outputs["stderr"].decode("utf-8", "replace")
        return payload
    finally:
        selector.close()
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=2)
        for stream in (process.stdout, process.stderr):
            if stream and not stream.closed:
                stream.close()


def inspect(rows: list[dict]) -> None:
    for row in rows:
        texts = [c["text"] for c in row["commands"] if c.get("op") == "text"]
        joined = " ".join(texts)
        if not texts or row["nodes"] < 2:
            raise ValueError("no concrete HTML layout/text commands")
        if "SCRIPT_EXECUTED_SENTINEL" in joined:
            raise ValueError("script body unexpectedly visible")
        depth = 0
        for command in row["commands"]:
            if command["op"] == "clip": depth += 1
            elif command["op"] == "unclip": depth -= 1
            if not 0 <= depth <= 32:
                raise ValueError("unbalanced command clipping")
        if depth:
            raise ValueError("unclosed command clipping")
        if row["fixture"] == "blocked.html" and (row["blocked_images"] < 2 or not row["blocked_css"]):
            raise ValueError("blocked resource callbacks not exercised")
    wide, narrow = rows[0], rows[1]
    if wide["document"] == narrow["document"] and wide["commands"] == narrow["commands"]:
        raise ValueError("no observable layout change between viewport widths")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare", action="store_true", help="fetch/check immutable sources only")
    parser.add_argument("--build", action="store_true", help="explicitly compile static engine/probe, one job")
    parser.add_argument("--run", action="store_true", help="explicitly run three layout fixtures; not a PSS gate")
    args = parser.parse_args()
    if not (args.prepare or args.build or args.run):
        parser.print_help(); return 0
    report = {"schema": 1, "feature_gate": "NOT_IMPLEMENTED", "resource_gate": "NOT_MEASURED",
              "probe_gate": "NOT_RUN", "engine_commit": COMMIT, "archive_sha256": ARCHIVE_SHA,
              "source_url": URL, "errors": [], "rows": []}
    result = 0
    try:
        source = prepare()
        report["sources_verified"] = True
        binary = CACHE / f"build-{COMMIT}" / "html-engine-probe"
        if args.build:
            binary = build(source); report["build"] = "PASS"
        if args.run:
            stamp = json.loads((binary.parent / "build-source.json").read_text())
            if stamp["archive_sha256"] != ARCHIVE_SHA or stamp["adapter_sha256"] != adapter_fingerprint() or stamp["binary_sha256"] != sha(binary):
                raise ValueError("probe source/binary changed; explicit --build required")
            report["font"] = font_family()
            report["probe_gate"] = "FAIL"
            for fixture, width in (("layout.html", 640), ("layout.html", 320), ("blocked.html", 640)):
                report["rows"].append(invoke(binary, FIXTURES / fixture, width, report["font"]["family"]))
            inspect(report["rows"])
            report["probe_gate"] = "PASS_APPROXIMATE_DISPLAY_LIST_ONLY"
    except Exception as error:
        report["errors"].append(f"{type(error).__name__}: {error}"); result = 1
    # Every run preserves failures and prior output, including partial rows.
    destination = ROOT / "docs/evidence/html-engine"
    destination.mkdir(parents=True, exist_ok=True)
    output = destination / f"probe-{time.time_ns()}.json"
    with output.open("x") as handle:
        json.dump(report, handle, indent=2, ensure_ascii=False); handle.write("\n")
    print(output)
    print(json.dumps({key: report[key] for key in ("probe_gate", "feature_gate", "resource_gate", "errors")}))
    return result


if __name__ == "__main__":
    raise SystemExit(main())
