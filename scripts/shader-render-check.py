#!/usr/bin/env python3
"""Render actual GLES effects into small GPU fixtures, not the application UI."""
import ctypes as c
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import zlib

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('shader_check', ROOT / 'scripts/shader-check.py')
shader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shader)
WIDTH, HEIGHT = 384, 128
OUT = ROOT / 'docs/evidence/shaders'


def png(path, pixels):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    # PNG stores straight alpha; GL readback is premultiplied. Convert only the
    # exported image, keeping all numerical assertions on the original GPU bytes.
    straight = bytearray(pixels)
    for i in range(0, len(straight), 4):
        alpha = pixels[i + 3]
        for channel in range(3):
            straight[i + channel] = min(255, (pixels[i + channel] * 255 + alpha // 2) // alpha) if alpha else 0
    pixels = bytes(straight)
    stride = WIDTH * 4
    rows = b''.join(b'\0' + pixels[y * stride:(y + 1) * stride] for y in reversed(range(HEIGHT)))
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', WIDTH, HEIGHT, 8, 6, 0, 0, 0))
                     + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def render(gl, program, name):
    def fn(name, args, ret=None):
        return shader.bind(gl, name, args, ret)
    U, I, F, P = c.c_uint, c.c_int, c.c_float, c.c_void_p
    fn('glUseProgram', [U])(program)
    fn('glViewport', [I, I, I, I])(0, 0, WIDTH, HEIGHT)
    for option in (0x0be2, 0x0b71, 0x0bd0):  # blend, depth, dither
        fn('glDisable', [U])(option)
    vao, texture = U(), U()
    fn('glGenVertexArrays', [I, c.POINTER(U)])(1, c.byref(vao))
    fn('glBindVertexArray', [U])(vao.value)
    fn('glGenTextures', [I, c.POINTER(U)])(1, c.byref(texture))
    try:
        fn('glActiveTexture', [U])(0x84c0)
        fn('glBindTexture', [U, U])(0x0de1, texture.value)
        for key, value in ((0x2801, 0x2601), (0x2800, 0x2601), (0x2802, 0x812f), (0x2803, 0x812f)):
            fn('glTexParameteri', [U, U, I])(0x0de1, key, value)
        source = bytes(v for y in range(HEIGHT) for x in range(WIDTH)
                       for v in (30 + x * 150 // WIDTH, 60 + y * 150 // HEIGHT,
                                 180 if (x // 24 + y // 24) % 2 else 70, 255))
        src = c.create_string_buffer(source)
        fn('glTexImage2D', [U, I, I, I, I, I, U, U, P])(
            0x0de1, 0, 0x1908, WIDTH, HEIGHT, 0, 0x1908, 0x1401, src)
        location = fn('glGetUniformLocation', [U, c.c_char_p], I)
        fn('glUniform1i', [I, I])(location(program, b'source'), 0)
        values = {
            'opacity': 1, 'clock': 1.2, 'working': 1, 'attention': 0, 'burst': 0,
            'radius': 14, 'seed': 3, 'intensity': .85, 'workingStyle': 0,
            'attentionStyle': 0, 'burstStyle': 0, 'size': (WIDTH, HEIGHT),
            'tone': (.2, .8, 1, 1), 'alert': (1, .6, .2, 1), 'flash': (.8, .7, 1, 1),
            'progress': .45, 'style': 0, 'horizontal': 1, 'reverse': 0,
            'glow': (.2, .8, 1, 1), 'drift': 1, 'meteors': 1, 'brightness': 1,
            'nebula': .3, 'scroll': 0, 'fade': .3, 'origin': (0, 0),
            'star': (.7, .9, 1, 1), 'accent': (.4, .3, 1, 1),
            'base': (.02, .04, .08, 1), 'meteor': (.9, .8, 1, 1), 'ink': 1,
            'textWidth': WIDTH, 'bandWidth': 72,
        }

        def draw(overrides=None):
            for key, value in (values | (overrides or {})).items():
                items = value if isinstance(value, tuple) else (value,)
                loc = location(program, ('u_' + key).encode())
                fn(f'glUniform{len(items)}f', [I] + [F] * len(items))(loc, *items)
            fn('glClearColor', [F, F, F, F])(0, 0, 0, 0)
            fn('glClear', [U])(0x4000)
            fn('glDrawArrays', [U, I, I])(4, 0, 3)
            pixels = (c.c_ubyte * (WIDTH * HEIGHT * 4))()
            fn('glReadPixels', [I, I, I, I, U, U, P])(0, 0, WIDTH, HEIGHT, 0x1908, 0x1401, pixels)
            error = fn('glGetError', [], U)()
            if error:
                raise RuntimeError(f'{name}: GL error 0x{error:x}')
            return bytes(pixels)

        first = draw()
        if not any(first[3::4]) or len(set(first)) < 8:
            raise RuntimeError(f'{name}: empty/constant fixture')
        if any(max(first[i:i + 3]) > first[i + 3] + 1 for i in range(0, len(first), 4)):
            raise RuntimeError(f'{name}: output is not premultiplied RGBA')
        second = draw({'clock': 4.5, 'progress': .75})
        if first == second:
            raise RuntimeError(f'{name}: state change produced identical pixels')
        if any(draw({'opacity': 0})):
            raise RuntimeError(f'{name}: zero opacity is not transparent')
        captures = []
        path = OUT / f'{name}.png'
        png(path, first)
        captures.append(str(path.relative_to(ROOT)))
        extra_checks = []
        if name == 'reveal':
            for style in range(3):
                shown = draw({'style': style, 'progress': 1})
                difference = max(abs(a - b) for a, b in zip(shown, source))
                if difference > 2:
                    raise RuntimeError(f'reveal style {style}: full extent differs from source by {difference}')
                extra_checks.append(f'style {style}: full extent matches source within 2/255')
                path = OUT / f'reveal-style-{style}.png'
                png(path, draw({'style': style}))
                captures.append(str(path.relative_to(ROOT)))
        if name == 'threadaura':
            for state, overrides in [('attention', {'working': 0, 'attention': 1}),
                                     ('burst', {'working': 0, 'burst': .4})]:
                pixels = draw(overrides)
                if not any(pixels[3::4]):
                    raise RuntimeError(f'aura {state} is empty')
                path = OUT / f'aura-{state}.png'
                png(path, pixels)
                captures.append(str(path.relative_to(ROOT)))
                extra_checks.append(f'{state}: visible output')
        return {'passed': True, 'checks': ['nonconstant visible output', 'premultiplied RGBA',
                'state changes pixels', 'zero opacity transparent'] + extra_checks,
                'rgba_sha256': hashlib.sha256(first).hexdigest(), 'captures': captures}
    finally:
        fn('glDeleteTextures', [I, c.POINTER(U)])(1, c.byref(texture))
        fn('glDeleteVertexArrays', [I, c.POINTER(U)])(1, c.byref(vao))


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    report = {'scope': 'Actual GPU fixture rendering; not file-manager UI or performance measurement',
              'dimensions': [WIDTH, HEIGHT], 'texture': 'synthetic gradient/checker, not app text or content'}
    try:
        with shader.Context(WIDTH, HEIGHT) as ctx:
            report['context'] = ctx.info
            assets = ROOT / 'assets/shaders'
            report['shaders'] = {}
            for name in ('threadaura', 'reveal', 'starfield', 'activity'):
                report['shaders'][name] = shader.check(ctx.gl, assets / 'quad.vert', assets / f'{name}.frag', render)
            report['passed'] = all(row['passed'] for row in report['shaders'].values())
    except Exception as exc:
        report.update(passed=False, error=str(exc))
    print(json.dumps(report, indent=2))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
