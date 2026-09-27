# Audio credits

## Engine

“ferrari355underhood4.mp3” by enginemusic, recorded in 1997 and published
4 November 2007. Licensed under Creative Commons Attribution 3.0 Unported.

Source: https://freesound.org/people/enginemusic/sounds/43484/
License: https://creativecommons.org/licenses/by/3.0/
Retrieved: 27 September 2026, using the publicly available high-quality MP3
preview: https://cdn.freesound.org/previews/43/43484_462458-hq.mp3

The downloaded preview is preserved as `art/audio/engine-source.mp3` (SHA-256
`8256a64d2e6ac8160edbcc4191a7fc5e13614e556e4493eaf2b47b12b0a94412`).
It is a Ferrari 355 Spider engine-compartment recording; it does not represent
the fictional OMARCHY GT #95's real-world counterpart or imply endorsement.

Todora changes: mono 44.1 kHz conversion, high/low-pass filtering, three short
excerpts, firing-cycle retiming to remove the recorded acceleration, DC
removal, level matching and crossfading over whole firing cycles. All three
loops use 200 samples per cycle; runtime playback keeps their firing phase
aligned. Pitch, blending, throttle filtering and gain make the recording
interactive. These are excerpts of one acceleration, not separately measured
idle/low/high-RPM takes. The three `engine-*.s16le` files are headerless signed
16-bit little-endian PCM. Rebuild them and their WAV previews with
`python3 tools/prepare_engine_audio.py` (requires ffmpeg).

## Tyres

“Car tire squeal skid loop” by audible-edge (Tom Haigh), loop edited by
qubodup. Licensed under Creative Commons Attribution 3.0 Unported.

Source: https://opengameart.org/content/car-tire-squeal-skid-loop
License: https://creativecommons.org/licenses/by/3.0/
Retrieved: 19 September 2026.

Todora changes: converted to mono 44.1 kHz signed 16-bit little-endian PCM,
removed DC offset, normalized and crossfaded the loop boundary. Runtime pitch,
filtering and volume follow rolling speed, tyre load and slide. A second,
low-pass-filtered layer provides continuous rolling contact. `tyres.s16le` is headerless PCM.
The source WAV is in `art/audio/tyres-source.wav`; run
`python3 tools/prepare_tyre_audio.py` on macOS to rebuild it and the WAV preview.

Omarchy radio music remains a live stream and is not bundled here.
