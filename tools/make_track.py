#!/usr/bin/env python3
"""Turn a real circuit into a Todora centreline module.

    tools/make_track.py at-1969 red_bull_ring "Red Bull Ring"
    tools/make_track.py be-1925 spa_francorchamps "Spa-Francorchamps"
    tools/make_track.py mc-1929 monaco "Monaco" --line 43.73543,7.42122

The plan comes from bacinger/f1-circuits, which traces circuit centrelines as
GeoJSON in WGS84; the heights come from the Open-Meteo DEM, sampled at those
same fixes. Both are public and both are what the existing circuits were built
from. Re-running this on an id already in the game reproduces its trace to
within a centimetre — the Red Bull Ring was projected by hand before this
existed, and a centimetre of a real circuit is a millimetre of a Todora lap.

The trace is projected flat about its own centroid — equirectangular on the
WGS84 equatorial radius, which is good to a few centimetres over a circuit —
and written out in metres, north as -Z so the map reads the way Bevy's axes do.
Heights are whole metres above the lowest point on the lap; the DEM is quantised
to the metre anyway, and `ribbon` smooths and grade-caps what it is given.

The trace is rotated to begin at the start/finish line, because the game reads
the line off the first sample and off nothing else: the paint, the grid, the lap
gate and the distance round the lap all hang from `centreline[0]`. The source
does not promise to start there — most of its traces do, three of them did not,
and the circuits built from those three carried their line half a lap from where
it belongs. So the line is a coordinate this tool is told, kept beside the trace
in `docs/track-screening/sources.json` and reapplied on every later run. Pass
--line lat,lon to set or move one; a circuit with neither a flag nor a recorded
line keeps the trace's own first fix, and then somebody has to go and check it.

Checking it means finding the pit lane: OpenStreetMap maps one for nearly every
circuit here as a `highway=raceway` way running a dozen metres off the road, and
the line belongs on the stretch that way runs alongside. The trace is only
rotated to a fix it already has, so the line lands within one fix — forty-odd
metres of real circuit, five of Todora's — of the coordinate given.

The trace revision is pinned. `master` is a moving target and a circuit that
was resurveyed between one module and the next would be two circuits generated
from "the same" source, so the revision the module was built from is written
into the module and into `docs/track-screening/sources.json`. Pass --revision to
move it deliberately; the point is that it cannot move by accident.

Nothing else here is a network dependency at build or test time. The trace and
the heights are baked into the module the moment it is generated, so the game
and its tests never reach for either. Within a run the DEM samples are cached
under ~/.cache/todora/dem, keyed by the coordinates asked for, so regenerating
a circuit — or generating forty in a row and being rate-limited part way — does
not ask the elevation service the same question twice.

Nothing here scales anything: `src/track/mod.rs` decides how much of a real
circuit a Todora lap is, and it decides it the same way for every circuit.
"""

import hashlib
import json
import math
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

TRACE = "https://raw.githubusercontent.com/bacinger/f1-circuits/{}/circuits/{}.geojson"
# The revision of bacinger/f1-circuits every circuit in the game was traced
# from. See the module docstring.
REVISION = "394d8fbe70ef2c0b0c8d23ff7bee61fa09606055"
DEM = "https://api.open-meteo.com/v1/elevation"
# WGS84 equatorial radius. The flattening does not survive the rounding to
# centimetres over a few kilometres, so the sphere is enough.
RADIUS = 6378137.0
# Open-Meteo takes this many coordinates per request.
BATCH = 100
# Answers already had, so a second run costs nothing and a run that was cut off
# part way picks up where it stopped. The DEM is a fixed dataset; a sample of it
# does not go stale.
CACHE = Path.home() / ".cache" / "todora" / "dem"
# The elevation service rate-limits a burst. Forty circuits in a row is a burst.
RETRIES = 6
CIRCUITS = Path(__file__).resolve().parent.parent / "src" / "track" / "circuits"


