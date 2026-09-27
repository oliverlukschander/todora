# Official competition and custom builds

The official leaderboard currently verifies **stock game rules**. The server
replays every submitted input under its own physics and track layout. A modified
client cannot make a faster tuned car count by merely claiming a stock version:
the submitted lap still has to reproduce under the server's rules.

This does **not** prove that Oliver's signed executable produced the request.
Player signatures authenticate a player's key, not a program. A public release
signature, binary hash or version number can be copied into another client's
request. A secret embedded in a downloadable executable can be extracted.
Reproducible input is also not proof that a human drove it.

## Recommended competition model

Keep **Official** for laps reproduced under the published stock rules. Add a
separate **Workshop** competition, organized by a hash of the complete tuning
rules, track fingerprint and physics version. Make the selected competition
visible in the board title, ghost card and upload result.

For Workshop, accept a bounded, declarative car specification (for example mass,
power, grip and steering), never executable code. The server validates finite
values and supported bounds, then replays the run with exactly that specification.
Two different tuning specifications get different rankings. Workshop ghosts carry
their specification and require an explicit switch before driving against them.
Official achievements and records remain separate. Arbitrary physics forks can
still play offline; they do not become verified Workshop runs automatically.

A rejected Official run must stay rejected. Silently moving it to another board
would conceal corrupt recordings and make the competition identity ambiguous.

This Workshop protocol is a proposal, **not an implemented endpoint**. It needs a
versioned recording envelope, separate board storage and query routes, verification
limits, explicit UI selection, and tests that cross-board submissions cannot leak.

## If executable provenance is mandatory

Strict admission of only Oliver-built binaries requires a trusted attestation
mechanism independent of the client process, with server challenges, fresh proofs
and a platform-specific trust chain. Signing downloads is still useful for safe
installation, but it does not provide that server-side proof. The project currently
has no such cross-platform attestation mechanism; Linux support must be considered
before promising one. Do not label replay verification as binary attestation.

## Player names and countries

Profiles save explicitly and only take the name/country confirmed by the server.
A failed country or name validation changes neither field nor the rename cooldown.
Names may change once a week; countries can change without renaming.

`src/identity/reserved.txt` reserves DHH, the creator handle, and published Omarchy
team handles/aliases. The roster was checked against
[Omarchy Teams](https://omarchy.org/teams/) on 2026-09-27; for members without a
published X link the public GitHub handle is reserved instead of guessing an X
handle. Case, separators and full-width ASCII are normalized for matching; this
is not a complete Unicode-confusable detector.

An existing account keeps its current name when editing its country, including a
reserved name. An operator must verify ownership before assigning a reserved name
to an account and audit any older claims before deploying the reservation list.
There is no automatic X-ownership verification or public claim workflow. Do not
reassign an existing identity solely because its display name matches a handle.

Flags are a bundled atlas from the MIT-licensed flag-icons project. The game never
contacts a flag service; choosing a country is optional. Attribution and the pinned
source revision are in `assets/ui/flags/CREDITS.md`.
