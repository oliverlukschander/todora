# Privacy: Todora's world leaderboards

Todora works offline and sends nothing anywhere until you choose to put your
laps on the world boards. This page says what happens once you do.

**Who.** The boards are run by the maker of Todora on a server in the EU
(Hetzner, Falkenstein, Germany), at `todora.lukschander.com`. Responsible for
the data: Oliver Lukschander, [oliver@lukschander.com](mailto:oliver@lukschander.com).

**What is stored, and why.**

| What | Why | How long |
| --- | --- | --- |
| A random player id and a public key made on your machine | To know that a lap is yours without an account or password | Until you delete it |
| The name you chose, and a country if you picked one | To show you on the boards | Until you change or delete it |
| Your laps: the inputs and the car's state at the line, with the car, setup, mode and game version | To drive every lap again before it counts, and so others can race it as a ghost | Until you delete it |
| Reports you make about a lap, with the reason you gave | So a lap that looks wrong can be looked at | Until you, or the lap's driver, delete their data |
| Laps that were refused: your player id and the reason only | To notice trouble | 30 days |

**What is not stored.** No email, password, real name or address. Your IP
address reaches the server with each request, as it does with any website; it
is used only to limit how often one address can ask, held in memory for an
hour, and never written to disk. The reverse proxy keeps no access log for the
boards.

**Who sees it.** Anyone looking at a board sees names, countries, times, cars
and setups, and can download a lap to race as a ghost. Nothing else is shared
with anyone.

**Your rights.** Settings → Online → **Delete my online data** removes your
player, every lap and place of yours, and the reports you made, from the server
at once, and removes the key from your machine. You can change your name (once
a week) and country there too, or go offline, which stops anything more being
sent. For anything else — a copy of what is held about you, a correction, a
question — use the contact address above. Under the GDPR you may also complain
to your data protection authority; in Austria that is the Datenschutzbehörde.

**Legal basis.** Your consent, given when you choose to go online (Art. 6(1)(a)
GDPR), which you can withdraw at any time by going offline or deleting your
data.