def fetch(url):
    """One request, retried through a rate limit rather than abandoned."""
    for attempt in range(RETRIES):
        try:
            with urllib.request.urlopen(url, timeout=60) as response:
                return json.load(response)
        except urllib.error.HTTPError as refused:
            if refused.code not in (429, 502, 503) or attempt == RETRIES - 1:
                raise
            wait = 2 ** attempt
            print(f"  {refused.code} from the server; waiting {wait}s", file=sys.stderr)
            time.sleep(wait)
    raise RuntimeError("unreachable")


def cached(url):
    """`fetch`, but the answer is kept on disk against the question."""
    CACHE.mkdir(parents=True, exist_ok=True)
    where = CACHE / (hashlib.sha256(url.encode()).hexdigest()[:32] + ".json")
    if where.exists():
        return json.loads(where.read_text())
    answer = fetch(url)
    where.write_text(json.dumps(answer))
    return answer


def trace(circuit_id, revision):
    """The circuit's GPS fixes, open: the closing duplicate is dropped."""
    data = fetch(TRACE.format(revision, circuit_id))
    feature = data["features"][0]
    fixes = feature["geometry"]["coordinates"]
    if fixes[0] == fixes[-1]:
        fixes = fixes[:-1]
    return feature["properties"], fixes


def start_at(fixes, line):
    """The trace, turned so it begins at the fix nearest the start/finish line."""
    lat0 = sum(lat for _, lat in fixes) / len(fixes)
    east = math.cos(math.radians(lat0))
    lat, lon = line
    at = min(
        range(len(fixes)),
        key=lambda i: math.hypot((fixes[i][0] - lon) * east, fixes[i][1] - lat),
    )
    return fixes[at:] + fixes[:at]


def plan(fixes):
    """Metres east and south of the centroid of the trace."""
    lon0 = sum(lon for lon, _ in fixes) / len(fixes)
    lat0 = sum(lat for _, lat in fixes) / len(fixes)
    scale = math.radians(RADIUS)
    return [
        ((lon - lon0) * scale * math.cos(math.radians(lat0)), -(lat - lat0) * scale)
        for lon, lat in fixes
    ]


def heights(fixes):
    """Whole metres above the lowest fix, from the DEM."""
    sampled = []
    for start in range(0, len(fixes), BATCH):
        batch = fixes[start : start + BATCH]
        query = urllib.parse.urlencode(
            {
                "latitude": ",".join(f"{lat:.6f}" for _, lat in batch),
                "longitude": ",".join(f"{lon:.6f}" for lon, _ in batch),
            }
        )
        sampled += cached(f"{DEM}?{query}")["elevation"]
    lowest = min(sampled)
    return [round(metres - lowest) for metres in sampled]


def lap_length(points):
    return sum(math.dist(points[i], points[(i + 1) % len(points)]) for i in range(len(points)))


def module(circuit_id, module_name, name, properties, points, ys, revision, line):
    slug = module_name.replace("_", "-")
    km = lap_length(points) / 1000.0
    rows = "".join(
        f"    [{x:.2f}, {y:.2f}, {z:.2f}],\n" for (x, z), y in zip(points, ys)
    )
    title = properties.get("Name", name)
    where = properties.get("Location", "")
    if where and where.split()[0].lower() not in title.lower():
        title = f"{title}, {where}"
    return f"""//! {title}.
//!
//! Generated by `tools/make_track.py {circuit_id} {module_name} "{name}"`.
//! GPS from bacinger/f1-circuits `{revision[:12]}`, id `{circuit_id}`
//! ({km:.3f} km). Heights from the Open-Meteo DEM, sampled at those same fixes.
//! Turned to begin at the start/finish line, {line[0]!r}, {line[1]!r}.

use super::Circuit;

pub(super) const CIRCUIT: Circuit = Circuit {{
    id: "{slug}",
    name: "{name}",
    // The trace as surveyed. Turn this up until `corners_are_corners` is happy
    // and `every_circuit_carries_a_road` is not yet unhappy: the road is five
    // times too wide for the shrunk land, so a corner needs exaggerating before
    // it is a corner, and a circuit wound tightly on itself has less room to do
    // it in than one made of straights.
    corners: 1.0,
    // What one lap comes out as once the circuit has been shrunk and had its
    // corners opened. Run `cargo test --locked --lib the_menu_shows_the_lap`
    // and write down the figure it asks for.
    lap: 0.0,
    // The shared plan scale. Raise it only if the circuit cannot carry a road
    // and a shoulder at 1, and only as far as it has to be raised: see
    // `Circuit::plan_scale` for what a multiplier gives up.
    plan_scale: 1.0,
    centreline: CENTRELINE,
    // Where the circuit passes over itself. Almost none do; see `Crossing`, and
    // `screen_tracks.py`, whose `finished_crossings` column is what says
    // whether this one needs a line here.
    crossings: &[],
}};

/// Metres from the circuit centroid. Y is height above the lowest point.
/// Connect the last sample back to the first to close the lap.
const CENTRELINE: &[[f32; 3]] = &[
{rows}];
"""


