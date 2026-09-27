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
PERIOD = 200  # Samples per firing cycle; shared with sound/synth.rs.
# Excerpts of one acceleration recording, not isolated measured RPM takes.
REGIONS = [('low', 1.7, 0.8, 182.5), ('mid', 4.9, 0.7, 212.9), ('high', 6.1, 0.9, 235.6)]


def decode(filters):
    decoded = subprocess.check_output([
        'ffmpeg', '-v', 'error', '-i', str(ROOT / 'art/audio/engine-source.mp3'),
        '-ac', '1', '-ar', str(RATE), '-af', filters, '-f', 'f32le', '-',
    ])
    samples = array.array('f', decoded)
    if sys.byteorder != 'little':
        samples.byteswap()
    return samples


def main():
    samples = decode('highpass=f=45,lowpass=f=6500')
    for name, start, seconds, pitch in REGIONS:
        # Isolate the firing note to locate full cycles. Forward/backward
        # filtering removes the filter's phase delay; the actual audio below
        # keeps its full spectrum, not this narrow analysis signal.
        band = f'bandpass=f={pitch}:t=h:w=50'
        fundamental = decode(f'{band},areverse,{band},areverse')
        crossings = []
        for i in range(round(start * RATE), round((start + seconds) * RATE)):
            a, b = fundamental[i - 1], fundamental[i]
            if a <= 0 < b:
                crossings.append(i - b / (b - a))
        source = []
        for a, b in zip(crossings, crossings[1:]):
            if not pitch * 0.8 < RATE / (b - a) < pitch * 1.2:
                raise ValueError(f'{name}: cannot identify a stable firing cycle')
            # Every cycle has the same length and starts at the same phase.
            # Otherwise the recorded acceleration restarts on every loop and
            # the independently pitched layers beat against one another.
            for j in range(PERIOD):
                at = a + (b - a) * j / PERIOD
                index = int(at)
                fraction = at - index
                source.append(samples[index] * (1 - fraction) + samples[index + 1] * fraction)
        # Overlap whole, aligned cycles so the join does not cancel the note.
        fade = 16 * PERIOD
        if len(source) < 2 * fade:
            raise ValueError(f'{name}: too few firing cycles')
        loop = list(source[fade:])
        for i in range(fade):
            t = i / (fade - 1)
            loop[-fade + i] = source[-fade + i] * (1 - t) + source[i] * t
        mean = sum(loop) / len(loop)
        loop = [x - mean for x in loop]
        rms = math.sqrt(sum(x * x for x in loop) / len(loop))
        gain = min(0.15 / rms, 0.85 / max(map(abs, loop))) * 32767
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
