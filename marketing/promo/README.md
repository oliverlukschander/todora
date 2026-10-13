# Todora 0.12.0 — README kit

For someone meeting Todora on GitHub for the first time. The invitation is to download it and drive a lap.

The README uses game-rendered pictures from this checkout, including the 51-circuit browser, driver profiles, four-player split-screen and Bonkers. The capture code comes from `promo/launch-kit`, adapted to 0.12.0; no old gameplay exports are used. See [capture provenance and checks](CAPTURE.md).

## Deliverables

- [`../../README.md`](../../README.md): the game page.
- [`../../docs/images/`](../../docs/images/): compressed 1920×1080 screenshots and a compact 1280×720 silent gameplay clip.
- `exports/video/todora-0.12-gameplay-16x9.mp4`: local 1920×1080 master, excluded from git.
- [`../../docs/design-history.md`](../../docs/design-history.md): the previous README, preserved with working relative links and a historical-context note.
- `shots/readme.json`: reproducible camera, circuit, timing and menu choices.
- `shots/edit.json`: reviewed screenshot positions and trailer cuts.

The clip uses real rendered motion at 60 frames per second. It is H.264 High, yuv420p, BT.709, with `+faststart`, matching the previous kit's delivery format. It has no audio. The live Omarchy radio is disabled during all captures.

## Look

The game's own Figtree typography and HUD supply the visual language: ink `#0B1115`, paper `#E8F0F2`, muted `#91A3AD`, lime `#C4F55C`, purple `#BD6BFF`. `graphics/src/kit.css` preserves the shared stylesheet from the launch kit for future cards. Pictures show the actual game; screenshots do not have fabricated UI or added scenery.

## Rebuild

From the repository root, with Rust and ffmpeg installed:

```sh
python3 marketing/promo/tools/capture.py --build marketing/promo/shots/readme.json
python3 marketing/promo/tools/check_hud.py
python3 marketing/promo/tools/export.py
```

A capture starts from an empty save folder. Before repeating an existing shot, move its directory under `.cache/data/` aside; the tool refuses to overwrite those saves. Capture `finish` before `ghost`: the latter copies the former's actual saved lap. Intermediate footage and logs stay under `.cache/`, ignored by git. No uncompressed frame sequences are retained.

The opt-in `promo` feature disables Winit and renders to a Bevy image using Metal. The clock advances by exactly 1/60 second per frame. An unshown virtual window supplies layout dimensions for split-screen. `TODORA_PROMO_DATA` isolates captures from the player's saves, and `ReadOnly` disables the online client, outbox writes, settings and achievement persistence. Local ghost recordings stay in the isolated directory.

## Release handoff

The README links to the checked-in short clip, so it has no dependency on a future upload. GitHub may show a download instead of an inline player. For a prominent release trailer, Oliver can attach `marketing/promo/exports/video/todora-0.12-gameplay-16x9.mp4` to release **v0.12.0** under a new name, **`todora-0.12-gameplay-16x9.mp4`**, and change both README video links to that asset's URL. Keep the existing `todora-30s-16x9.mp4`: the release notes identify it as older footage. The kit scripts do not upload assets or edit releases.

## Credits

Original game artwork and footage: Todora, MIT. Figtree: SIL OFL 1.1. Circuit geometry and elevation retain their source licenses; see [circuit credits](../../assets/tracks/CREDITS.md) and [all licenses](../../LICENSES.md). The silent clip includes no music or audio samples. For future cuts with game sound, retain the attribution in [audio credits](../../assets/audio/CREDITS.md). Todora is not affiliated with any circuit, racing series, car manufacturer or Omarchy.
