"""Run two rendered instances over loopback; save screenshots and packet reports.

cargo build --features game-center,multiplayer-test
python3 tools/check_multiplayer.py suzuka monza spa-francorchamps
"""
import json
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]


def check(circuit):
    base = ROOT / "dist/multiplayer-check" / circuit
    children = []
    try:
        for role, selected in [("host", circuit), ("join", "monza" if circuit != "monza" else "suzuka")]:
            folder = base / role
            folder.mkdir(parents=True, exist_ok=True)
            for name in ["report.json", "screen.png"]:
                (folder / name).unlink(missing_ok=True)
            env = dict(os.environ, TODORA_LOCAL_PEER=role, BEVY_ASSET_ROOT=str(ROOT),
                       TODORA_MULTIPLAYER_CHECK=str(folder), TODORA_CHECK_CIRCUIT=selected)
            log = (folder / "app.log").open("w")
            child = subprocess.Popen([str(ROOT / "target/debug/todora")], cwd=ROOT, env=env,
                                     stdout=log, stderr=subprocess.STDOUT)
            children.append((role, child, log))
        for role, child, _ in children:
            code = child.wait(timeout=65)
            if code:
                raise RuntimeError(f"{circuit}/{role} exited {code}; see {base / role / 'app.log'}")
            report = json.loads((base / role / "report.json").read_text())
            assert report["driving"] and report["remote_visible"]
            assert report["remote_seq"] > 10 and report["travelled"] > 0.2
            assert report["circuit"] == circuit
            assert (base / role / "screen.png").is_file()
            print(f"{circuit}/{role}: {report['remote_seq']} packets, {report['travelled']:.2f} m driven", flush=True)
    finally:
        for _, child, log in children:
            if child.poll() is None:
                child.kill()
                child.wait()
            log.close()


if __name__ == "__main__":
    for circuit in sys.argv[1:] or ["suzuka"]:
        check(circuit)
