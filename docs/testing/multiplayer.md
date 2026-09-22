# Game Center shared-practice prototype

Two drivers can share a circuit, see each other's cars, and compare valid lap
times. Each car keeps Todora's existing local physics. There are no car-to-car
collisions, race standings or ranked results in this prototype.

## Build and play

```sh
bash tools/package_game_center_macos.sh
open "dist/Todora Multiplayer.app"
```

This builds a separate development app and DMG with the `game-center` feature.
Without signing variables it uses an ad hoc signature for local launch checks;
live Game Center needs the Apple setup below. Ordinary builds and releases do
not include the multiplayer UI or native GameKit bridge.

Choose a circuit, car and driving mode before opening **Multiplayer** with **M**,
the button, or the controller's **right-stick click (R3)**. Open multiplayer on
both Macs, sign in with different Game Center accounts, then use Apple's friend
invitation or matchmaking interface. Both copies must use the same prototype
version and circuit data.

The coordinator is chosen consistently from the two Game Center player IDs;
its selected circuit and driving mode are used by both players. The other Mac
loads that selection automatically. A three-second countdown follows readiness
and a clock-offset exchange. Cars start on separate sides of the grid. The lap
clock still starts at the start/finish line, as it does in solo driving.

The live driver's car is cyan; the saved ghost stays amber. The practice panel
shows valid lap counts and each driver's best valid time in this session.
**R** restarts only your car. Circuit and garage changes are unavailable until
you leave the session. **Esc** opens the local pause menu and releases the
controls; the car coasts and the session continues. The interrupted lap is
invalid. **M / R3** leaves and returns to solo driving.

## Apple account setup

Use the developer's own enrolled team. Do not reuse a certificate from an
unrelated organization's team.

1. Wait for Apple Developer Program enrollment to become active. Add that Apple
   Account in **Xcode → Settings → Accounts**, select its team, and create an
   **Apple Development** certificate through **Manage Certificates**.
