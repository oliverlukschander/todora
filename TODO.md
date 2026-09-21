# TODO

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
