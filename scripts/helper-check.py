#!/usr/bin/env python3
"""Protocol/lifecycle checks for the built helper; not a resource benchmark."""
import json
import pathlib
import subprocess
import sys
import tempfile

binary = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/files-helper').resolve()
with tempfile.TemporaryDirectory(prefix='files-helper-check-') as tmp:
    root = pathlib.Path(tmp)
    (root / 'a').write_bytes(b'abc')
    (root / 'sub').mkdir()
    (root / 'sub' / 'b').write_bytes(b'12345')
    (root / 'sub' / 'cycle').symlink_to(root)
    request = json.dumps({'command': 'du', 'id': 71, 'path': str(root)}).encode()
    def run(payload):
        return subprocess.run([str(binary), 'du'], input=payload, capture_output=True, timeout=5)
    for payload in (request + b'\n', b' ' * 40000 + request + b'\n', request):
        result = run(payload)
        assert result.returncode == 0, result.stderr
        assert json.loads(result.stdout) == {'status': 'usage', 'id': 71, 'bytes': 8, 'entries': 4}
    result = run(json.dumps({'command': 'du', 'id': 72, 'path': str(root / 'missing')}).encode())
    assert result.returncode == 0
    assert json.loads(result.stdout)['status'] == 'error'
    result = run(b' ' * 65537 + b'\n')
    assert result.returncode == 2 and not result.stdout
    # The process must exit after one complete request even with stdin kept open.
    process = subprocess.Popen([str(binary), 'du'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        process.stdin.write(request + b'\n')
        process.stdin.flush()
        assert process.wait(timeout=5) == 0
        assert json.loads(process.stdout.read())['bytes'] == 8
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        process.stdin.close()
        process.stdout.close()
        process.stderr.close()
print(json.dumps({'protocol': 'PASS', 'cases': 6, 'resource_benchmark': False}))
