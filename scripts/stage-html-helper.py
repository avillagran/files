#!/usr/bin/env python3
"""Stage the already built, verified experimental adapter beside local files.
No build/run, no global install. Call make html-helper for the pinned build.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile

ROOT=Path(__file__).resolve().parent.parent
COMMIT='6fd7aaa63833c913cf1a0336e46fb2e027e4c656'
source=Path.home()/'.cache/files-test/html-engine'/('build-'+COMMIT)
stamp=json.loads((source/'build-source.json').read_text())
spec=importlib.util.spec_from_file_location('files_html_recipe',ROOT/'scripts/html-engine-probe.py')
recipe=importlib.util.module_from_spec(spec)
spec.loader.exec_module(recipe)
if recipe.adapter_fingerprint()!=stamp['adapter_sha256']:
    raise SystemExit('Native HTML adapter source changed; rebuild before staging')
binary=source/'html-engine-probe'
if hashlib.sha256(binary.read_bytes()).hexdigest()!=stamp['binary_sha256']:
    raise SystemExit('Native HTML binary differs from recorded build')
# This is only a development build artifact in ignored target/, never /usr/bin.
destination=ROOT/'target/release'
destination.mkdir(parents=True,exist_ok=True)
fd,name=tempfile.mkstemp(prefix='.files-html-helper-',dir=destination)
try:
    with os.fdopen(fd,'wb') as output,binary.open('rb') as input:
        shutil.copyfileobj(input,output)
        output.flush();os.fsync(output.fileno())
        os.fchmod(output.fileno(),0o755)
    os.replace(name,destination/'files-html-helper')
finally:
    Path(name).unlink(missing_ok=True)
shutil.copyfile(ROOT/'scripts/fixtures/html-engine/THIRD-PARTY.txt',destination/'files-html-helper.LICENSES')
(destination/'files-html-helper.build.json').write_text(json.dumps(stamp,indent=2)+'\n')
print(destination/'files-html-helper')
