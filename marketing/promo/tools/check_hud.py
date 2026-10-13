#!/usr/bin/env python3
"""Check every frame's static lime HUD label for the 1080p capture scale regression.

Uses ffmpeg only. Pass source MP4 paths, or check all five gameplay shots.
Allows two pixels of compression/antialiasing variation, not a scale change.
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FOOTAGE = ROOT / 'marketing/promo/.cache/footage'
WIDTH, HEIGHT = 260, 35


def check(path):
    raw = subprocess.check_output([
        'ffmpeg', '-v', 'error', '-i', str(path),
        '-vf', f'crop={WIDTH}:{HEIGHT}:40:40', '-pix_fmt', 'rgb24', '-f', 'rawvideo', '-',
    ])
    frame_bytes = WIDTH * HEIGHT * 3
    boxes = []
    for at in range(0, len(raw), frame_bytes):
        frame = raw[at:at + frame_bytes]
        pixels = [i for i, (r, g, b) in enumerate(zip(frame[::3], frame[1::3], frame[2::3]))
                  if g > 150 and r > 90 and g > r * 1.1 and r > b * 1.2]
        if not pixels:
            raise SystemExit(f'{path.name}: missing label on frame {len(boxes)}')
        xs = [i % WIDTH for i in pixels]
        ys = [i // WIDTH for i in pixels]
        boxes.append((min(xs), min(ys), max(xs), max(ys)))
    if not boxes:
        raise SystemExit(f'{path.name}: no frames')
    spans = [max(axis) - min(axis) for axis in zip(*boxes)]
    print(f'{path.name}: {len(boxes)} frames; label bounds vary by {spans} px', flush=True)
    return max(spans) <= 2


if __name__ == '__main__':
    paths = list(map(Path, sys.argv[1:])) or [
        FOOTAGE / f'{name}.mp4' for name in ['corner', 'cockpit', 'finish', 'ghost', 'bonkers']
    ]
    results = [check(path) for path in paths]
    raise SystemExit(0 if all(results) else 1)
