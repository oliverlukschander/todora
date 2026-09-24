# Omarchy GT #95

Todora's default car is an invented front-engined endurance GT: the **OMARCHY GT #95**. It belongs to no maker and copies no real car. It has a wedge nose with twin hexagonal intakes and four round endurance lamps, slim blade headlamps, a flat waist, a squared-off tail with a full-width light bar, a cambered rear wing and a carbon diffuser.

The livery is the pale blue and orange DHH raced in at Le Mans in 2014, so he can tell it is his: an orange bonnet stripe, sunstrip, window surrounds and sill pinstripes, and a white number board with an orange field and a white **95**. **OMARCHY RACING** runs across the wing, **OMARCHY** is on the doors, windscreen and tail, and the rear number plate reads **DHH**. A small Danish flag sits at the back of the roof. Only the colours carry over: no maker's, sponsor's or series' name, logo or roundel is on it, and all the lettering is Todora's own.

Clubman and Express use the same model with their own handling and body colours. The accent, lettering, carbon parts and lights keep their authored materials when the body colour changes.

## Assets and rebuilding

- Editable scene: `art/models/omarchy_gt_95.blend`
- Game asset: `assets/models/omarchy_gt_95.glb`
- Studio preview: `art/models/omarchy_gt_95.png`
- Generator: `tools/make_omarchy_gt.py`

```sh
/Applications/Blender.app/Contents/MacOS/Blender --background --python tools/make_omarchy_gt.py
```

The generator reuses mesh helpers from `make_shooting_brake.py`, creates the body and running gear, fits glass and livery to the surfaces, exports only the car, saves the editable scene and renders front, rear and side views. It needs no downloaded textures or fonts.

Blender uses +Y forward and +Z up; glTF export maps forward to Bevy's −Z. The four animated parents are `WheelFL`, `WheelFR`, `WheelRL` and `WheelRR`. Their centres match `src/car/physics.rs`: ±0.50 across, +0.70 at the front and −0.76 at the rear, with a 0.20 wheel radius. The whole car uses the game's 0.405 scale: about 1.06 game units long, 0.45 across the painted body and 0.52 at its widest including protruding details. Player, ghost, wheel contacts and wheelbase share this scale. Steering lock preserves the previous low-speed turning radius.

These proportions suit the 2.94-unit asphalt width (3.60 including lines and kerbs). Track lengths and road widths are scaled independently, so the car is sized for the road width rather than a uniform geographic scale.

The GT handling doubles the previous estate car's aerodynamic grip contribution. Base tyre grip stays at 14 m/s²; at 20 m/s, the available lateral acceleration rises from 17.6 to 21.2 m/s² (about 20% more). Clubman keeps 9% more aero and Express 11% less than the default GT. This is arcade tuning, not a measured aerodynamic specification.

Only the body material is named `Paint`, which is the runtime recolouring contract. Static body details are combined into one mesh; each rotating wheel has its own mesh. Lights, cameras and the studio floor remain in the Blender scene and are excluded from the game export.
