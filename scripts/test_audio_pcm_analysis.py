"""Pure content checks, importing only the two analysis functions (no Pulse/UI I/O)."""
import array
import ast
import math
from pathlib import Path
import unittest

source=Path(__file__).with_name('audio-pcm-ui-check.py')
tree=ast.parse(source.read_text())
functions=ast.Module(body=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in ('spectrum','signal')],type_ignores=[])
namespace={'array':array,'math':math}
exec(compile(functions,str(source),'exec'),namespace)
analyze=namespace['signal']

def tone(frequency,frames=48000):
    data=array.array('h')
    for index in range(frames):
        value=int(1000*math.sin(2*math.pi*frequency*index/48000))
        data.extend((value,value))
    if __import__('sys').byteorder!='little':data.byteswap()
    return data.tobytes()

class PcmAnalysis(unittest.TestCase):
    def test_accepts_only_corresponding_stereo_tones(self):
        for frequency in (440,880):
            self.assertGreater(analyze(tone(frequency),frequency)['target_to_other_tone_energy_ratio'],100)
    def test_wrong_tone_is_rejected(self):
        with self.assertRaises(AssertionError):analyze(tone(880),440)
    def test_silence_not_accepted_as_signal_and_signal_not_silence(self):
        self.assertEqual(analyze(bytes(48000*4))['rms_s16'],0)
        with self.assertRaises(AssertionError):analyze(bytes(48000*4),440)
        with self.assertRaises(AssertionError):analyze(tone(440))
    def test_incomplete_pcm_never_passes(self):
        with self.assertRaises(AssertionError):analyze(bytes(4*100))

if __name__=='__main__':unittest.main()
