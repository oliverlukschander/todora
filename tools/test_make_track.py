"""Offline regression checks for imported racing order."""

import json
import re
import tempfile
import unittest
from pathlib import Path

import make_track


class RacingDirectionTests(unittest.TestCase):
    def test_reversing_preserves_the_start_and_every_fix(self):
        fixes = [[10, 50], [11, 50], [11, 49], [10, 49]]
        reversed_fixes = make_track.orient(fixes, "anticlockwise")
        self.assertEqual(reversed_fixes, [fixes[0], fixes[3], fixes[2], fixes[1]])
        self.assertEqual(make_track.orient(reversed_fixes, "clockwise"), fixes)
        self.assertEqual(make_track.orient(fixes, "clockwise"), fixes)
        self.assertEqual(make_track.orient(reversed_fixes, "anticlockwise"), reversed_fixes)

    def test_rotation_and_direction_keep_the_requested_line(self):
        fixes = [[10, 50], [11, 50], [11, 49], [10, 49]]
        at_line = make_track.start_at(fixes, [49, 11])
        self.assertEqual(make_track.orient(at_line, "anticlockwise")[0], [11, 49])

    def test_unknown_or_ambiguous_direction_is_not_guessed(self):
        with self.assertRaises(ValueError):
            make_track.orient([[0, 0], [1, 0], [0, 1]], None)
        with self.assertRaises(ValueError):
            make_track.orient([[0, 0], [1, 1], [0, 1], [1, 0]], "clockwise")

    def test_every_baked_trace_matches_its_verified_direction(self):
        recorded = json.loads(make_track.SOURCES.read_text())
        modules = {p.stem for p in make_track.CIRCUITS.glob("*.rs") if p.stem != "mod"}
        self.assertEqual(modules, set(recorded))
        for module, source in recorded.items():
            with self.subTest(circuit=module):
                text = (make_track.CIRCUITS / f"{module}.rs").read_text()
                points = [tuple(map(float, row)) for row in re.findall(
                    r"\[(-?\d+\.\d+), (-?\d+\.\d+), (-?\d+\.\d+)\]",
                    text.split("const CENTRELINE:")[1],
                )]
                self.assertEqual(len(points), source["fixes"])
                area = sum(a[0] * b[2] - b[0] * a[2]
                           for a, b in zip(points, points[1:] + points[:1]))
                self.assertNotEqual(area, 0)
                self.assertEqual("clockwise" if area > 0 else "anticlockwise",
                                 source["direction"])

    def test_local_traces_reproduce_baked_coordinates_and_heights_offline(self):
        for module, recorded in make_track.sources().items():
            if not recorded["source_id"].endswith(".geojson"):
                continue
            with self.subTest(circuit=module):
                path = make_track.CIRCUITS.parents[2] / recorded["source_id"]
                properties, fixes, elevations = make_track.local_trace(path)
                self.assertEqual(properties["revision"], recorded["revision"])
                fixes = make_track.orient(
                    make_track.start_at(fixes, recorded["line"]), recorded["direction"])
                low = min(elevations.values())
                expected = [(round(x, 2), round(elevations[tuple(fix)] - low), round(z, 2))
                            for fix, (x, z) in zip(fixes, make_track.plan(fixes))]
                text = (make_track.CIRCUITS / f"{module}.rs").read_text()
                actual = [tuple(map(float, row)) for row in re.findall(
                    r"\[(-?\d+\.\d+), (-?\d+\.\d+), (-?\d+\.\d+)\]",
                    text.split("const CENTRELINE:")[1])]
                self.assertEqual(actual, expected)

    def test_local_trace_rejects_missing_elevation_and_open_laps(self):
        properties = dict(source="survey", source_id="track", revision="a" * 40,
                          line=[50, 10], direction="clockwise")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trace.geojson"
            for coordinates in (
                [[10, 50], [11, 50], [11, 49], [10, 50]],
                [[10, 50, 2], [11, 50, 3], [11, 49, 4], [10, 49, 5]],
                [[10, 50, 2], [11, 50, float("nan")], [11, 49, 4], [10, 50, 2]],
            ):
                path.write_text(json.dumps({"features": [{"properties": properties,
                    "geometry": {"type": "LineString", "coordinates": coordinates}}]}))
                with self.assertRaises(ValueError):
                    make_track.local_trace(path)

    def test_fuji_keeps_the_dunlop_chicane_instead_of_the_bypass(self):
        path = make_track.CIRCUITS.parents[2] / "docs/track-screening/traces/fuji.geojson"
        _, fixes, _ = make_track.local_trace(path)
        # Surveyed northward leg of the chicane. The shortcut runs well west
        # of this point; a similar total lap length alone cannot catch it.
        lon, lat = 138.9238369, 35.365111
        nearest = min(((x - lon) * 91000) ** 2 + ((y - lat) * 111000) ** 2
                      for x, y in fixes)
        self.assertLess(nearest, 5 ** 2)


if __name__ == "__main__":
    unittest.main()
