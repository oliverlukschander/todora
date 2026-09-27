# Licensing and credits

Todora's original code, documentation and original artwork are available under
the [MIT license](LICENSE), copyright © 2026 Oliver Lukschander. You may fork,
modify and redistribute them, including commercially, while retaining the
copyright and license notice.

Third-party material keeps its own terms. The MIT license does not replace the
licenses below, including where imported track data is embedded in Rust files.

| Material | License and attribution |
| --- | --- |
| Rust dependencies | [Full license texts, notices and exact-version source links](THIRD_PARTY_NOTICES.html). Includes Symphonia under MPL 2.0. |
| Engine and tyre recordings, including edited loops and source recordings in `art/audio/` | CC BY 3.0; [authors, sources and modifications](assets/audio/CREDITS.md). |
| OpenStreetMap-derived circuit geometry, traces and baked data | ODbL 1.0; [attribution and corresponding data sources](assets/tracks/CREDITS.md). |
| Open-Meteo elevation samples | CC BY 4.0; [sources and modifications](assets/tracks/CREDITS.md). |
| Figtree font | SIL OFL 1.1; [copyright and license](assets/fonts/OFL.txt). |
| Country flags from flag-icons | MIT, copyright Panayiotis Lipiridis; [license](assets/ui/flags/LICENSE) and [source revision](assets/ui/flags/CREDITS.md). |

Omarchy and other third-party names and marks belong to their respective owners;
Todora's MIT grant does not grant rights to those marks or imply endorsement.
Radio music is streamed separately and is not included in the game distribution.

### Additional bundled-code attribution

`ring` 0.17.14 includes fiat-crypto code under Apache 2.0. Its short attribution
header is reproduced here because the generator lists full license texts rather
than this header. The corresponding `AUTHORS` and source files are in the
[ring 0.17.14 source archive](https://crates.io/api/v1/crates/ring/0.17.14/download).

```text
The Apache License, Version 2.0 (Apache-2.0)

Copyright 2015-2020 the fiat-crypto authors (see the AUTHORS file)

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

## Redistributing a build

Keep `LICENSE`, this file, `THIRD_PARTY_NOTICES.html`, and the credits and license
files under `assets/` with the application. The packaging scripts copy them into
the Linux application directory or the Mac app's `Contents/Resources` directory.
The server image includes them under `/usr/share/doc/todora`.

The MPL-covered Symphonia source is available from the exact-version source
archive links in the dependency notices. If you modify those dependencies,
make the corresponding MPL-covered source available and update the notices.
The circuit credit file explains how to obtain the ODbL-covered data and its
derivatives. Keep those licenses and attribution when redistributing that data.

## Updating dependency notices

Install the pinned notice generator once:

```sh
cargo install cargo-about --version 0.9.2 --locked --features cli
```

After changing dependencies, run:

```sh
cargo about generate --locked --workspace --all-features --fail tools/licenses.hbs -o THIRD_PARTY_NOTICES.html
```

Review the output and any warnings, then commit the updated notices with
`Cargo.lock`. `about.toml` covers Apple Silicon Macs, x86-64 Linux and the ARM64
Linux server. CI regenerates the notices and checks that they are current.
Review the additional bundled-code attribution above when updating `ring`.
Asset credits are maintained separately in the files listed above.
