#!/usr/bin/env python3
"""Render promo shots from the real game.

    marketing/promo/tools/capture.py [--build] SHOTS.json [name ...]

SHOTS.json maps a shot name to a shot (see src/promo.rs). Each is rendered by
the game's `promo` feature into marketing/promo/.cache/footage/<name>.mp4, silently. Laps and settings the
recordings save go to marketing/promo/.cache/data, never the player's own.
"""
import json, os, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CACHE = ROOT / "marketing/promo/.cache"
BIN = ROOT / "target/debug/todora"

def main():
    args = sys.argv[1:]
    if args and args[0] == "--build":
        args = args[1:]
        subprocess.run(["cargo", "build", "--locked", "--features", "promo"], cwd=ROOT, check=True)
    shots = json.loads(Path(args[0]).read_text())
    wanted = args[1:] or list(shots)
    (CACHE / "footage").mkdir(parents=True, exist_ok=True)
    (CACHE / "shots").mkdir(parents=True, exist_ok=True)
    for name in wanted:
        shot = dict(shots[name])
        shot["out"] = str(CACHE / "footage" / f"{name}.mp4")
        spec = CACHE / "shots" / f"{name}.json"
        # Each shot starts from nothing saved, unless it asks for a lap to chase.
        data = CACHE / "data" / name
        if data.exists():
            raise SystemExit(f"Capture data already exists: {data}. Choose a new shot name or archive that folder first.")
        data.mkdir(parents=True)
        # `ghost_from` races the best lap another shot saved: its ghost.
        source = shot.pop("ghost_from", None)
        if source:
            laps = CACHE / "data" / source / "laps"
            (data / "laps").mkdir(exist_ok=True)
            for lap in laps.iterdir():
                (data / "laps" / lap.name).write_bytes(lap.read_bytes())
        extra_env = shot.pop("env", {})
        spec.write_text(json.dumps(shot, indent=1))
        env = dict(os.environ, **extra_env, BEVY_ASSET_ROOT=str(ROOT), TODORA_PROMO=str(spec), TODORA_PROMO_DATA=str(data))
        start = time.time()
        log = CACHE / "shots" / f"{name}.log"
        with open(log, "w") as out:
            result = subprocess.run([str(BIN)], env=env, stdout=out, stderr=subprocess.STDOUT)
        status = "ok" if result.returncode == 0 else f"FAILED ({result.returncode}, see {log})"
        print(f"{name}: {status} in {time.time() - start:.0f}s", flush=True)
        if result.returncode:
            sys.exit(1)

if __name__ == "__main__":
    main()
