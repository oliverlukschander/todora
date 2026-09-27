#!/usr/bin/env python3
"""Prepare three credited engine loops; requires ffmpeg, no Python packages."""
import array
import math
from pathlib import Path
import subprocess
import sys
import wave

ROOT = Path(__file__).resolve().parents[1]
RATE = 44100
# Excerpts of one acceleration recording, not isolated measured RPM takes.
REGIONS = [('low', 1.7, 0.8), ('mid', 4.9, 0.7), ('high', 6.1, 0.9)]


def main():
    decoded = subprocess.check_output([
        'ffmpeg', '-v', 'error', '-i', str(ROOT / 'art/audio/engine-source.mp3'),
        '-ac', '1', '-ar', str(RATE), '-af', 'highpass=f=45,lowpass=f=6500',
        '-f', 's16le', '-',
    ])
    samples = array.array('h', decoded)
    if sys.byteorder != 'little':
        samples.byteswap()
    for name, start, seconds in REGIONS:
        source = samples[round(start * RATE):round((start + seconds) * RATE)]
        # Overlap the end with the head; the wrap then continues after the head.
        fade = round(0.08 * RATE)
        loop = list(source[fade:])
        for i in range(fade):
            t = i / (fade - 1)
            loop[-fade + i] = source[-fade + i] * (1 - t) + source[i] * t
        mean = sum(loop) / len(loop)
        loop = [x - mean for x in loop]
        rms = math.sqrt(sum(x * x for x in loop) / len(loop))
        gain = min(0.15 * 32768 / rms, 0.85 * 32767 / max(map(abs, loop)))
        pcm = array.array('h', (round(x * gain) for x in loop))
        if sys.byteorder != 'little':
            pcm.byteswap()
        (ROOT / f'assets/audio/engine-{name}.s16le').write_bytes(pcm.tobytes())
        with wave.open(str(ROOT / f'art/audio/engine-{name}.wav'), 'wb') as out:
            out.setparams((1, 2, RATE, 0, 'NONE', 'not compressed'))
            out.writeframes(pcm.tobytes())
        print(f'{name}: {len(pcm) / RATE:.2f}s, crossfaded, mono {RATE} Hz')


if __name__ == '__main__':
    main()
