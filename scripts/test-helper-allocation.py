#!/usr/bin/env python3
"""Functional allocation wire checks on private fixtures; no resource measurements."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--helper', type=Path, default=ROOT / 'target/release/files-helper')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--identity-limit', action='store_true')
args, rest = parser.parse_known_args()
RECEIPT = {'scope': 'private fixture functional wire only; no UI/native input/install/resource claim',
           'resource_gate': 'NOT_MEASURED', 'startup_gate': 'NOT_MEASURED',
           'helper': str(args.helper.resolve()),
           'sha256': hashlib.sha256(args.helper.read_bytes()).hexdigest(), 'cases': []}


class Contract(unittest.TestCase):
    def setUp(self):
        base = Path.home() / '.cache/files-test'
        base.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix='allocation-cli-', dir=base)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def invoke(self, path=None, command='measure_directory_allocated', **overrides):
        path = self.root if path is None else path
        request = {'command': command, 'id': 41, 'path': path if isinstance(path, list) else str(path)}
        if command == 'measure_directory_allocated':
            request['version'] = 1
        request.update(overrides)
        result = subprocess.run([str(args.helper), command], input=json.dumps(request) + '\n',
                                text=True, capture_output=True, timeout=25)
        messages = [json.loads(line) for line in result.stdout.splitlines()]
        RECEIPT['cases'].append({'test': self.id(), 'request': request, 'exit': result.returncode,
                                 'messages': messages, 'stderr': result.stderr})
        self.assertEqual(result.returncode, 0, result.stderr)
        return messages

    def allocated(self, path=None, **overrides):
        messages = self.invoke(path, **overrides)
        self.assertGreaterEqual(len(messages), 3)
        self.assertTrue(all(m['id'] == 41 for m in messages))
        self.assertEqual(messages[0], {'status': 'usage_progress', 'id': 41, 'bytes': 0, 'entries': 0})
        previous = (0, 0)
        for message in messages[:-1]:
            self.assertEqual(set(message), {'status', 'id', 'bytes', 'entries'})
            self.assertEqual(message['status'], 'usage_progress')
            value = (message['bytes'], message['entries'])
            self.assertGreaterEqual(value[0], previous[0])
            self.assertGreaterEqual(value[1], previous[1])
            self.assertLessEqual(value[1], 1_000_000)
            previous = value
        self.assertEqual(messages[-1]['status'], 'allocated_usage')
        report = messages[-1]['report']
        self.assertEqual(report['version'], 1)
        self.assertEqual(previous, (report['bytes'], report['entries']))
        return report

    def test_sparse_hardlinks_and_legacy_apparent_agree_with_owned_metadata(self):
        (self.root / 'sub').mkdir()
        sparse = self.root / 'sparse'
        with sparse.open('wb') as file:
            file.truncate(16 * 1024 * 1024)
        data = self.root / 'data'
        data.write_bytes(b'x' * 4096)
        os.link(data, self.root / 'sub/alias')
        os.link(sparse, self.root / 'sub/sparse-alias')
        os.symlink(data, self.root / 'link')
        before = (data.stat(), sparse.stat(), hashlib.sha256(data.read_bytes()).hexdigest())
        report = self.allocated()
        self.assertEqual((report['bytes'], report['entries']), (32 * 1024 * 1024 + 8192, 6))
        names = (2 * before[0].st_blocks + 2 * before[1].st_blocks) * 512
        unique = (before[0].st_blocks + before[1].st_blocks) * 512
        self.assertEqual(report['allocation'], {'by_name': {'state': 'bytes', 'value': names},
                                               'by_identity': {'state': 'bytes', 'value': unique}})
        for command in ['du', 'measure_directory', 'preview_directory']:
            messages = self.invoke(command=command)
            self.assertEqual(messages[-1], {'status': 'usage', 'id': 41,
                                           'bytes': report['bytes'], 'entries': report['entries']})
            if command != 'measure_directory':
                self.assertEqual(len(messages), 1)
        self.assertEqual((data.stat().st_ino, sparse.stat().st_ino), (before[0].st_ino, before[1].st_ino))
        self.assertEqual(hashlib.sha256(data.read_bytes()).hexdigest(), before[2])
        self.assertEqual(sparse.stat().st_size, 16 * 1024 * 1024)

    def test_empty_success_has_measured_zero(self):
        report = self.allocated()
        self.assertEqual((report['bytes'], report['entries']), (0, 0))
        self.assertEqual(report['allocation'], {'by_name': {'state': 'bytes', 'value': 0},
                                               'by_identity': {'state': 'bytes', 'value': 0}})

    def test_future_versions_and_zero_id_reject_before_traversal(self):
        for overrides in [{'version': 0}, {'version': 2}, {'id': 0}]:
            messages = self.invoke(self.root / 'does-not-exist', **overrides)
            self.assertEqual(len(messages), 1)
            self.assertEqual(messages[0]['status'], 'error')
            self.assertIn('Versión o identidad', messages[0]['message'])

    def test_command_binding_and_malformed_request_reject_before_output(self):
        base = {'command': 'measure_directory_allocated', 'id': 41,
                'version': 1, 'path': str(self.root)}
        missing = dict(base)
        del missing['version']
        for request in [missing, dict(base, version=-1), dict(base, version=True),
                        dict(base, path='/bad\x00path'), dict(base, path='/' + 'x' * 4096),
                        dict(base, command='measure_directory')]:
            result = subprocess.run([str(args.helper), 'measure_directory_allocated'],
                                    input=json.dumps(request) + '\n', text=True,
                                    capture_output=True, timeout=5)
            RECEIPT['cases'].append({'test': self.id(), 'request': request,
                                     'exit': result.returncode, 'stdout': result.stdout,
                                     'stderr': result.stderr})
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, '')

    def test_relative_and_final_symlink_reject_without_partial_result(self):
        os.symlink(self.root, self.root / 'link')
        for path in ['.', self.root / 'link', self.root / 'missing']:
            messages = self.invoke(path)
            self.assertEqual(len(messages), 1)
            self.assertEqual(messages[0]['status'], 'error')

    def test_raw_root_roundtrips_and_cross_root_identity_is_not_global_union(self):
        raw = os.fsencode(self.root) + b'/raw-\xff'
        os.mkdir(raw)
        with open(raw + b'/data', 'wb') as file:
            file.write(b'raw')
        report = self.allocated(list(raw))
        self.assertEqual((report['bytes'], report['entries']), (3, 1))
        second = self.root / 'second'
        second.mkdir()
        os.link(raw + b'/data', second / 'alias')
        other = self.allocated(second)
        self.assertEqual(report['allocation'], other['allocation'])
        # These are two root observations of the same inode, not a union total.

    @unittest.skipUnless(args.identity_limit, '65,536-identity fixture not requested')
    def test_identity_boundary_alias_at_cap_and_explicit_unavailable_beyond_cap(self):
        allocated = 0
        for index in range(65_536):
            path = self.root / str(index)
            path.touch(exist_ok=False)
            allocated += path.stat().st_blocks * 512
        report = self.allocated()
        self.assertEqual(report['entries'], 65_536)
        self.assertEqual(report['allocation']['by_identity'], {'state': 'bytes', 'value': allocated})
        os.link(self.root / '0', self.root / 'alias')
        alias = (self.root / 'alias').stat().st_blocks * 512
        report = self.allocated()
        self.assertEqual(report['entries'], 65_537)
        self.assertEqual(report['allocation'], {'by_name': {'state': 'bytes', 'value': allocated + alias},
                                               'by_identity': {'state': 'bytes', 'value': allocated}})
        (self.root / 'extra').touch(exist_ok=False)
        extra = (self.root / 'extra').stat().st_blocks * 512
        report = self.allocated()
        self.assertEqual((report['bytes'], report['entries']), (0, 65_538))
        self.assertEqual(report['allocation'], {'by_name': {'state': 'bytes', 'value': allocated + alias + extra},
                                               'by_identity': {'state': 'unavailable', 'value': 'identity_limit'}})


if __name__ == '__main__':
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(Contract)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    RECEIPT.update(status='PASS' if result.wasSuccessful() else 'FAIL', tests=result.testsRun,
                   skipped=len(result.skipped))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(RECEIPT, indent=2) + '\n')
    raise SystemExit(0 if result.wasSuccessful() else 1)
