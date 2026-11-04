#!/usr/bin/env python3
"""Regresión independiente del prólogo/epílogo XML DOCX vía helper real.

No compila, no abre UI y no mide recursos. Requiere un helper con PreviewOffice.
"""
import argparse
import hashlib
import io
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORD = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
DOC = (f'<w:document xmlns:w="{WORD}"><w:body><w:p><w:r>'
       '<w:t><![CDATA[niño & 文档]]></w:t></w:r></w:p></w:body></w:document>').encode()
DECL = b'<?xml version="1.0" encoding="UTF-8"?>'
CASES = [
    ('valid-cdata-in-text', DOC, True),
    ('valid-declaration', DECL + DOC, True),
    ('valid-bom-declaration', b'\xef\xbb\xbf' + DECL + DOC, True),
    ('valid-comments-outside-root', b'<!--before-->' + DOC + b'<!--after-->', True),
    ('cdata-before-root', b'<![CDATA[unexpected]]>' + DOC, False),
    ('cdata-after-root', DOC + b'<![CDATA[unexpected]]>', False),
    ('empty-cdata-before-root', b'<![CDATA[]]>' + DOC, False),
    ('duplicate-declaration', DECL + DECL + DOC, False),
    ('declaration-after-comment', b'<!--before-->' + DECL + DOC, False),
    ('declaration-after-root', DOC + DECL, False),
]


def revision(path):
    s = path.stat()
    return dict(device=s.st_dev, inode=s.st_ino, bytes=s.st_size,
                modified_seconds=s.st_mtime_ns // 10**9,
                modified_nanoseconds=s.st_mtime_ns % 10**9,
                changed_seconds=s.st_ctime_ns // 10**9,
                changed_nanoseconds=s.st_ctime_ns % 10**9)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--helper', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--compression',choices=['stored','deflate'],default='stored')
    parser.add_argument('--descriptor',action='store_true')
    args = parser.parse_args()
    helper = args.helper.resolve(strict=True)
    base = ROOT / '.cargo/tmp'
    base.mkdir(parents=True, exist_ok=True)
    report = dict(functional_gate='FAIL', resource_gate='NOT_MEASURED',
                  helper_sha256=hashlib.sha256(helper.read_bytes()).hexdigest(),
                  compression=args.compression,descriptor=args.descriptor,cases=[])
    with tempfile.TemporaryDirectory(prefix='office-xml-wire-', dir=base) as temp:
        for index, (name, xml, accepted) in enumerate(CASES, 1):
            path = Path(temp) / (name + '.docx')
            class Sequential(io.BytesIO):
                def seekable(self):return False
                def seek(self,*args):raise io.UnsupportedOperation('owned nonseekable ZIP fixture')
            buffer=Sequential() if args.descriptor else None
            method=zipfile.ZIP_DEFLATED if args.compression=='deflate' else zipfile.ZIP_STORED
            with zipfile.ZipFile(buffer if buffer is not None else path,'w',compression=method) as package:
                package.writestr('word/document.xml',xml)
            if buffer is not None:path.write_bytes(buffer.getvalue())
            source = revision(path)
            request = dict(command='preview_office', id=index, path=str(path), revision=source)
            item = dict(name=name, expected='OfficeText' if accepted else 'Error', passed=False)
            try:
                result = subprocess.run([str(helper), 'preview_office'],
                                        input=json.dumps(request).encode() + b'\n',
                                        capture_output=True, timeout=8)
                response = json.loads(result.stdout)
                item['response'] = response
                item['exit_code'] = result.returncode
                kind = response.get('status')
                item['passed'] = (result.returncode == 0 and response.get('id') == index
                                  and kind == ('office_text' if accepted else 'error'))
                if accepted:
                    item['passed'] &= (response.get('source') == source
                                       and response.get('text') == 'niño & 文档\n'
                                       and response.get('truncated') is False)
            except (subprocess.TimeoutExpired, ValueError, OSError) as error:
                item['error'] = str(error)
            report['cases'].append(item)
    if all(item['passed'] for item in report['cases']):
        report['functional_gate'] = 'PASS'
    with args.report.open('x') as output:
        json.dump(report, output, ensure_ascii=False, indent=2)
        output.write('\n')
    print(report['functional_gate'], args.report)
    return 0 if report['functional_gate'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
