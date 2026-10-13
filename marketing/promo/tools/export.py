#!/usr/bin/env python3
"""Export reviewed frames and the silent 0.12.0 README clip with ffmpeg."""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FOOTAGE = ROOT / 'marketing/promo/.cache/footage'
IMAGES = ROOT / 'docs/images'
SPEC = ROOT / 'marketing/promo/shots/edit.json'


def ffmpeg(*args):
    subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', *map(str, args)], check=True)


def main():
    edit = json.loads(SPEC.read_text())
    IMAGES.mkdir(parents=True, exist_ok=True)
    for name, (shot, seconds) in edit['stills'].items():
        ffmpeg('-ss', seconds, '-i', FOOTAGE / f'{shot}.mp4', '-frames:v', 1,
               '-q:v', 3, '-update', 1, IMAGES / f'{name}.jpg')
    clips = []
    for index, (shot, start, duration) in enumerate(edit['cuts']):
        clip = FOOTAGE / f'cut-{index}.mp4'
        ffmpeg('-ss', start, '-i', FOOTAGE / f'{shot}.mp4', '-t', duration,
               '-an', '-vf', 'scale=1920:1080,setsar=1', '-r', 60,
               '-c:v', 'libx264', '-preset', 'slow', '-crf', 24, '-profile:v', 'high',
               '-pix_fmt', 'yuv420p', '-color_primaries', 'bt709', '-color_trc', 'bt709',
               '-colorspace', 'bt709', clip)
        clips.append(clip)
    listing = FOOTAGE / 'concat.txt'
    listing.write_text(''.join(f"file '{clip.name}'\n" for clip in clips))
    master = ROOT / 'marketing/promo/exports/video/todora-0.12-gameplay-16x9.mp4'
    master.parent.mkdir(parents=True, exist_ok=True)
    ffmpeg('-f', 'concat', '-safe', 0, '-i', listing, '-c', 'copy', '-movflags', '+faststart', master)
    out = IMAGES / 'todora-short.mp4'
    ffmpeg('-i', master, '-an', '-vf', 'scale=1280:720', '-c:v', 'libx264',
           '-preset', 'slow', '-crf', 27, '-profile:v', 'high', '-pix_fmt', 'yuv420p',
           '-movflags', '+faststart', out)
    print(f'{out}: {out.stat().st_size / 1_000_000:.2f} MB')


if __name__ == '__main__':
    main()
