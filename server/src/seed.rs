//! `todora-server seed [circuit] [players]` fills a board with made-up drivers
//! for trying the game against a busy board locally. Every one of them points
//! at the same real AI lap, so ghosts download and replay. Never run it on the
//! live server: nothing marks these drivers as fake.

use rusqlite::Connection;
use todora::verify;

use crate::db;

pub fn run(db: &Connection, args: &[String]) {
    let circuit = args.first().map_or("monza", String::as_str);
    let players: u32 = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(500);
    let bytes = verify::ai_run(circuit, "regular").expect("the AI laps this circuit");
    let checked = verify::Verifier::new()
        .check(&bytes)
        .expect("the AI lap is a lap");
    let countries = ["AT", "DE", "GB", "FR", "IT", "ES", "NL", "US", "JP", "BR"];
    let names = [
        "Kestrel", "Magpie", "Merlin", "Heron", "Swift", "Falcon", "Osprey", "Wren",
    ];
    for i in 0..players {
        let id = format!("seed{i:06}");
        let key = [i.to_le_bytes(), [0x5e, 0xed, 0, 0]].concat().repeat(4);
        let name = format!("{} {}", names[i as usize % names.len()], 100 + i);
        let country = countries[i as usize % countries.len()];
        if db::add_player(db, &id, &key, &name, Some(country)).is_err() {
            continue;
        }
        // Spread the field from the AI's time down to 25% quicker.
        let steps =
            (f64::from(checked.steps) * (0.75 + 0.25 * f64::from(i) / f64::from(players))) as u32;
        let seeded = verify::Checked {
            steps,
            lap: u64::from(i) | 0x5eed_0000_0000_0000,
            ..checked.clone()
        };
        let run = format!("{:016x}", seeded.lap);
        db::keep_run(db, &run, &id, &seeded, &bytes).expect("a seeded lap");
    }
    eprintln!("seeded {players} drivers on {circuit} regular");
}
