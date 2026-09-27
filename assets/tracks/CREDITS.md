# Endurance circuit geometry

Sarthe Run, Volcano Straight, Orange Grove Airfield, Corkscrew Coast,
Connecticut Valley, Pacific Harbour, Maple Forest, Elkhart Lakes, Peachtree Run
and Oak Tree Valley use geometry derived from © OpenStreetMap contributors,
via `tobi/track-atlas` at commit
`c783def5a24df8ce8f034262e8228c262b854e0e`.

- OpenStreetMap: https://www.openstreetmap.org/copyright
- Geometry database license: Open Database License (ODbL) 1.0,
  https://opendatacommons.org/licenses/odbl/1-0/
- Source snapshot: https://github.com/tobi/track-atlas/tree/c783def5a24df8ce8f034262e8228c262b854e0e
- Todora's derived geometry and baked elevation data are available in
  `docs/track-screening/traces/` and the corresponding modules in `src/track/circuits/`
  at https://github.com/oliverlukschander/todora under ODbL 1.0.

Lexington Hills uses OpenStreetMap geometry directly. Its pinned ways and nodes
are included in `docs/track-screening/traces/mid-ohio-source.osm`; the same ODbL
license and source offer apply.

Elevation samples: Open-Meteo Elevation API, based on Copernicus DEM GLO-90.
API data attribution: https://open-meteo.com/ (CC BY 4.0),
https://creativecommons.org/licenses/by/4.0/.
Coordinates are projected into local metres and heights are relative to the
lowest sample. Todora smooths and scales these data for its arcade roads.

Only the geometry is imported from track-atlas, not its corner-name or
simulator-derived metadata. These data licenses do not replace the MIT
license on Todora's game code.
