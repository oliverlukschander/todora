# Play together on one computer

Choose **Split-screen** from the starting screen. Join two to four players, choose
a circuit with left/right, then press **Space** or a controller's **Start** button.
Every player gets a coloured car, their own camera and a lap clock. The fastest
valid lap is the session's target; cars can pass through each other.

| Device | Join | Drive | Handbrake |
| --- | --- | --- | --- |
| Keyboard 1 | Enter or the WASD button | W / S / A / D | Space |
| Keyboard 2 | Right Shift or the arrow button | Arrow keys | Right Ctrl |
| Gamepad | A | RT or A accelerates; LT or X brakes; left stick or D-pad steers | B |

Use any mix of these devices, up to four players. One controller belongs to one
car. In the lobby B removes that controller and Backspace removes the last joined
player. Keyboard rollover depends on the keyboard; gamepads avoid crowded keys.

Two players share the screen side by side. Three use three quadrants with session
standings in the fourth; four use all four quadrants. **Esc / Start** pauses
everyone. From the pause, **R** restarts the session and **F10 / B** returns to the
lobby. A disconnected controller pauses all players; reconnect it or press A on a
replacement, then resume. From the lobby, Esc returns to the starting screen.

These are local session times, separate from personal records, achievements and
world leaderboard uploads. All players use the currently selected car, setup and
mode with individual input; choose those in the solo garage before entering.
Local play uses chase cameras and native rendering resolution. The usual render
scale is restored after leaving. Restarting clears the local session times.

For reproducible runtime checks, build with `visual-check` and use
`TODORA_SCREEN=local`, `local-lobby`, or `local-cycle` with `TODORA_PLAYERS=2..4` and
`TODORA_CAPTURE=/tmp/todora-local.png`. The cycle verifies camera/car counts and
cleanup on returning to solo play. Set `BEVY_ASSET_ROOT` to the repository when
running the binary directly. This mode does not write player settings or laps.
