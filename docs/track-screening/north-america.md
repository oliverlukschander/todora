# North American endurance circuits and collections

Eight additions bring the catalogue to 51 circuits. These venues appeared in
DHH's American Le Mans Series era (2012–2013). They are contemporary surveyed
layouts, not recreations of the surface conditions in those seasons.

| Venue | Todora name | Layout | Source |
| --- | --- | --- | --- |
| Laguna Seca | Corkscrew Coast | Full course, anticlockwise | track-atlas `laguna-seca/gp` |
| Lime Rock | Connecticut Valley | Classic full course, clockwise | track-atlas `lime-rock/gp` |
| Long Beach | Pacific Harbour | Street circuit, clockwise | track-atlas `long-beach/gp` |
| Mid-Ohio | Lexington Hills | 13-turn professional course, clockwise | Pinned OpenStreetMap ways |
| Mosport / Canadian Tire Motorsport Park | Maple Forest | Grand Prix course, clockwise | track-atlas `mosport/gp` |
| Road America | Elkhart Lakes | Full course, clockwise | track-atlas `road-america/gp` |
| Road Atlanta | Peachtree Run | Full course, clockwise | track-atlas `road-atlanta/gp` |
| Virginia International Raceway | Oak Tree Valley | Full course, clockwise | track-atlas `virginia-international-raceway/gp` |

Baltimore is not included: its temporary 2012–2013 street layout needs a
separately verified historical trace. Do not substitute today's street network
or claim this collection reproduces every ALMS round.

## Sources and regeneration

Checked 27 September 2026. The [ACO's 2013 ALMS calendar announcement](https://www.24h-lemans.com/en/news/alms-announces-2013-schedule-9063)
identifies the venues and the replacement of Mid-Ohio by Austin.
[Honda's 2012 Mid-Ohio race report](https://hondaracing-us.honda.com/Articles/General/HPD-Mid-Ohio-Challenge-Race-Report)
and [Michelin's report](https://michelinmedia.com/michelin-shines-sunny-hot-mid-ohio/)
confirm the 2.258-mile layout; the [operator](https://www.midohio.com/info-m)
distinguishes its 13-turn and 15-turn courses. The start/finish anchor is on
the pit straight, checked against the [operator's facility map](https://www.midohio.com/images/Files/midohio-20_v3.pdf).

Seven traces come from [tobi/track-atlas at c783def5a24df](https://github.com/tobi/track-atlas/tree/c783def5a24df8ce8f034262e8228c262b854e0e).
Their upstream files were checked byte-for-byte against that commit. Each
GeoJSON in [traces](traces) records its source path, SHA-256, direction and
start/finish anchor, and contains baked Open-Meteo elevation samples.

Lime Rock’s dense upstream trace contains metre-long survey steps that look
like extra corners. Its pinned plan is simplified with a 0.5-metre
Ramer–Douglas–Peucker tolerance at full size; retained fixes keep their sampled
elevations. This removes survey noise while preserving the classic course.

Mid-Ohio joins OSM ways 444205375, 1315957515, 1315957514, 1315957513,
1315957512 and 1315957511 in that order. It excludes the chicane (444205376),
practice cut-throughs and pit lane. The exact ways, node coordinates and object
versions are in [mid-ohio-source.osm](traces/mid-ohio-source.osm). Its `revision`
is the SHA-1 of that pinned extract; its SHA-256 and object versions are also
recorded in the GeoJSON. Linear subdivision to at most 20 metres preserves
the surveyed plan while allowing an accurate start-line anchor.

Regenerate with the existing importer, for example:

```sh
python3 tools/make_track.py docs/track-screening/traces/road-atlanta.geojson road_atlanta "Peachtree Run"
```

The importer initializes lap length and scale. Restore the documented scale
exception, measure the finished lap with the `the_laps` ignored test, and run
the geometry, driving, camera, scenery and medal checks before admission.

Long Beach requires a plan multiplier of 2.2: 2.1 and below cannot fit the
road and minimum shoulder between its closely spaced streets. The road width
and physics stay common to all circuits. Other additions use the shared scale.

[Geometry licenses and source offer](../../assets/tracks/CREDITS.md) ship with
the game. Elevation is a terrain model; it does not reproduce individual bumps.

## Menu collections

Collections overlap and filter the catalogue without duplicating circuits:

- **All:** every circuit, alphabetically by Todora name.
- **DHH ’14:** the eight 2014 WEC venues in race order: Silverstone, Spa,
  Le Mans, Austin, Fuji, Shanghai, Bahrain and Interlagos. The final order is
  confirmed by the [FIA](https://www.fia.com/events/world-endurance-championship/season-2014/calendar)
  and [ACO](https://www.24h-lemans.com/en/news/final-changes-in-the-calendar-of-the-2014-fia-wec-13870).
- **Endurance:** WEC venues plus these North American endurance additions.
- **Grand Prix:** the existing Grand Prix-inspired catalogue, including Fuji.
- **Heritage:** a curated selection of classic Grand Prix venues. This label
  describes their racing history, not a promise of period-correct layouts.

Click a collection, use `[` / `]`, or use the controller's LT / RT triggers.
Search works inside the selected collection, including existing real-venue
IDs and common abbreviations such as COTA, CTMP and VIR. Switching collections
keeps the selected circuit when it still matches. Only Drive applies a choice.

Circuit IDs, existing surface fingerprints, saved laps and the original
40-circuit weekly rotation remain unchanged. The 40-circuit achievement
thresholds also stay unchanged. New medal targets are provisional AI targets.

## Admission checks

All 51 circuits must pass the shared shape, shoulder, driving, scenery and
camera checks. The lap-length sanity range now includes the short Lime Rock
course. Six new hilly venues use the existing camera clearance adjustment;
the same limit of less than one sixth of a lap remains enforced.

Mid-Ohio exposed a grandstand placement bug: clearance was checked along the
curved road offset, while the building uses straight modules. The checks now
follow the actual building footprint, include the roof overhang and check the
shared end against the road without claiming a neighbour's space twice.

Local validation on macOS: 333 game tests and 15 server tests passed (12
authoring/diagnostic tests ignored), all eight Python tooling tests passed,
and workspace Clippy passed
with all targets/features and warnings denied. Existing 43-track replay
fixtures and medal entries were checked unchanged. The menu launch and a
Mid-Ohio switch both exited successfully; loading remained responsive for 20
frames. Metal screenshot capture returns black images in this environment,
so visual review remains pending the Linux CI captures. The Linux render
checks now include all eight additions and the Endurance collection.
