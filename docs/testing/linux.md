# Linux verification

Linux remains supported alongside macOS. Its single-player game uses the same
physics, circuits, mini-map, sector timing, lap validity and ghosts. The native
Game Center transport is macOS-only; `--features game-center` must compile on
Linux without Apple frameworks or exposing multiplayer controls. The separate
`multiplayer-test` feature enables a development-only loopback transport.
Windows is optional and untested.

## Build dependencies

Use Rust 1.97.1 (the CI toolchain) and the committed Cargo.lock. On Debian/Ubuntu:

```sh
sudo apt-get install build-essential pkg-config libasound2-dev libudev-dev \
  libx11-dev libxi6 libxcursor1 libxrandr2 libxkbcommon-x11-0 \
  libwayland-dev libxkbcommon-dev librsvg2-bin
tools/package_linux.sh --install
```

Install the Vulkan driver appropriate for the GPU. See
[Bevy's Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)
for other distributions, including Arch/Omarchy. The package script accepts
either rustup or distribution-provided Cargo, respects `CARGO_TARGET_DIR` and
`XDG_DATA_HOME`, and requires a Linux host or container.

## Automated checks

The [Linux workflow](../../.github/workflows/linux.yml) runs on Ubuntu 24.04
x86-64 for pushes and pull requests. It checks formatting and all-feature
Clippy across the workspace, runs the game and server test suites (including
timing, track limits and ghost persistence), runs the Python tooling tests, and
tests that the Apple feature remains inert on Linux.
It also builds and installs the release package into a path containing spaces,
validates desktop entries and shared libraries, and launches it outside the
checkout to check asset loading.

Rendering uses the existing `visual-check` feature, Xvfb and Mesa's software
Vulkan driver. Suzuka, Monza and Monaco screenshots and logs are retained as
the `linux-check` workflow artifact. Blank captures, application errors and
timeouts fail the check. Install `python3-pil` for screenshot validation. To
reproduce on an X11 desktop:

```sh
cargo build --locked --features visual-check
bash tools/check_linux.sh
```

For a headless machine, install `xvfb xauth mesa-vulkan-drivers python3-pil`, then:

```sh
WGPU_BACKEND=vulkan xvfb-run -a -s '-screen 0 800x600x24' bash tools/check_linux.sh
```

On 2026-09-21, the [Ubuntu x86-64 verification run](https://github.com/oliverlukschander/todora/actions/runs/35586095153)
passed all 206 tests, the Apple-feature regression check, Clippy, all three
captures, and release packaging/install/startup checks. The screenshots were
visually reviewed at 800×600: the mini-map and HUD fit, the rubber line remains
subtle, and Suzuka's bridge renders correctly. The driver was Mesa 25.2.8
llvmpipe with Vulkan.

The screenshots are a rendering smoke test, not a frame-rate benchmark. Real
Linux hardware still needs a driving pass to verify GPU performance, audible
sound, gamepad hot-plugging and the desktop's Wayland/window-manager behavior.
These checks do not verify Game Center Internet matchmaking.

The local Debian 12 x86-64 container on Apple Silicon passed all 206 tests on
2026-09-21, but its Mesa 22.3.6 software renderer timed out on both X11 and
Wayland. Treat the emulated container as a build/test environment; its graphics results do not
establish Linux desktop performance or native Wayland compatibility.

For visual review on Apple Silicon, prefer a native `linux/arm64` container.
On 2026-09-27, Debian 12 with Mesa 22.3.6 captured the profile, leaderboard,
local lobby and split-screen layouts with `TODORA_SETTINGS='{"antialiasing":false}'`.
With anti-aliasing enabled, the leaderboard and lobby stalled inside llvmpipe;
disabling it completed the same captures without changing the game code.
This is a software-renderer workaround, not a recommended desktop setting.
For example, after building with `--features visual-check`:

```sh
BEVY_ASSET_ROOT="$PWD" WGPU_BACKEND=vulkan \
  TODORA_SETTINGS='{"antialiasing":false}' TODORA_SMALL=1 \
  TODORA_SCREEN=local TODORA_PLAYERS=4 TODORA_CIRCUIT=road-atlanta \
  TODORA_CAPTURE=/tmp/todora-local-4.png \
  xvfb-run -a -s '-screen 0 800x600x24' "${CARGO_TARGET_DIR:-target}/debug/todora"
```

Use `TODORA_SCREEN=board`, `profile`, `local-lobby`, or `local-cycle` for the
other screens. `TODORA_EXAMPLE=21` supplies leaderboard fixtures; `local-cycle`
asserts that returning to solo play removes every local car and camera.
Keep Cargo target directories or volumes separate between ARM and x86 builds.
