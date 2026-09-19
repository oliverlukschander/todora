#!/usr/bin/env python3
"""Prepare the credited tyre loop as mono 44.1 kHz PCM (macOS afconvert)."""
import array
from pathlib import Path
import subprocess
import sys
import tempfile
import wave

ROOT = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as temp:
    converted = Path(temp) / 'tyres.wav'
    subprocess.run(['afconvert', str(ROOT / 'art/audio/tyres-source.wav'),
                    str(converted), '-f', 'WAVE', '-d', 'LEI16@44100', '-c', '1'], check=True)
    with wave.open(str(converted), 'rb') as source:
        samples = array.array('h', source.readframes(source.getnframes()))
    if sys.byteorder != 'little':
        samples.byteswap()

# Overlap the tail with the head so the loop boundary has no click.
fade = 4410
loop = list(samples[fade:])
for i in range(fade):
    t = i / (fade - 1)
    loop[-fade + i] = samples[-fade + i] * (1 - t) + samples[i] * t
mean = sum(loop) / len(loop)
peak = max(abs(x - mean) for x in loop)
pcm = array.array('h', (round((x - mean) * 0.78 * 32767 / peak) for x in loop))
if sys.byteorder != 'little':
    pcm.byteswap()
(ROOT / 'assets/audio/tyres.s16le').write_bytes(pcm.tobytes())
with wave.open(str(ROOT / 'art/audio/tyres-loop.wav'), 'wb') as output:
    output.setparams((1, 2, 44100, 0, 'NONE', 'not compressed'))
    output.writeframes(pcm.tobytes())
print(f'Prepared {len(pcm) / 44100:.2f}s tyre loop')
