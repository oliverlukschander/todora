#!/usr/bin/env python3
"""Screen a pinned checkout of bacinger/f1-circuits through Todora's ribbon.

    python3 tools/screen_tracks.py /path/to/f1-circuits /tmp/todora-screen
    python3 tools/screen_tracks.py /path/to/f1-circuits /tmp/todora-screen-5 \
        --min-radius 5 --scale az-2016=2 --scale mc-1929=1.25 --scale us-2022=1.25

This is a plan-geometry experiment, not circuit admission. Heights are zero, so
it says nothing about hills, elevation failures, bridges, the loft's triangles,
markers or whether anybody can drive it; those are decided by the Rust suite
against a generated module, and a candidate that passes this still has all of
them to clear. What it does is answer, cheaply and for all forty at once, the
questions that come first:

    length, kept                does enough of the layout survive the shrink
    min_radius                  did opening the corners converge
    narrowest, widest, carries  does a road and a shoulder fit anywhere it goes
    raw_crossings               does the source trace cross itself
    finished_crossings          does the centreline still cross itself after
                                splining, exaggerating and opening

The cross-section columns are a *plan* answer and heights are zero here, so a
circuit that crosses itself comes out wanting a cross-section of less than
nothing however good a bridge the engine would build it. Read the two together:
`carries false` with `finished_crossings 0` is a circuit that will not fit,
and `carries false` with `finished_crossings 1` is a circuit that needs a
`Crossing` in its module and then will.

The cross-section columns reproduce `Profile::fit`: the same per-station,
per-side room, the same two bounds, the same taper. The constants come out of
`src/track/profile.rs` by name rather than being copied here, so the probe
cannot quietly describe a road the game no longer builds — if one is renamed
this stops rather than screening the wrong thing.

The output records every metric rather than stopping at Track::new's first
assertion. A radius override changes a scratch copy only; the game's source is
never changed.
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

sys.dont_write_bytecode = True
from make_track import REVISION, plan  # noqa: E402


ROOT = Path(__file__).resolve().parent.parent
# Fit constants read out of the engine, so the probe and the game agree.
WANTED = ["HALF_WIDTH", "VERGE", "GRASS_OF_VERGE", "SPARE", "CLEARANCE", "LEAST_VERGE", "TAPER"]


def constants():
    """The cross-section constants, by name, from `profile.rs`."""
    source = (ROOT / "src/track/profile.rs").read_text()
    found = {}
    for name in WANTED:
        match = re.search(rf"^(?:pub\([\w ]+\) )?const {name}: f32 = ([^;]+);", source, re.M)
        if not match:
            raise RuntimeError(f"{name} moved; review the probe before trusting it")
        found[name] = match[1].strip()
    source = (ROOT / "src/track/mod.rs").read_text()
    match = re.search(r"^const PLAN_SCALE: f32 = ([^;]+);", source, re.M)
    if not match:
        raise RuntimeError("PLAN_SCALE moved; review the probe before trusting it")
    found["PLAN_SCALE"] = match[1].strip()
    return found


def crossing_count(points):
    """Proper crossings of a closed polyline; shared endpoints are not crossings."""
    def sub(a, b):
        return a[0] - b[0], a[1] - b[1]

    def cross(a, b):
        return a[0] * b[1] - a[1] * b[0]

    count = 0
    n = len(points)
    for i, a in enumerate(points):
        r = sub(points[(i + 1) % n], a)
        for j in range(i + 2, n):
            if i == 0 and j == n - 1:
                continue
            b = points[j]
            s = sub(points[(j + 1) % n], b)
            determinant = cross(r, s)
            if abs(determinant) < 1e-8:
                continue
            t = cross(sub(b, a), s) / determinant
            u = cross(sub(b, a), r) / determinant
            count += 0 < t < 1 and 0 < u < 1
    return count


def probe(named):
    """The diagnostic executable's source: `Profile::fit`, reported not asserted."""
    written = "".join(f"const {name}: f32 = {value};\n" for name, value in named.items())
    return '''#![allow(dead_code)]
#[path="ribbon.rs"] mod ribbon;
use bevy::prelude::*;
include!("data.rs");

''' + written + '''const EDGE: f32 = HALF_WIDTH + VERGE;

/// `Profile::fit`, without the refusal: fit, taper, and report what is left.
fn fit(r: &ribbon::Ribbon) -> (f32, f32) {
    let stations = r.stations();
    let n = stations.len();
    let step = r.length() / n as f32;
    let cap = EDGE + CLEARANCE;
    let mut sides: Vec<Vec<f32>> = [-1.0f32, 1.0]
        .iter()
        .map(|&side| {
            (0..n)
                .map(|i| {
                    let inward = stations[i].curvature * side;
                    let bend = if inward > 1e-7 { (1.0 - SPARE) / inward } else { f32::MAX };
                    EDGE.min(bend).min(r.room(i, side, cap) - CLEARANCE) - HALF_WIDTH
                })
                .collect()
        })
        .collect();
    for verge in &mut sides {
        let most = TAPER * step;
        for _ in 0..2 {
            for i in 0..n {
                let next = (i + 1) % n;
                verge[next] = verge[next].min(verge[i] + most);
            }
            for i in (0..n).rev() {
                let next = (i + 1) % n;
                verge[i] = verge[i].min(verge[next] + most);
            }
        }
    }
    let all = sides.concat();
    (
        HALF_WIDTH + all.iter().copied().fold(f32::MAX, f32::min),
        HALF_WIDTH + all.iter().copied().fold(f32::MIN, f32::max),
    )
}

/// Proper crossings of the finished centreline, which is not the same question
/// as crossings of the raw trace: splining rounds a trace off and opening the
/// corners moves it, so a crossing can appear or go away.
fn crossings(line: &[Vec3]) -> usize {
    let n = line.len();
    let flat = |v: Vec3| (v.x, v.z);
    let sub = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0, a.1 - b.1);
    let cross = |a: (f32, f32), b: (f32, f32)| a.0 * b.1 - a.1 * b.0;
    let mut count = 0;
    for i in 0..n {
        let a = flat(line[i]);
        let r = sub(flat(line[(i + 1) % n]), a);
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let b = flat(line[j]);
            let s = sub(flat(line[(j + 1) % n]), b);
            let determinant = cross(r, s);
            if determinant.abs() < 1e-8 {
                continue;
            }
            let t = cross(sub(b, a), s) / determinant;
            let u = cross(sub(b, a), r) / determinant;
            if t > 0.0 && t < 1.0 && u > 0.0 && u < 1.0 {
                count += 1;
            }
        }
    }
    count
}

fn main() {
    println!("id,name,plan_multiplier,raw_crossings,finished_crossings,length,kept,\\
min_radius,narrowest,widest,carries");
    for (id, name, corners, scale, raw, points) in DATA {
        let control: Vec<Vec3> = points
            .iter()
            .map(|p| Vec3::new(p[0] * PLAN_SCALE * scale, 0.0, p[2] * PLAN_SCALE * scale))
            .collect();
        // No bridges. Heights are zero here, so a bridge would have nothing to
        // lift the road over; what the screen reports instead is that one is
        // needed, in `finished_crossings`.
        let r = ribbon::Ribbon::new(&control, *corners, &[]);
        let line: Vec<Vec3> = r.stations().iter().map(|s| s.pos).collect();
        let (narrowest, widest) = fit(&r);
        let carries = narrowest >= HALF_WIDTH + LEAST_VERGE;
        let name = name.replace('"', "\\"\\"");
        println!(
            "{id},\\"{name}\\",{scale},{raw},{},{:.6},{:.6},{:.6},{narrowest:.6},{widest:.6},{carries}",
            crossings(&line),
            r.length(),
            r.kept(),
            r.min_radius(),
        );
    }
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--min-radius", type=float)
    parser.add_argument("--scale", action="append", default=[], metavar="ID=MULTIPLIER")
    parser.add_argument(
        "--shared-scale",
        action="store_true",
        help="ignore the per-circuit multipliers the modules carry",
    )
    args = parser.parse_args()
    scales = {key: float(value) for key, value in (s.split("=") for s in args.scale)}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)

    # What the circuits already in the game are built with, so that screening
    # reproduces them rather than describing a different circuit of the same
    # name. A --scale on the command line overrides a module's own.
    corners, built = {}, {}
    for path in (ROOT / "src/track/circuits").glob("*.rs"):
        text = path.read_text()
        source_id = re.search(r"make_track.py (\S+)", text)
        factor = re.search(r"corners: ([\d.]+)", text)
        multiplier = re.search(r"plan_scale: ([\d.]+)", text)
        if source_id and factor:
            corners[source_id[1]] = float(factor[1])
        if source_id and multiplier:
            built[source_id[1]] = float(multiplier[1])

    rows = []
    for path in sorted((args.source / "circuits").glob("*.geojson")):
        feature = json.loads(path.read_text())["features"][0]
        fixes = feature["geometry"]["coordinates"]
        if fixes[0] == fixes[-1]:
            fixes = fixes[:-1]
        points = plan(fixes)
        coordinates = ",".join(f"[{x:.2f}_f32,0.0,{z:.2f}_f32]" for x, z in points)
        name = json.dumps(feature["properties"]["Name"], ensure_ascii=False)
        rows.append(
            f'("{path.stem}",{name},{corners.get(path.stem, 1.0)!r}_f32,'
            f'{scales.get(path.stem, 1.0 if args.shared_scale else built.get(path.stem, 1.0))!r}_f32,'
            f'{crossing_count(points)},&[{coordinates}]),'
        )
    if not rows:
        parser.error("source has no circuits/*.geojson")

    ribbon = (ROOT / "src/track/ribbon.rs").read_text()
    if args.min_radius is not None:
        ribbon, replaced = re.subn(
            r"const MIN_RADIUS: f32 = [\d.]+;",
            f"const MIN_RADIUS: f32 = {args.min_radius!r};", ribbon,
        )
        if replaced != 1:
            raise RuntimeError("MIN_RADIUS moved; review the probe before overriding it")
    (output / "ribbon.rs").write_text(ribbon)
    (output / "data.rs").write_text(
        "const DATA: &[(&str,&str,f32,f32,usize,&[[f32;3]])] = &[\n"
        + "\n".join(rows) + "\n];\n"
    )
    named = constants()
    (output / "main.rs").write_text(probe(named))

    build = subprocess.run(
        ["cargo", "build", "--locked", "--lib", "--message-format=json"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    )
    bevy = None
    for line in build.stdout.splitlines():
        event = json.loads(line)
        if event.get("reason") == "compiler-artifact" and event["target"]["name"] == "bevy":
            bevy = next((Path(f) for f in event["filenames"] if f.endswith(".rlib")), None)
    if bevy is None:
        raise RuntimeError("cargo did not report the Bevy library")
    subprocess.run([
        "rustc", "--edition=2024", "-C", "opt-level=1", str(output / "main.rs"),
        "-L", f"dependency={bevy.parent}", "--extern", f"bevy={bevy}",
        "-o", str(output / "screen"),
    ], cwd=ROOT, check=True)
    with (output / "screen.csv").open("w") as result:
        subprocess.run([str(output / "screen")], stdout=result, check=True)
    provenance = {
        "todora_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.source, text=True).strip(),
        "source_pinned_at": REVISION,
        "radius_override": args.min_radius,
        "plan_multipliers": "shared scale only" if args.shared_scale else {**built, **scales},
        "cross_section": named,
        "heights": "zero; plan geometry only",
        "not_screened": "elevation, the loft's triangles, markers, lap timing, driving",
    }
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(output / "screen.csv")


if __name__ == "__main__":
    main()
