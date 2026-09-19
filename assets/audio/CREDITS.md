# Tyre audio

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
