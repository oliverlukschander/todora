"""Offline regression checks for imported racing order."""

import json
import re
import unittest

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


if __name__ == "__main__":
    unittest.main()