def register(module_name):
    """Add the new circuit to `circuits/mod.rs`, if it is not already there."""
    path = CIRCUITS / "mod.rs"
    source = path.read_text()
    if re.search(rf"^mod {module_name};$", source, re.M):
        return
    mods = sorted(re.findall(r"^mod (\w+);$", source, re.M) + [module_name])
    source = re.sub(
        r"^mod \w+;(\n^mod \w+;)*$",
        "\n".join(f"mod {m};" for m in mods),
        source,
        count=1,
        flags=re.M,
    )
    listed = re.search(r"const ALL: &\[Circuit\] = &\[(.*?)\];", source, re.S)
    entries = [e.strip() for e in listed.group(1).split(",") if e.strip()]
    # The list is the order the menu shows, which is by module name: it is a
    # list you find a name in rather than one that says which was added when.
    entries = sorted(entries + [f"{module_name}::CIRCUIT"])
    source = source[: listed.start()] + (
        "const ALL: &[Circuit] = &[\n"
        + "".join(f"    {e},\n" for e in entries)
        + "];"
    ) + source[listed.end() :]
    path.write_text(source)


SOURCES = CIRCUITS.parents[2] / "docs" / "track-screening" / "sources.json"


def sources():
    return json.loads(SOURCES.read_text()) if SOURCES.exists() else {}


def note(circuit_id, module_name, name, revision, fixes, points, line):
    """Record where this circuit came from, alongside the other circuits'."""
    SOURCES.parent.mkdir(parents=True, exist_ok=True)
    known = sources()
    known[module_name] = {
        "source_id": circuit_id,
        "name": name,
        "source": "bacinger/f1-circuits",
        "revision": revision,
        "heights": "Open-Meteo DEM",
        "fixes": len(fixes),
        "lap_km": round(lap_length(points) / 1000.0, 3),
        # Where the trace was turned to begin. See the module docstring.
        "line": line,
    }
    SOURCES.write_text(json.dumps(dict(sorted(known.items())), indent=2) + "\n")


def main():
    argv = sys.argv[1:]
    revision = REVISION
    line = None
    if "--revision" in argv:
        at = argv.index("--revision")
        revision = argv[at + 1]
        argv = argv[:at] + argv[at + 2 :]
    if "--line" in argv:
        at = argv.index("--line")
        line = [float(part) for part in argv[at + 1].split(",")]
        argv = argv[:at] + argv[at + 2 :]
    if len(argv) != 3:
        sys.exit(__doc__)
    circuit_id, module_name, name = argv
    properties, fixes = trace(circuit_id, revision)
    # A line already recorded outlives a rerun; only --line moves one.
    line = line or sources().get(module_name, {}).get("line") or list(reversed(fixes[0]))
    # Rounded once, here, so the module header and the provenance file spell the
    # same coordinate the same way and can be held to each other.
    line = [round(float(part), 5) for part in line]
    fixes = start_at(fixes, line)
    points = plan(fixes)
    path = CIRCUITS / f"{module_name}.rs"
    path.write_text(
        module(
            circuit_id, module_name, name, properties, points, heights(fixes), revision, line
        )
    )
    register(module_name)
    note(circuit_id, module_name, name, revision, fixes, points, line)
    subprocess.run(["cargo", "fmt", "--all"], cwd=CIRCUITS.parents[2], check=False)
    print(f"{path}: {len(fixes)} fixes, {lap_length(points) / 1000.0:.3f} km")


if __name__ == "__main__":
    main()
