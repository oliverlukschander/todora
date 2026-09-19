# Aston Martin V8 Vantage GTE #95

David Heinemeier Hansson, Kristian Poulsen and Nicki Thiim won the **LM GTE Am class at the 2014 24 Hours of Le Mans** in Aston Martin Racing / Young Driver AMR's #95 V8 Vantage GTE. This was a class victory; Audi won the race overall.

The default **VANTAGE #95** garage choice recreates its pale Gulf blue and orange appearance. Clubman and Express use the same model with their existing handling and alternative body colours. Orange stripes, numbers, lettering, carbon parts and lights keep their authored materials when the body colour changes.

## Sources

Checked 19 September 2026:

- [ACO: Le Mans 2014 — LM GTE Am class winner reactions](https://www.24h-lemans.com/en/news/le-mans-2014-lm-gte-am-class-winner-reactions-16109), 15 June 2014. Names the three winning drivers and the #95 Aston Martin Vantage GTE.
- [DHH's personal site](https://dhh.dk/). Identifies him as Omarchy's creator and confirms his 2014 class win with Aston Martin.
- [2014 grid photograph in DHH's Evrone interview](https://evrone.com/sites/default/files/upload_ck/cases/dhh/amr2014-2.jpg). Front and side proportions, orange grille outline, number board, yellow auxiliary lights, Gulf colours, Basecamp lettering and wing.
- [2014 race photograph](https://img.huffingtonpost.com/asset/5d0223b5210000dc18edd730.jpeg?ops=1778_1000). Side profile, wheel arches, fastback glazing, race number and rear wing.

Reference photographs are not bundled with the game. The mesh and lettering are original Blender geometry, modelled from those references. This is a stylized game recreation, not a factory CAD model; small sponsor lettering is simplified.

## Assets and rebuilding

- Editable scene: `art/models/vantage_gte_95.blend`
- Game asset: `assets/models/vantage_gte_95.glb`
- Studio preview: `art/models/vantage_gte_95.png`
- Generator: `tools/make_vantage_gte.py`

```sh
/Applications/Blender.app/Contents/MacOS/Blender --background --python tools/make_vantage_gte.py
```

The generator reuses mesh helpers from `make_shooting_brake.py`, creates the body and running gear, fits glass and livery to the surfaces, exports only the car, saves the editable scene and renders front, rear and side views. It needs no downloaded textures or fonts.

Blender uses +Y forward and +Z up; glTF export maps forward to Bevy's −Z. The four animated parents are `WheelFL`, `WheelFR`, `WheelRL` and `WheelRR`. Their centres match `src/car/physics.rs`: ±0.50 across, +0.70 at the front and −0.76 at the rear, with a 0.20 wheel radius. The whole car uses the game's 0.405 scale: about 1.06 game units long, 0.45 across the painted body and 0.52 at its widest including protruding details. Player, ghost, wheel contacts and wheelbase share this scale. Steering lock preserves the previous low-speed turning radius.

These proportions suit the 2.94-unit asphalt width (3.60 including lines and kerbs). Track lengths and road widths are scaled independently, so the car is sized for the road width rather than a uniform geographic scale.

The GTE handling doubles the previous estate car's aerodynamic grip contribution. Base tyre grip stays at 14 m/s²; at 20 m/s, the available lateral acceleration rises from 17.6 to 21.2 m/s² (about 20% more). Clubman keeps 9% more aero and Express 11% less than the default Vantage. This is arcade tuning inspired by the real car's splitter, floor and adjustable rear wing, not a measured aerodynamic specification. See the [Aston Martin Racing brochure](https://astonmartins.com/wp-content/uploads/2013/04/Vantage-GTE-Brochure.pdf).

Only the body material is named `Paint`, which is the runtime recolouring contract. Static body details are combined into one mesh; each rotating wheel has its own mesh. Lights, cameras and the studio floor remain in the Blender scene and are excluded from the game export.
