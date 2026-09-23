# TODO

## Polish plan

The approved plan, decisions and build order are in the
[Todora polish plan](https://claude.ai/code/artifact/fe56e4ae-9ea5-4bcf-ab4d-6a3111b5c44a).

- [x] 3-2-1-GO start lights with the car held on the grid (2026-09-23).
- [x] Drive the multiplayer countdown through the same start lights (2026-09-23).
- [x] Determinism spike: bit-identical replay on macOS arm64, Linux arm64 and
  Linux x86-64 for all 40 circuits (2026-09-23). Physics trig uses `libm`.
- [x] Pure lap judge (`LapTimer::judge`) shared with future replay checks (2026-09-23).
- [x] Sector colours and sector bar (2026-09-23).
- [x] Settings file: saved choices restored at launch (2026-09-23).
- [x] Settings page with audio, display, HUD and control rows wired in (2026-09-23).
- [x] Lap summary card and new-best celebration (2026-09-23).
- [x] Medals: author times (provisional where not yet driven), menu badges and summary line (2026-09-23).
- [ ] Drive author laps on all 40 circuits × 3 modes and rerun `write_the_author_times`.
- [x] First-run onboarding cards and the one-time Beginner hint (2026-09-23).
- [x] Run format, replay check and outbox for valid new bests (2026-09-24).
- [x] Leaderboard server: signed players, replay-verified runs, boards, ghosts, reports, delete-my-data (2026-09-24).
- [ ] Add the server to Linux CI (the agent's GitHub token lacks the `workflow`
  scope): after `cargo test --locked --all-targets` in `.github/workflows/linux.yml`,
  run `cargo clippy --locked -p todora-server --all-targets -- -D warnings` and
  `cargo test --locked -p todora-server`.
- [x] Game client: opt-in, signed uploads with backoff, Online settings tab, leaderboard overlay with world/country/rivals (2026-09-24).
- [x] Race a downloaded ghost beside your own, with its delta and world-record purple (2026-09-24).
- [x] Deploy script (tested against local Docker, amd64 and arm64), reverse-proxy
  examples and a privacy note (2026-09-24).
- [ ] Before going live: fill in the contact address in `docs/privacy.md`, point
  DNS for todora.lukschander.com at the Hetzner host, add the Caddy or nginx site,
  run `server/deploy/deploy.sh root@host` (ARCH=arm64 on CAX), and check
  `/v1/health` over HTTPS.
- [x] Records view: every circuit's best, medal and world place, with medal totals (2026-09-24).
- [x] Synthesised engine note with its own volume (2026-09-24). Listen to `dist/engine.wav` and tune.
- [ ] Online leaderboards at todora.lukschander.com.

## Game Center multiplayer

An optional two-player shared-practice prototype is implemented for macOS.
See [setup, controls and verification](docs/testing/multiplayer.md).

- [x] Finish Apple Developer setup under the developer's team: Todora App ID, Game Center,
  App Store Connect record, development certificate and two-Mac profile (2026-09-22).
- [ ] Verify a live Game Center match between two signed Macs with distinct
  accounts, including invitations, Internet latency, pause and disconnects.
- [ ] Bring the native integration to tvOS after the single-player port works.
- [ ] Evaluate authoritative racing and car contact after shared practice is
  proven; the prototype keeps cars independent and has no collision handling.

## Apple TV / tvOS port

Explore a native Apple TV version while preserving Todora's driving feel and
visual style. Reuse the existing physics, circuits, timing and ghost logic.
Feasibility reviewed on 2026-09-21; no tvOS build or hardware test performed yet.

- [ ] Identify the Apple TV model and controller available for testing.
- [ ] Prototype one circuit with a driveable car, controller input and sound on
  a real Apple TV. Measure frame rate and input responsiveness before estimating
  the full port.
- [ ] Resolve platform dependencies: Todora currently uses Bevy 0.19.1 with
  winit 0.30.13, which rejects tvOS. Evaluate newer winit UIKit support or a
  custom integration; compatibility with Bevy remains unverified. The current
  gilrs controller and CPAL audio backends also need tvOS support or replacement.
- [ ] Add an Xcode tvOS app target, Metal rendering setup, bundled assets,
  signing and provisioning. Rust provides an ARM64 tvOS target.
- [ ] Connect Apple's Game Controller framework to the existing controls and
  support all menus with a gamepad. Treat Siri Remote driving as a separate
  design decision.
- [ ] Adapt HUD readability for TV viewing, app suspension/resumption, controller
  disconnection and audio interruptions.
- [ ] Adapt best-time and ghost storage to tvOS. Account for purgeable local
  files and evaluate iCloud persistence for records and ghosts.
- [ ] Verify all circuits, timing, lap validity and ghosts on hardware; profile
  demanding layouts and loading times while retaining the current handling.
- [ ] Once the port is ready, prepare TestFlight testing and an eventual App
  Store submission. Distribution requires Apple Developer Program membership.

References: [Rust tvOS targets](https://doc.rust-lang.org/rustc/platform-support/apple-tvos.html),
[upstream winit UIKit backend](https://github.com/rust-windowing/winit/blob/master/winit-uikit/src/lib.rs),
[Apple controller integration](https://developer.apple.com/documentation/xcode/configuring-game-controllers),
[Apple tvOS storage guidance (archived)](https://developer.apple.com/library/archive/documentation/General/Conceptual/AppleTV_PG/iCloudStorage.html),
[Apple distribution guidance](https://developer.apple.com/documentation/Xcode/distributing-your-app-for-beta-testing-and-releases).
