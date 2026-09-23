# Leaderboard server

`server/` is Todora's leaderboard: one Rust binary and one SQLite file. It links
the game as a library, so every submitted lap is driven again by the game's own
engine and lap judge before it counts. A time on the board is a lap somebody
drove, on the circuit as this build lays it out, under these physics.

## What it checks

1. **Signature.** A player is an Ed25519 key pair made on their machine. Every
   change is signed over the method, path, a timestamp and the body; requests
   more than five minutes off the server's clock are refused.
2. **Envelope.** The run must decode whole, be at most 256 KB, name a circuit
   this build has, carry that circuit's current fingerprint and the current
   `PHYSICS_VERSION`, and come from a game with the same major and minor
   version.
3. **Replay.** The entry state must be a car at the line, facing along the road,
   no faster than the car can go in its mode and not sliding. The inputs are
   then driven step by step; the lap must finish on the claimed step, stay valid
   and produce the claimed sectors. Two replays run at a time.
4. **Once only.** A lap is identified by a hash of its entry, car, setup and
   inputs — not of its bytes — so a downloaded ghost cannot be resubmitted
   under another name, even with its version string changed.
5. **Limits.** 240 requests an hour per address and 60 per player, counted in
   memory only.
6. **Reports.** Times appear at once. Anyone can report a lap; reported laps
   are listed for the admin, who can hide one, which puts the player's next best
   lap on the board instead. Replay cannot catch an input bot or a
   tool-assisted lap; reports are for those.

## API

| Method | Path | What |
| --- | --- | --- |
| GET | `/v1/health` | server, game and physics version |
| POST | `/v1/players` | register: `{"public_key", "name", "country"?}`, signed with that key |
| PATCH | `/v1/players/{id}` | `{"name"?, "country"?}`; a name changes once a week |
| DELETE | `/v1/players/{id}` | delete the player, their laps, places and reports |
| POST | `/v1/runs` | submit a run (the bytes of a `TODORUN1` file) |
| GET | `/v1/runs/{id}` | a lap's bytes, to race as a ghost |
| GET | `/v1/boards/{circuit}/{mode}` | top ten; `?player=` adds that player ±5; `?country=AT`, `?car=clubman`, `?season=`, `?rivals=id,id`, `?week=2026-W39` (laps set that week) |
| POST | `/v1/reports` | `{"run", "reason"}` |
| GET | `/admin/reports` | reported laps (`x-admin-token`) |
| POST | `/admin/runs/{id}/hide` | take a lap off the boards (`x-admin-token`) |

Signed requests carry `x-todora-player` (except registration), `x-todora-time`
(Unix seconds) and `x-todora-signature` (base64 Ed25519 over
`"{METHOD} {path}\n{time}\n"` followed by the body).

A board is a circuit, a mode and a physics version. When the physics change,
`PHYSICS_VERSION` moves on and a new season starts; the old boards stay
readable with `?season=`.

## Privacy

Players are a random id, a public key, a name and an optional country they
chose. There is no email, password or address. IP addresses are used for rate
limits in memory and never written. Rejected submissions are kept for 30 days
as a reason and a player id only. `DELETE /v1/players/{id}` removes everything
the server holds about a player.

## Running it locally

```sh
cargo test --locked -p todora-server
TODORA_DB=/tmp/todora.db cargo run --locked -p todora-server
cargo run --locked -p todora-server -- seed monza 2000   # a busy board to look at
```

Then start the game with `TODORA_BOARD_URL=http://127.0.0.1:8787`.

In a Linux container, as it will run on the server:

```sh
docker build -f server/Dockerfile -t todora-server .
docker run --rm -p 8787:8787 -v todora-data:/data todora-server
curl http://127.0.0.1:8787/v1/health
```

Configuration is `TODORA_DB`, `TODORA_LISTEN` (default `127.0.0.1:8787`; the
image listens on `0.0.0.0:8787`) and `TODORA_ADMIN_TOKEN` (24 characters or
more; moderation is off without it).

## Deploying

```sh
server/deploy/deploy.sh root@your-hetzner-host          # ARCH=arm64 for CAX machines
server/deploy/deploy.sh local                           # try the same thing on this machine
```

The script builds the image for the target, copies it over SSH, and runs one
container on `127.0.0.1:8787` with its data in the `todora-data` volume,
`--restart unless-stopped`, 512 MB of memory and 1.5 CPUs. It makes an admin
token once, in `/etc/todora/admin-token` on the target, and passes it to the
container. It does not touch anything else on the host.

Put the host's reverse proxy in front of it for TLS and the name
`todora.lukschander.com`: `server/deploy/Caddyfile.example` or
`server/deploy/nginx.example`. The proxy must set `X-Forwarded-For` and should
keep no access log for this site.

Back up the `todora-data` volume (`docker run --rm -v todora-data:/data -v
$PWD:/out debian tar czf /out/todora-data.tgz /data`). The database is SQLite in
WAL mode; copy it while the container is stopped, or with `sqlite3 .backup`.

Before going live, fill in the contact address in [privacy.md](privacy.md).