2. Register the explicit App ID **com.lukschander.todora** with the **Game Center**
   capability in [Certificates, Identifiers & Profiles](https://developer.apple.com/account/resources/identifiers/list).
3. Create a **macOS** app named **Todora** in [App Store Connect](https://appstoreconnect.apple.com/),
   using that bundle ID, English as the primary language, and a unique SKU such
   as `todora-macos`. Enable Game Center for the app version and configure
   multiplayer compatibility for the versions being tested.
4. Register the two development Macs and create/download a **Mac App Development**
   provisioning profile for this App ID, certificate and those Macs. No release
   submission is needed for development testing.
5. Build with the downloaded profile and your own signing identity:

   ```sh
   TODORA_GAME_CENTER_PROFILE="/absolute/path/Todora.provisionprofile" \
   TODORA_SIGN_IDENTITY="Apple Development: your certificate name" \
   bash tools/package_game_center_macos.sh
   ```

The signing helper rejects expired profiles, other platforms, wildcard or
unrelated App IDs, missing Game Center capability, and certificates not
authorized by the profile. It embeds the profile and enables the sandbox,
Game Center, incoming/outgoing networking and USB controller access. Game
Center builds use Foundation's Application Support directory so saved ghosts
work inside the app sandbox; sandboxed and ordinary builds may have separate
local records. Do not commit certificates or provisioning profiles.

## Apple setup completed on 2026-09-22

The developer's personal team is **Oliver Lukschander — BAADW73W4C**.
The App ID is `com.lukschander.todora`, with Game Center enabled. The
[App Store Connect record](https://appstoreconnect.apple.com/apps/6814738694/distribution/macos/version/inflight)
is named **Todora Demo** (Apple ID `6814738694`, SKU `todora-macos`). Its
development version and multiplayer compatibility are set to `0.10.0`, matching
the packaged app. Nothing has been submitted for review or released.

The **Todora Mac Development** profile covers Oliver MacBook Pro and the
previously registered Oliver’s MacBook Air. It expires on 2027-09-22. The local
download is `~/Downloads/Todora_Mac_Development.provisionprofile`; keep the
profile and certificates out of Git. Rebuild on this Mac with:

```sh
TODORA_GAME_CENTER_PROFILE="$HOME/Downloads/Todora_Mac_Development.provisionprofile" \
TODORA_SIGN_IDENTITY="8C013A41B047FBF3DB481F8CFA0FD67041D4A220" \
bash tools/package_game_center_macos.sh
```

That fingerprint selects the installed personal-team certificate authorized by
this profile. Regenerate the profile and update the identity when renewing it
or adding another test Mac. A live match between two Macs remains to be tested.

The development-signed `dist/Todora Multiplayer.app` and DMG were rebuilt.
Strict signature verification, the embedded profile and Game Center/sandbox
entitlements passed validation. The signed app launched with Metal and bundled
assets on the MacBook Pro. Game Center sign-in and live matchmaking still need
an interactive check with M and a second Mac using a different Game Center
account.

Signing from the agent's background session reported `errSecInternalComponent`
and a locked login keychain. Running the same packaging command in an
interactive Terminal, with the login keychain unlocked, succeeded.

## Multiplayer beta preparation on 2026-09-22

The next test release is `v0.11.0-beta.1`, with app version `0.11.0`.
Both players must install this version because the protocol includes the app
version. Multiplayer uses **M** and music uses **N**; **M** still changes driving
mode inside the garage.

Before publishing the development-signed DMG, register the other tester’s Mac
and regenerate **Todora Mac Development** to include it. The current profile
only covers Oliver’s MacBook Pro and MacBook Air. For Apple Silicon, collect
**Provisioning UDID** from **System Information → Hardware**, not Hardware UUID.
A public download link does not let an unregistered Mac run this build.

[Apple’s registered-Mac distribution instructions](https://help.apple.com/xcode/mac/current/en.lproj/dev295cc0fae.html)
and [device registration guidance](https://developer.apple.com/help/account/devices/register-a-single-device)
were checked on 2026-09-22. Use a GitHub prerelease for this test build and retain
0.10.0 as the stable release. See the [beta release notes](../releases/v0.11.0-beta.1.md).

## Local verification without Game Center credentials

The `multiplayer-test` feature provides a loopback-only TCP transport for two
processes on one Mac. It uses the same protocol and gameplay integration, but
does not test Apple authentication, invitations, Internet latency or GameKit's
unreliable delivery. This transport and the capture automation are excluded
from the packaged Game Center app.

```sh
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
python3 tools/test_sign_game_center.py
cargo build --features game-center,multiplayer-test
python3 tools/check_multiplayer.py suzuka monza spa-francorchamps
```

The rendered check deliberately starts the client on a different circuit,
verifies agreement and car movement, and saves reports and screenshots under
`dist/multiplayer-check/`. For manual loopback play, run these in two terminals,
then press M in each window:

```sh
TODORA_LOCAL_PEER=host cargo run --features multiplayer-test
TODORA_LOCAL_PEER=join cargo run --features multiplayer-test
```

## Protocol and remaining hardware checks

GameKit carries versioned, bounded JSON packets. Session controls use reliable
delivery; 30 Hz car snapshots use unreliable delivery. The receiver rejects
malformed poses, stale sequences and old reset epochs, interpolates with a
100 ms buffer, and limits extrapolation to 100 ms. Full 3D positions preserve
Suzuka's bridge elevation. A reset or recovery snaps instead of sweeping a car
across the circuit. Stale cars disappear after 1.5 seconds; a silent connection
ends after 15 seconds while driving. Matchmaking and loading have longer limits.

Remote cars never enter local physics or the lap/ghost recording queries.
Remote lap statistics are displayed as reported and never update local best
times, sector references or saved ghosts. This is a casual, peer-trusting mode;
authoritative race adjudication and collision prediction remain future work.

Before calling Game Center multiplayer verified, test two properly signed Macs
with distinct accounts: authentication, invitations, cancellation, automatic
matching, circuit changes during setup, countdown skew, controller input,
pause, restarts, invalid laps, valid finish records, disconnection/rejoining,
sleep/wake and packet loss across different Internet connections. No live
Game Center match or tvOS build has been verified yet.

## Verified locally on 2026-09-21

- The final full Rust suite passed (205 tests, 8 ignored), including 12
  multiplayer tests. These cover different clocks, version/geometry mismatches, packet
  validation and reordering, resets, stale peers, pause behavior, valid-lap
  accounting, and isolation from local records.
- Clippy passed with warnings denied for ordinary and all-feature builds.
- Two rendered processes agreed on the circuit after starting on different
  layouts and exchanged live movement on Suzuka, Monza and Spa-Francorchamps.
  Both cars and the HUD were visually checked at 800×600, including Suzuka's
  elevated road. Screenshots and reports are in `dist/multiplayer-check/`.
- The native bridge compiled and its main-queue error callback, bounded event
  polling and Application Support lookup passed a standalone macOS check.
- The signing helper's profile rejection checks passed. Live sign-in,
  invitations and development-profile signing await the Apple account setup.
- `dist/Todora Multiplayer.app` and its DMG were rebuilt. The app's ad hoc
  signature and plist passed validation, and it launched from outside the
  project directory with Metal and bundled assets. The packaged executable
  links GameKit and excludes the loopback transport and capture hooks.

The capture helper keeps its two windows visible and rejects all-black
captures; obscured macOS test windows can otherwise produce empty screenshots.

Apple references, checked 2026-09-21:
[real-time data exchange](https://developer.apple.com/documentation/gamekit/exchanging-data-between-players-in-real-time-games),
[multiplayer compatibility](https://developer.apple.com/help/app-store-connect/configure-game-center/add-multiplayer-compatibility),
[macOS matchmaking UI](https://developer.apple.com/documentation/gamekit/gkmatchmakerviewcontroller),
[Game Center entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.game-center).
