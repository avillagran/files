#!/usr/bin/env python3
"""Compile/link the actual GLES assets in a temporary hardware EGL context.

Linux capability check, not application integration or a performance benchmark.
Run serialized with other graphics checks. No dependencies beyond Python/Mesa.
"""
import ctypes as c
import ctypes.util
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PTR, INT, UINT = c.c_void_p, c.c_int, c.c_uint


def bind(lib, name, args, result):
    function = getattr(lib, name)
    function.argtypes, function.restype = args, result
    return function


class Context:
    def __init__(self, width=1, height=1):
        if not 1 <= width <= 2048 or not 1 <= height <= 2048:
            raise ValueError('Probe surface exceeds bounds')
        self.width, self.height = width, height
        self.egl = c.CDLL(ctypes.util.find_library('EGL') or 'libEGL.so.1')
        self.gl = c.CDLL(ctypes.util.find_library('GLESv2') or 'libGLESv2.so.2')
        self.display = self.surface = self.context = None

    def egl_call(self, name, args, result=UINT):
        return bind(self.egl, name, args, result)

    def __enter__(self):
        try:
            self.display = self.egl_call('eglGetPlatformDisplay', [UINT, PTR, PTR], PTR)(0x31dd, None, None)
            major, minor = INT(), INT()
            if not self.egl_call('eglInitialize', [PTR, c.POINTER(INT), c.POINTER(INT)])(
                    self.display, c.byref(major), c.byref(minor)):
                raise RuntimeError('eglInitialize failed')
            if not self.egl_call('eglBindAPI', [UINT])(0x30a0):
                raise RuntimeError('eglBindAPI failed')
            attrs = (INT * 13)(0x3033, 1, 0x3040, 0x40, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3021, 8, 0x3038)
            config, count = PTR(), INT()
            if not self.egl_call('eglChooseConfig', [PTR, c.POINTER(INT), c.POINTER(PTR), INT, c.POINTER(INT)])(
                    self.display, attrs, c.byref(config), 1, c.byref(count)) or not count.value:
                raise RuntimeError('No GLES3 pbuffer configuration')
            self.surface = self.egl_call('eglCreatePbufferSurface', [PTR, PTR, c.POINTER(INT)], PTR)(
                self.display, config, (INT * 5)(0x3057, self.width, 0x3056, self.height, 0x3038))
            self.context = self.egl_call('eglCreateContext', [PTR, PTR, PTR, c.POINTER(INT)], PTR)(
                self.display, config, None, (INT * 3)(0x3098, 3, 0x3038))
            if not self.surface or not self.context or not self.egl_call('eglMakeCurrent', [PTR, PTR, PTR, PTR])(
                    self.display, self.surface, self.surface, self.context):
                raise RuntimeError('EGL context activation failed')
            self.info = {'egl': f'{major.value}.{minor.value}'}
            string = bind(self.gl, 'glGetString', [UINT], c.c_char_p)
            for key, enum in [('vendor', 0x1f00), ('renderer', 0x1f01), ('version', 0x1f02), ('glsl', 0x8b8c)]:
                raw = string(enum)
                self.info[key] = raw.decode() if raw else ''
            renderer = self.info['renderer'].lower()
            if not renderer or any(name in renderer for name in ('llvmpipe', 'softpipe', 'lavapipe', 'swrast', 'software')):
                raise RuntimeError(f'Hardware renderer required: {renderer}')
            return self
        except Exception:
            self.__exit__(None, None, None)
            raise

    def __exit__(self, *_):
        if self.display:
            self.egl_call('eglMakeCurrent', [PTR, PTR, PTR, PTR])(self.display, None, None, None)
            if self.context:
                self.egl_call('eglDestroyContext', [PTR, PTR])(self.display, self.context)
            if self.surface:
                self.egl_call('eglDestroySurface', [PTR, PTR])(self.display, self.surface)
            self.egl_call('eglTerminate', [PTR])(self.display)


def status(gl, handle, kind, enum):
    value = INT()
    bind(gl, f'glGet{kind}iv', [UINT, UINT, c.POINTER(INT)], None)(handle, enum, c.byref(value))
    length = INT()
    bind(gl, f'glGet{kind}iv', [UINT, UINT, c.POINTER(INT)], None)(handle, 0x8b84, c.byref(length))
    log = c.create_string_buffer(max(1, min(length.value, 65536)))
    bind(gl, f'glGet{kind}InfoLog', [UINT, INT, c.POINTER(INT), PTR], None)(handle, len(log), None, log)
    return bool(value.value), log.value.decode(errors='replace')


def check(gl, vertex, fragment, on_link=None):
    shaders, program = [], 0
    logs = []
    try:
        for path, stage in [(vertex, 0x8b31), (fragment, 0x8b30)]:
            shader = bind(gl, 'glCreateShader', [UINT], UINT)(stage)
            shaders.append(shader)
            code = c.c_char_p(path.read_bytes())
            bind(gl, 'glShaderSource', [UINT, INT, c.POINTER(c.c_char_p), c.POINTER(INT)], None)(shader, 1, c.byref(code), None)
            bind(gl, 'glCompileShader', [UINT], None)(shader)
            passed, log = status(gl, shader, 'Shader', 0x8b81)
            logs.append({'file': path.name, 'log': log})
            if not passed:
                return {'passed': False, 'stage': path.name, 'logs': logs}
        program = bind(gl, 'glCreateProgram', [], UINT)()
        for shader in shaders:
            bind(gl, 'glAttachShader', [UINT, UINT], None)(program, shader)
        bind(gl, 'glLinkProgram', [UINT], None)(program)
        passed, log = status(gl, program, 'Program', 0x8b82)
        report = {'passed': passed, 'stage': 'link', 'logs': logs, 'link_log': log,
                  'sha256': hashlib.sha256(fragment.read_bytes()).hexdigest()}
        if passed and on_link is not None:
            report['render'] = on_link(gl, program, fragment.stem)
        return report
    finally:
        if program:
            bind(gl, 'glDeleteProgram', [UINT], None)(program)
        for shader in shaders:
            bind(gl, 'glDeleteShader', [UINT], None)(shader)


def main():
    report = {'scope': 'hardware GLES3 compile/link only; no app rendering, visual correctness, or performance claim'}
    try:
        assets = ROOT / 'assets/shaders'
        with Context() as ctx:
            report['context'] = ctx.info
            report['shaders'] = {name: check(ctx.gl, assets / 'quad.vert', assets / f'{name}.frag')
                                 for name in ('threadaura', 'reveal', 'starfield', 'activity')}
            report['passed'] = all(row['passed'] for row in report['shaders'].values())
    except Exception as exc:
        report.update(passed=False, error=str(exc))
    print(json.dumps(report, indent=2))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
