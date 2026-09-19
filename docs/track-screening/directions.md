# Racing direction audit — 19 September 2026

All 40 shipped centrelines were compared with [F1DB circuit records](https://github.com/f1db/f1db/tree/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits), pinned to revision `1e0008211a84f94f14acc6eb1eb5d2694861867d`. **Paul Ricard and Marina Bay were reversed.** The other 38 already matched. Each row links its reference; directions describe the layouts represented by the game's pinned GPS traces.

The importer had treated GeoJSON point order as racing order. It now normalizes that order against the verified `direction` in `sources.json`. Reversing keeps the first fix at the recorded start/finish line and keeps each elevation attached to its GPS fix. The grid, lap gates and progress follow that corrected traversal. Existing surface fingerprints reject ghosts from the two old reversed tracks without deleting other records.

The check uses signed area in the game's X/Z plane: X is east, Z south, so positive area is clockwise when viewed from above. This is a direction check for a known layout, not a way to discover its real racing direction. **Suzuka is a figure eight:** its lobes turn opposite ways; its existing racing order and start approach are retained. Its net area happens to be positive, so it can be regression-checked against this pinned layout, but a different figure-eight trace requires checking its start approach rather than inferring direction from area alone.

Layout-specific checks:

- **Buenos Aires:** the represented No. 6/Senna-S layout runs clockwise. The anticlockwise 1954 race used a different configuration. See [RacingCircuits.info's layout history](https://www.racingcircuits.info/south-america/argentina/buenos-aires.html).
- **Kyalami:** the current 4.522 km layout runs anticlockwise, as described by the [circuit operator](https://www.kyalamigrandprixcircuit.com/grand-prix-circuit/). The old clockwise layout is not the one in the game.
- **Marina Bay:** the corrected anticlockwise order agrees with [Pitpass's circuit reference](https://www.pitpass.com/circuits/18/singapore).
- **Paul Ricard:** the corrected clockwise order also agrees with the [Formula One circuit catalogue](https://en.wikipedia.org/wiki/List_of_Formula_One_circuits).

| Circuit id | Circuit / source | Direction | Change in 0.8.0 |
|---|---|---|---|
| albert-park | [Albert Park](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/melbourne.yml) | Clockwise | Already correct |
| algarve | [Algarve](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/portimao.yml) | Clockwise | Already correct |
| bahrain | [Bahrain](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/bahrain.yml) | Clockwise | Already correct |
| baku | [Baku](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/baku.yml) | Anticlockwise | Already correct |
| barcelona-catalunya | [Barcelona-Catalunya](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/catalunya.yml) | Clockwise | Already correct |
| buenos-aires | [Buenos Aires](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/buenos-aires.yml) | Clockwise | Already correct |
| americas | [Circuit of the Americas](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/austin.yml) | Anticlockwise | Already correct |
| estoril | [Estoril](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/estoril.yml) | Clockwise | Already correct |
| gilles-villeneuve | [Gilles-Villeneuve](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/montreal.yml) | Clockwise | Already correct |
| hermanos-rodriguez | [Hermanos Rodríguez](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/mexico-city.yml) | Clockwise | Already correct |
| hockenheim | [Hockenheim](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/hockenheimring.yml) | Clockwise | Already correct |
| hungaroring | [Hungaroring](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/hungaroring.yml) | Clockwise | Already correct |
| imola | [Imola](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/imola.yml) | Anticlockwise | Already correct |
| indianapolis | [Indianapolis](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/indianapolis.yml) | Clockwise | Already correct |
| interlagos | [Interlagos](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/interlagos.yml) | Anticlockwise | Already correct |
| istanbul-park | [Istanbul Park](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/istanbul.yml) | Anticlockwise | Already correct |
| jacarepagua | [Jacarepaguá](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/jacarepagua.yml) | Anticlockwise | Already correct |
| jeddah | [Jeddah](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/jeddah.yml) | Anticlockwise | Already correct |
| kyalami | [Kyalami](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/kyalami.yml) | Anticlockwise | Already correct |
| las-vegas | [Las Vegas](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/las-vegas.yml) | Anticlockwise | Already correct |
| losail | [Losail](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/lusail.yml) | Clockwise | Already correct |
| madring | [Madring](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/madring.yml) | Clockwise | Already correct |
| magny-cours | [Magny-Cours](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/magny-cours.yml) | Clockwise | Already correct |
| marina-bay | [Marina Bay](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/marina-bay.yml) | Anticlockwise | Reversed |
| miami | [Miami](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/miami.yml) | Anticlockwise | Already correct |
| monaco | [Monaco](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/monaco.yml) | Clockwise | Already correct |
| monza | [Monza](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/monza.yml) | Clockwise | Already correct |
| mugello | [Mugello](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/mugello.yml) | Clockwise | Already correct |
| nurburgring | [Nürburgring](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/nurburgring.yml) | Clockwise | Already correct |
| paul-ricard | [Paul Ricard](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/paul-ricard.yml) | Clockwise | Reversed |
| red-bull-ring | [Red Bull Ring](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/spielberg.yml) | Clockwise | Already correct |
| sepang | [Sepang](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/sepang.yml) | Clockwise | Already correct |
| shanghai | [Shanghai](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/shanghai.yml) | Clockwise | Already correct |
| silverstone | [Silverstone](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/silverstone.yml) | Clockwise | Already correct |
| sochi | [Sochi](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/sochi.yml) | Clockwise | Already correct |
| spa-francorchamps | [Spa-Francorchamps](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/spa-francorchamps.yml) | Clockwise | Already correct |
| suzuka | [Suzuka](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/suzuka.yml) | Clockwise (figure eight) | Already correct |
| watkins-glen | [Watkins Glen](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/watkins-glen.yml) | Clockwise | Already correct |
| yas-marina | [Yas Marina](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/yas-marina.yml) | Anticlockwise | Already correct |
| zandvoort | [Zandvoort](https://raw.githubusercontent.com/f1db/f1db/1e0008211a84f94f14acc6eb1eb5d2694861867d/src/data/circuits/zandvoort.yml) | Clockwise | Already correct |

Run `python3 -m unittest discover -s tools -p 'test_*.py'` to check importer orientation, anchor preservation and all 40 baked source traces. `cargo test --locked` also checks the finished ribbon direction and grid heading against this table, alongside road geometry and complete driving/lap tests. These checks use local data and do not call circuit or elevation services.
