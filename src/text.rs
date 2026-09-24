//! Every word the game shows, in English, German, French, Spanish and Italian.
//!
//! [`t`] looks a key up in the language in force, falling back to English, so
//! any system can ask for a string without being handed a resource. The
//! language is the one chosen in the settings, or the system's when that says
//! Automatic. Labels built once at startup carry a [`Tr`] marker and are
//! written again when the language changes; everything drawn each frame asks
//! [`t`] as it draws. `{}` in a string is filled in by [`tf`], in order.

use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Language {
    En,
    De,
    Fr,
    Es,
    It,
}

impl Language {
    pub(crate) const ALL: [Self; 5] = [Self::En, Self::De, Self::Fr, Self::Es, Self::It];

    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::De => "de",
            Self::Fr => "fr",
            Self::Es => "es",
            Self::It => "it",
        }
    }

    /// Its own name for itself.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::De => "Deutsch",
            Self::Fr => "Français",
            Self::Es => "Español",
            Self::It => "Italiano",
        }
    }

    pub(crate) fn from_code(code: &str) -> Option<Self> {
        let code = code.get(..2)?.to_lowercase();
        Self::ALL.into_iter().find(|l| l.code() == code)
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub(crate) fn current() -> Language {
    Language::ALL[CURRENT.load(Relaxed) as usize % Language::ALL.len()]
}

pub(crate) fn set(language: Language) {
    let at = Language::ALL
        .iter()
        .position(|l| *l == language)
        .unwrap_or(0);
    CURRENT.store(at as u8, Relaxed);
}

/// The system's language, if it is one of ours: the locale variables, then on
/// macOS the first of the user's preferred languages.
pub(crate) fn system() -> Language {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(language) = std::env::var(var)
            .ok()
            .as_deref()
            .and_then(Language::from_code)
        {
            return language;
        }
    }
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(first) = text.split('"').nth(1).and_then(Language::from_code) {
            return first;
        }
    }
    Language::En
}

/// The string for `key` in the language in force.
pub(crate) fn t(key: &str) -> &'static str {
    t_in(current(), key)
}

/// The string for `key` in `language`, or English, or `?`.
pub(crate) fn t_in(language: Language, key: &str) -> &'static str {
    let at = Language::ALL
        .iter()
        .position(|l| *l == language)
        .unwrap_or(0);
    TABLE
        .iter()
        .chain(MORE)
        .find(|(k, _)| *k == key)
        .map_or("?", |(_, words)| {
            let word = words[at];
            if word.is_empty() { words[0] } else { word }
        })
}

/// [`t`] with each `{}` filled in by the next of `args`.
pub(crate) fn tf(key: &str, args: &[&dyn std::fmt::Display]) -> String {
    fill(t(key), args)
}

fn fill(text: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut rest = text;
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        if let Some(arg) = args.next() {
            out.push_str(&arg.to_string());
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

/// A label written once and written again when the language changes.
#[derive(Component, Clone, Copy)]
pub(crate) struct Tr(pub &'static str);

/// A label in the language in force, that follows it.
pub(crate) fn label(key: &'static str, size: f32, color: Color) -> impl Bundle {
    (crate::ui::label(t(key), size, color), Tr(key))
}

pub struct TextPlugin;

impl Plugin for TextPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, choose).add_systems(
            Update,
            (
                choose.run_if(resource_changed::<crate::settings::Settings>),
                retranslate,
            )
                .chain(),
        );
    }
}

/// The settings' language, or the system's.
fn choose(settings: Option<Res<crate::settings::Settings>>, mut seen: Local<Option<Language>>) {
    let wanted = settings
        .as_ref()
        .and_then(|s| Language::from_code(&s.language))
        .unwrap_or_else(|| *seen.get_or_insert_with(system));
    set(wanted);
}

/// Every marked label, again, when the language has changed.
fn retranslate(mut labels: Query<(&Tr, &mut Text)>, mut last: Local<Option<Language>>) {
    let now = current();
    if *last == Some(now) {
        return;
    }
    *last = Some(now);
    for (tr, mut text) in &mut labels {
        let wanted = t(tr.0);
        if text.0 != wanted {
            text.0 = wanted.into();
        }
    }
}

/// Key, then English, German, French, Spanish, Italian.
type Entry = (&'static str, [&'static str; 5]);

#[rustfmt::skip]
pub(crate) const TABLE: &[Entry] = &[
    // The pause.
    ("pause.label", ["TODORA  /  SESSION PAUSED", "TODORA  /  PAUSE", "TODORA  /  PAUSE", "TODORA  /  PAUSA", "TODORA  /  PAUSA"]),
    ("pause.title", ["Take a breather.", "Kurz durchatmen.", "Une petite pause.", "Tómate un respiro.", "Prendi fiato."]),
    ("pause.note", ["Your lap will be right here.", "Deine Runde wartet hier.", "Votre tour vous attend ici.", "Tu vuelta te espera aquí.", "Il tuo giro ti aspetta qui."]),
    ("pause.session", ["The session continues. This lap is invalid.", "Die Sitzung läuft weiter. Diese Runde zählt nicht.", "La session continue. Ce tour est invalide.", "La sesión continúa. Esta vuelta no cuenta.", "La sessione continua. Questo giro non vale."]),
    ("pause.resume", ["Resume", "Weiter", "Reprendre", "Continuar", "Riprendi"]),
    ("pause.reset_one", ["Reset this ghost", "Diesen Geist zurücksetzen", "Effacer ce fantôme", "Borrar este fantasma", "Azzera questo fantasma"]),
    ("pause.reset_all", ["Reset all ghosts", "Alle Geister zurücksetzen", "Effacer tous les fantômes", "Borrar todos los fantasmas", "Azzera tutti i fantasmi"]),
    ("pause.board", ["World leaderboard / L", "Weltrangliste / L", "Classement mondial / L", "Clasificación mundial / L", "Classifica mondiale / L"]),
    ("pause.replay", ["Replay and photo", "Wiederholung und Foto", "Replay et photo", "Repetición y foto", "Replay e foto"]),
    ("pause.guide", ["How to drive", "So wird gefahren", "Comment piloter", "Cómo conducir", "Come si guida"]),
    ("pause.settings", ["Settings", "Einstellungen", "Réglages", "Ajustes", "Impostazioni"]),
    ("pause.quit", ["Quit game", "Spiel beenden", "Quitter le jeu", "Salir del juego", "Esci dal gioco"]),
    ("pause.reset_note", ["Reset removes saved best times and restarts the lap.", "Zurücksetzen löscht gespeicherte Bestzeiten und startet die Runde neu.", "Effacer supprime les meilleurs temps enregistrés et relance le tour.", "Borrar elimina los mejores tiempos guardados y reinicia la vuelta.", "Azzerare cancella i migliori tempi salvati e riavvia il giro."]),
    ("pause.hint", ["↑ ↓ / D-pad / Stick  Select\nEnter / A  Confirm · Esc / B / Start  Resume", "↑ ↓ / Steuerkreuz / Stick  Wählen\nEnter / A  Bestätigen · Esc / B / Start  Weiter", "↑ ↓ / Croix / Stick  Choisir\nEntrée / A  Valider · Échap / B / Start  Reprendre", "↑ ↓ / Cruceta / Stick  Elegir\nIntro / A  Confirmar · Esc / B / Start  Continuar", "↑ ↓ / Croce / Levetta  Scegli\nInvio / A  Conferma · Esc / B / Start  Riprendi"]),
    ("sound.music", ["Music  /  N    {}", "Musik  /  N    {}", "Musique  /  N    {}", "Música  /  N    {}", "Musica  /  N    {}"]),
    ("sound.effects", ["Sound effects  /  F8    {}", "Effekte  /  F8    {}", "Effets  /  F8    {}", "Efectos  /  F8    {}", "Effetti  /  F8    {}"]),
    ("word.on", ["On", "An", "Oui", "Sí", "Sì"]),
    ("word.off", ["Off", "Aus", "Non", "No", "No"]),
    ("word.none", ["None", "Keines", "Aucun", "Ninguno", "Nessuno"]),
    // The title.
    ("title.subtitle", ["Forty circuits. One lap at a time.", "Vierzig Strecken. Eine Runde nach der anderen.", "Quarante circuits. Un tour à la fois.", "Cuarenta circuitos. Una vuelta cada vez.", "Quaranta circuiti. Un giro alla volta."]),
    ("title.drive", ["Drive", "Fahren", "Piloter", "Conducir", "Guida"]),
    ("title.circuits", ["Circuits", "Strecken", "Circuits", "Circuitos", "Circuiti"]),
    ("title.board", ["World leaderboard", "Weltrangliste", "Classement mondial", "Clasificación mundial", "Classifica mondiale"]),
    ("title.weekly", ["This week's challenge", "Herausforderung der Woche", "Défi de la semaine", "Reto de la semana", "Sfida della settimana"]),
    ("title.quit", ["Quit", "Beenden", "Quitter", "Salir", "Esci"]),
    ("title.hint", ["↑ ↓  Choose     Enter / A  Go", "↑ ↓  Wählen     Enter / A  Los", "↑ ↓  Choisir     Entrée / A  Go", "↑ ↓  Elegir     Intro / A  Vamos", "↑ ↓  Scegli     Invio / A  Via"]),
    // The countdown.
    ("countdown.ready", ["READY", "BEREIT", "PRÊT", "LISTO", "PRONTI"]),
    ("countdown.go", ["GO", "LOS", "GO", "YA", "VIA"]),
    // The HUD.
    ("hud.free_drive", ["TODORA  /  FREE DRIVE", "TODORA  /  FREIES FAHREN", "TODORA  /  PILOTAGE LIBRE", "TODORA  /  CONDUCCIÓN LIBRE", "TODORA  /  GUIDA LIBERA"]),
    ("hud.timing", ["SESSION TIMING", "ZEITNAHME", "CHRONOMÉTRAGE", "CRONOMETRAJE", "CRONOMETRAGGIO"]),
    ("hud.invalid", ["LAP INVALID", "RUNDE UNGÜLTIG", "TOUR INVALIDE", "VUELTA NO VÁLIDA", "GIRO NON VALIDO"]),
    ("hud.clock", ["LAP  {}\n{}\nLAST {}\nBEST {}", "RUNDE  {}\n{}\nLETZTE {}\nBESTE {}", "TOUR  {}\n{}\nDERNIER {}\nMEILLEUR {}", "VUELTA  {}\n{}\nÚLTIMA {}\nMEJOR {}", "GIRO  {}\n{}\nULTIMO {}\nMIGLIORE {}"]),
    ("hud.sector", ["SECTOR {}   {}{}", "SEKTOR {}   {}{}", "SECTEUR {}   {}{}", "SECTOR {}   {}{}", "SETTORE {}   {}{}"]),
    ("hud.ghost", ["GHOST", "GEIST", "FANTÔME", "FANTASMA", "FANTASMA"]),
    ("hud.ghost_off", ["GHOST OFF", "GEIST AUS", "FANTÔME COUPÉ", "FANTASMA APAGADO", "FANTASMA SPENTO"]),
    ("hud.keys", [
        "WASD / Arrows  Drive    Space  Handbrake    R  Restart\nG  Ghost    V  Camera    L  Board    Scroll  Zoom    Esc  Pause",
        "WASD / Pfeile  Fahren    Leertaste  Handbremse    R  Neustart\nG  Geist    V  Kamera    L  Rangliste    Scrollen  Zoom    Esc  Pause",
        "ZQSD / Flèches  Piloter    Espace  Frein à main    R  Recommencer\nG  Fantôme    V  Caméra    L  Classement    Molette  Zoom    Échap  Pause",
        "WASD / Flechas  Conducir    Espacio  Freno de mano    R  Reiniciar\nG  Fantasma    V  Cámara    L  Clasificación    Rueda  Zoom    Esc  Pausa",
        "WASD / Frecce  Guida    Spazio  Freno a mano    R  Ricomincia\nG  Fantasma    V  Camera    L  Classifica    Rotella  Zoom    Esc  Pausa",
    ]),
    ("hud.pad", [
        "Left stick  Steer    A  Gas    X  Brake    B  Handbrake    RB  Reset\nStart  Pause    LB  Garage    View  Circuits    Y  Ghost    D-pad up  Camera",
        "Linker Stick  Lenken    A  Gas    X  Bremse    B  Handbremse    RB  Neustart\nStart  Pause    LB  Garage    View  Strecken    Y  Geist    Steuerkreuz hoch  Kamera",
        "Stick gauche  Diriger    A  Gaz    X  Frein    B  Frein à main    RB  Recommencer\nStart  Pause    LB  Garage    View  Circuits    Y  Fantôme    Croix haut  Caméra",
        "Stick izquierdo  Girar    A  Gas    X  Freno    B  Freno de mano    RB  Reiniciar\nStart  Pausa    LB  Garaje    View  Circuitos    Y  Fantasma    Cruceta arriba  Cámara",
        "Levetta sinistra  Sterza    A  Gas    X  Freno    B  Freno a mano    RB  Ricomincia\nStart  Pausa    LB  Garage    View  Circuiti    Y  Fantasma    Croce su  Camera",
    ]),
    ("hud.sound", ["N  Music: {}    F8  Effects: {}", "N  Musik: {}    F8  Effekte: {}", "N  Musique : {}    F8  Effets : {}", "N  Música: {}    F8  Efectos: {}", "N  Musica: {}    F8  Effetti: {}"]),
    ("hud.fastest", ["  FASTEST", "  SCHNELLSTER", "  LE PLUS RAPIDE", "  EL MÁS RÁPIDO", "  IL PIÙ VELOCE"]),
    ("hud.faster", ["  FASTER", "  SCHNELLER", "  PLUS RAPIDE", "  MÁS RÁPIDO", "  PIÙ VELOCE"]),
    ("hud.slower", ["  SLOWER", "  LANGSAMER", "  PLUS LENT", "  MÁS LENTO", "  PIÙ LENTO"]),
    // The first-drive cards.
    ("guide.drive", ["DRIVE", "FAHREN", "PILOTER", "CONDUCIR", "GUIDA"]),
    ("guide.drive_keys", [
        "W  accelerate      S  brake, reverse from a stop\nA / D  steer      Space  handbrake      R  restart the lap",
        "W  Gas      S  Bremse, aus dem Stand rückwärts\nA / D  lenken      Leertaste  Handbremse      R  Runde neu starten",
        "W  accélérer      S  freiner, reculer à l'arrêt\nA / D  diriger      Espace  frein à main      R  recommencer le tour",
        "W  acelerar      S  frenar, marcha atrás parado\nA / D  girar      Espacio  freno de mano      R  reiniciar la vuelta",
        "W  accelera      S  frena, retromarcia da fermo\nA / D  sterza      Spazio  freno a mano      R  ricomincia il giro",
    ]),
    ("guide.drive_pad", [
        "RT or A  accelerate      LT or X  brake, reverse from a stop\nLeft stick  steer      B  handbrake      RB  restart the lap",
        "RT oder A  Gas      LT oder X  Bremse, aus dem Stand rückwärts\nLinker Stick  lenken      B  Handbremse      RB  Runde neu starten",
        "RT ou A  accélérer      LT ou X  freiner, reculer à l'arrêt\nStick gauche  diriger      B  frein à main      RB  recommencer le tour",
        "RT o A  acelerar      LT o X  frenar, marcha atrás parado\nStick izquierdo  girar      B  freno de mano      RB  reiniciar la vuelta",
        "RT o A  accelera      LT o X  frena, retromarcia da fermo\nLevetta sinistra  sterza      B  freno a mano      RB  ricomincia il giro",
    ]),
    ("guide.lap", ["THE LAP", "DIE RUNDE", "LE TOUR", "LA VUELTA", "IL GIRO"]),
    ("guide.lap_body", [
        "The clock starts at the chequered line, after the run-up.\nAll four wheels off the asphalt and kerbs, and the lap does not count.",
        "Die Uhr startet an der Zielflagge, nach dem Anlauf.\nAlle vier Räder neben Asphalt und Randsteinen, und die Runde zählt nicht.",
        "Le chrono démarre au damier, après l'élan.\nLes quatre roues hors de l'asphalte et des vibreurs, et le tour ne compte pas.",
        "El reloj arranca en la línea a cuadros, tras la carrerilla.\nCon las cuatro ruedas fuera del asfalto y los pianos, la vuelta no cuenta.",
        "Il cronometro parte alla linea a scacchi, dopo la rincorsa.\nTutte e quattro le ruote fuori da asfalto e cordoli, e il giro non vale.",
    ]),
    ("guide.ghost", ["THE GHOST", "DER GEIST", "LE FANTÔME", "EL FANTASMA", "IL FANTASMA"]),
    ("guide.ghost_keys", [
        "Your best lap drives again beside you in amber.\nBeat it, and it becomes the new one.  G  shows or hides it.",
        "Deine beste Runde fährt in Bernstein neben dir.\nSchlag sie, und sie wird die neue.  G  blendet sie ein oder aus.",
        "Votre meilleur tour roule à côté de vous, en ambre.\nBattez-le, et il devient le nouveau.  G  l'affiche ou le cache.",
        "Tu mejor vuelta corre a tu lado en ámbar.\nSupérala y será la nueva.  G  la muestra u oculta.",
        "Il tuo giro migliore corre accanto a te, in ambra.\nBattilo e diventa il nuovo.  G  lo mostra o lo nasconde.",
    ]),
    ("guide.ghost_pad", [
        "Your best lap drives again beside you in amber.\nBeat it, and it becomes the new one.  Y  shows or hides it.",
        "Deine beste Runde fährt in Bernstein neben dir.\nSchlag sie, und sie wird die neue.  Y  blendet sie ein oder aus.",
        "Votre meilleur tour roule à côté de vous, en ambre.\nBattez-le, et il devient le nouveau.  Y  l'affiche ou le cache.",
        "Tu mejor vuelta corre a tu lado en ámbar.\nSupérala y será la nueva.  Y  la muestra u oculta.",
        "Il tuo giro migliore corre accanto a te, in ambra.\nBattilo e diventa il nuovo.  Y  lo mostra o lo nasconde.",
    ]),
    ("guide.footer", ["{} / {}        {}  {}      {}  skip", "{} / {}        {}  {}      {}  überspringen", "{} / {}        {}  {}      {}  passer", "{} / {}        {}  {}      {}  saltar", "{} / {}        {}  {}      {}  salta"]),
    ("guide.next", ["next", "weiter", "suivant", "siguiente", "avanti"]),
    ("guide.go", ["drive", "fahren", "piloter", "conducir", "guida"]),
    ("guide.beginner", [
        "Laps not counting?  Beginner mode is 20% slower — C / LB  →  Garage & setup  →  mode.",
        "Runden zählen nicht?  Der Anfängermodus ist 20% langsamer — C / LB  →  Garage  →  Modus.",
        "Des tours qui ne comptent pas ?  Le mode Débutant est 20% plus lent — C / LB  →  Garage  →  mode.",
        "¿Vueltas que no cuentan?  El modo Principiante es un 20% más lento — C / LB  →  Garaje  →  modo.",
        "Giri che non valgono?  La modalità Principiante è più lenta del 20% — C / LB  →  Garage  →  modalità.",
    ]),
    // The lap card.
    ("card.lap", ["LAP", "RUNDE", "TOUR", "VUELTA", "GIRO"]),
    ("card.new_best", ["NEW BEST", "NEUE BESTZEIT", "NOUVEAU RECORD", "NUEVO RÉCORD", "NUOVO RECORD"]),
    ("card.first", ["FIRST TIME SET", "ERSTE ZEIT", "PREMIER TEMPS", "PRIMER TIEMPO", "PRIMO TEMPO"]),
    ("card.to_best", ["{} to best", "{} zur Bestzeit", "{} du record", "{} al récord", "{} dal record"]),
    ("card.invalid", ["INVALID", "UNGÜLTIG", "INVALIDE", "NO VÁLIDA", "NON VALIDO"]),
    ("card.assisted", ["ASSISTED", "MIT HILFE", "ASSISTÉ", "ASISTIDA", "ASSISTITO"]),
    ("card.off_track", ["All four wheels off the track in sector {}", "Alle vier Räder neben der Strecke in Sektor {}", "Les quatre roues hors piste au secteur {}", "Las cuatro ruedas fuera de pista en el sector {}", "Tutte e quattro le ruote fuori pista nel settore {}"]),
    ("card.rescued", ["Fetched back to the road in sector {}", "In Sektor {} auf die Strecke zurückgeholt", "Ramené sur la piste au secteur {}", "Devuelto a la pista en el sector {}", "Riportato in pista nel settore {}"]),
    ("card.paused", ["Paused during shared practice", "Im gemeinsamen Training pausiert", "Pause pendant l'entraînement partagé", "Pausa durante el entrenamiento compartido", "In pausa durante le prove condivise"]),
    ("card.not_counted", ["Did not count", "Zählt nicht", "Ne compte pas", "No cuenta", "Non vale"]),
    ("card.top", ["Top speed {} {}", "Höchstgeschwindigkeit {} {}", "Vitesse max {} {}", "Velocidad máxima {} {}", "Velocità massima {} {}"]),
    ("card.slowest", ["{}    ·    slowest {} {} in sector {}", "{}    ·    langsamste {} {} in Sektor {}", "{}    ·    minimum {} {} au secteur {}", "{}    ·    mínima {} {} en el sector {}", "{}    ·    minima {} {} nel settore {}"]),
    ("card.banner", ["NEW BEST", "NEUE BESTZEIT", "NOUVEAU RECORD", "NUEVO RÉCORD", "NUOVO RECORD"]),
    // The settings page.
    ("settings.label", ["TODORA  /  SETTINGS", "TODORA  /  EINSTELLUNGEN", "TODORA  /  RÉGLAGES", "TODORA  /  AJUSTES", "TODORA  /  IMPOSTAZIONI"]),
    ("settings.hint", [
        "↑ ↓  Choose    ← →  Change    LB / RB  or  Q / E  Tab\nEsc / B  Back",
        "↑ ↓  Wählen    ← →  Ändern    LB / RB  oder  Q / E  Reiter\nEsc / B  Zurück",
        "↑ ↓  Choisir    ← →  Modifier    LB / RB  ou  Q / E  Onglet\nÉchap / B  Retour",
        "↑ ↓  Elegir    ← →  Cambiar    LB / RB  o  Q / E  Pestaña\nEsc / B  Volver",
        "↑ ↓  Scegli    ← →  Cambia    LB / RB  o  Q / E  Scheda\nEsc / B  Indietro",
    ]),
    ("tab.audio", ["Audio", "Audio", "Audio", "Audio", "Audio"]),
    ("tab.display", ["Display", "Anzeige", "Affichage", "Pantalla", "Schermo"]),
    ("tab.hud", ["HUD", "HUD", "HUD", "HUD", "HUD"]),
    ("tab.controls", ["Controls", "Steuerung", "Commandes", "Controles", "Comandi"]),
    ("tab.keys", ["Keys", "Tasten", "Touches", "Teclas", "Tasti"]),
    ("tab.online", ["Online", "Online", "En ligne", "En línea", "Online"]),
    ("row.music", ["Radio", "Radio", "Radio", "Radio", "Radio"]),
    ("row.music_volume", ["Music volume", "Musiklautstärke", "Volume de la musique", "Volumen de la música", "Volume musica"]),
    ("row.effects", ["Tyre sound and beeps", "Reifen und Signaltöne", "Pneus et bips", "Neumáticos y pitidos", "Gomme e segnali"]),
    ("row.effects_volume", ["Effects volume", "Effektlautstärke", "Volume des effets", "Volumen de efectos", "Volume effetti"]),
    ("row.engine_volume", ["Engine volume", "Motorlautstärke", "Volume du moteur", "Volumen del motor", "Volume motore"]),
    ("row.fullscreen", ["Full screen", "Vollbild", "Plein écran", "Pantalla completa", "Schermo intero"]),
    ("row.vsync", ["Vertical sync", "Vertikale Synchronisation", "Synchronisation verticale", "Sincronización vertical", "Sincronizzazione verticale"]),
    ("row.antialiasing", ["Anti-aliasing", "Kantenglättung", "Anticrénelage", "Antialiasing", "Anti-aliasing"]),
    ("row.fps_cap", ["Frame limit", "Bildratenbegrenzung", "Limite d'images", "Límite de fotogramas", "Limite fotogrammi"]),
    ("row.render_scale", ["Render scale", "Renderauflösung", "Échelle de rendu", "Escala de renderizado", "Scala di rendering"]),
    ("row.ui_scale", ["Text and HUD size", "Text- und HUD-Größe", "Taille du texte et du HUD", "Tamaño de texto y HUD", "Dimensione testo e HUD"]),
    ("row.tv_margin", ["TV-safe margin", "TV-Sicherheitsrand", "Marge TV", "Margen para TV", "Margine TV"]),
    ("row.fov", ["Field of view", "Sichtfeld", "Champ de vision", "Campo de visión", "Campo visivo"]),
    ("row.camera", ["Camera  (V / D-pad up while driving)", "Kamera  (V / Steuerkreuz hoch beim Fahren)", "Caméra  (V / croix haut en roulant)", "Cámara  (V / cruceta arriba al conducir)", "Camera  (V / croce su in guida)"]),
    ("row.show_fps", ["Show frame rate", "Bildrate anzeigen", "Afficher les images/s", "Mostrar fotogramas/s", "Mostra fotogrammi/s"]),
    ("row.units", ["Speed units", "Geschwindigkeitseinheit", "Unité de vitesse", "Unidad de velocidad", "Unità di velocità"]),
    ("row.minimap", ["Mini-map", "Minikarte", "Mini-carte", "Minimapa", "Minimappa"]),
    ("row.g_meter", ["G-meter", "G-Messer", "Accéléromètre", "Medidor G", "Misuratore G"]),
    ("row.countdown", ["Start countdown", "Startcountdown", "Compte à rebours", "Cuenta atrás", "Conto alla rovescia"]),
    ("row.colour_blind", ["Colour-blind colours", "Farben für Farbenblinde", "Couleurs pour daltoniens", "Colores para daltónicos", "Colori per daltonici"]),
    ("row.high_contrast", ["High-contrast HUD", "Kontrastreiches HUD", "HUD contrasté", "HUD de alto contraste", "HUD ad alto contrasto"]),
    ("row.reduced_motion", ["Reduced motion", "Weniger Bewegung", "Mouvements réduits", "Movimiento reducido", "Movimento ridotto"]),
    ("row.language", ["Language", "Sprache", "Langue", "Idioma", "Lingua"]),
    ("row.steering", ["Steering sensitivity", "Lenkempfindlichkeit", "Sensibilité de direction", "Sensibilidad de dirección", "Sensibilità sterzo"]),
    ("row.deadzone", ["Stick dead zone", "Stick-Totzone", "Zone morte du stick", "Zona muerta del stick", "Zona morta levetta"]),
    ("row.rumble", ["Rumble", "Vibration", "Vibrations", "Vibración", "Vibrazione"]),
    ("row.sticky", ["Tap to hold pedals", "Pedale per Tippen halten", "Pédales maintenues d'une pression", "Pedales fijos con un toque", "Pedali fissi con un tocco"]),
    ("row.assists", ["Beginner driving assists", "Fahrhilfen für Anfänger", "Aides au pilotage (Débutant)", "Ayudas de conducción (Principiante)", "Aiuti alla guida (Principiante)"]),
    ("row.online", ["World leaderboards", "Weltranglisten", "Classements mondiaux", "Clasificaciones mundiales", "Classifiche mondiali"]),
    ("row.name", ["Name  (type, or ← → for ideas)", "Name  (tippen, oder ← → für Ideen)", "Nom  (tapez, ou ← → pour des idées)", "Nombre  (escribe, o ← → para ideas)", "Nome  (scrivi, o ← → per idee)"]),
    ("row.country", ["Country", "Land", "Pays", "País", "Paese"]),
    ("row.pending", ["Laps waiting to upload", "Runden, die auf den Upload warten", "Tours en attente d'envoi", "Vueltas pendientes de subir", "Giri in attesa di invio"]),
    ("row.forget", ["Delete my online data", "Meine Online-Daten löschen", "Supprimer mes données en ligne", "Borrar mis datos en línea", "Elimina i miei dati online"]),
    ("row.defaults", ["Put every key back", "Alle Tasten zurücksetzen", "Rétablir toutes les touches", "Restaurar todas las teclas", "Ripristina tutti i tasti"]),
    ("value.always_full", ["Always full", "Immer ganz", "Toujours complet", "Siempre completa", "Sempre completo"]),
    ("value.short", ["Short on restart", "Kurz beim Neustart", "Court au redémarrage", "Corta al reiniciar", "Breve al riavvio"]),
    ("value.press", ["Press to delete", "Zum Löschen drücken", "Appuyer pour supprimer", "Pulsa para borrar", "Premi per eliminare"]),
    ("value.press_again", ["Press again to delete everything", "Nochmals drücken, um alles zu löschen", "Appuyer encore pour tout supprimer", "Pulsa otra vez para borrarlo todo", "Premi ancora per eliminare tutto"]),
    ("value.waiting", ["Press a key or button…  (Esc cancels)", "Taste drücken…  (Esc bricht ab)", "Appuyez sur une touche…  (Échap annule)", "Pulsa una tecla o botón…  (Esc cancela)", "Premi un tasto…  (Esc annulla)"]),
    ("value.automatic", ["Automatic", "Automatisch", "Automatique", "Automático", "Automatico"]),
    ("camera.chase", ["Chase", "Verfolger", "Poursuite", "Persecución", "Inseguimento"]),
    ("camera.near", ["Chase, close", "Verfolger, nah", "Poursuite, proche", "Persecución, cerca", "Inseguimento, vicino"]),
    ("camera.bonnet", ["Bonnet", "Motorhaube", "Capot", "Capó", "Cofano"]),
    ("act.throttle", ["Throttle", "Gas", "Accélérateur", "Acelerador", "Acceleratore"]),
    ("act.brake", ["Brake / reverse", "Bremse / rückwärts", "Frein / marche arrière", "Freno / marcha atrás", "Freno / retromarcia"]),
    ("act.left", ["Steer left", "Links lenken", "Tourner à gauche", "Girar a la izquierda", "Sterza a sinistra"]),
    ("act.right", ["Steer right", "Rechts lenken", "Tourner à droite", "Girar a la derecha", "Sterza a destra"]),
    ("act.handbrake", ["Handbrake", "Handbremse", "Frein à main", "Freno de mano", "Freno a mano"]),
    ("act.restart", ["Restart lap", "Runde neu starten", "Recommencer le tour", "Reiniciar vuelta", "Ricomincia giro"]),
    ("act.ghost", ["Ghost", "Geist", "Fantôme", "Fantasma", "Fantasma"]),
    ("act.camera", ["Camera", "Kamera", "Caméra", "Cámara", "Camera"]),
    ("act.board", ["Leaderboard", "Rangliste", "Classement", "Clasificación", "Classifica"]),
];

/// An achievement's name and what it asks for, in the language in force.
pub(crate) fn achievement(id: &str) -> Option<(&'static str, &'static str)> {
    let at = Language::ALL
        .iter()
        .position(|l| *l == current())
        .unwrap_or(0);
    ACHIEVEMENTS
        .iter()
        .find(|(i, _, _)| *i == id)
        .map(|(_, name, what)| (name[at], what[at]))
}

/// The leaderboard, replay, online, reset, medals, achievements, circuit menu
/// and garage.
#[rustfmt::skip]
pub(crate) const MORE: &[Entry] = &[
    // The leaderboard.
    ("board.label", ["TODORA  /  WORLD LEADERBOARD", "TODORA  /  WELTRANGLISTE", "TODORA  /  CLASSEMENT MONDIAL", "TODORA  /  CLASIFICACIÓN MUNDIAL", "TODORA  /  CLASSIFICA MONDIALE"]),
    ("view.world", ["World", "Welt", "Monde", "Mundo", "Mondo"]),
    ("view.country", ["Country", "Land", "Pays", "País", "Paese"]),
    ("view.rivals", ["Rivals", "Rivalen", "Rivaux", "Rivales", "Rivali"]),
    ("view.week", ["This week", "Diese Woche", "Cette semaine", "Esta semana", "Questa settimana"]),
    ("view.records", ["Records", "Rekorde", "Records", "Récords", "Record"]),
    ("view.awards", ["Awards", "Erfolge", "Succès", "Logros", "Traguardi"]),
    ("board.hint", [
        "← →  Circuit    ↑ ↓  Row    Q / E  or  LB / RB  View    A / Enter  Race ghost    X / P  Pin rival    Esc / B  Back",
        "← →  Strecke    ↑ ↓  Zeile    Q / E  oder  LB / RB  Ansicht    A / Enter  Gegen Geist fahren    X / P  Rivale merken    Esc / B  Zurück",
        "← →  Circuit    ↑ ↓  Ligne    Q / E  ou  LB / RB  Vue    A / Entrée  Affronter le fantôme    X / P  Épingler un rival    Échap / B  Retour",
        "← →  Circuito    ↑ ↓  Fila    Q / E  o  LB / RB  Vista    A / Intro  Correr contra el fantasma    X / P  Fijar rival    Esc / B  Volver",
        "← →  Circuito    ↑ ↓  Riga    Q / E  o  LB / RB  Vista    A / Invio  Sfida il fantasma    X / P  Fissa rivale    Esc / B  Indietro",
    ]),
    ("board.you", ["You #{} of {}  ·  {}  ·  record {} by {}  ·  {}", "Du #{} von {}  ·  {}  ·  Rekord {} von {}  ·  {}", "Vous #{} sur {}  ·  {}  ·  record {} par {}  ·  {}", "Tú #{} de {}  ·  {}  ·  récord {} de {}  ·  {}", "Tu #{} su {}  ·  {}  ·  record {} di {}  ·  {}"]),
    ("board.total", ["{} on the board  ·  record {} by {}", "{} in der Rangliste  ·  Rekord {} von {}", "{} au classement  ·  record {} par {}", "{} en la clasificación  ·  récord {} de {}", "{} in classifica  ·  record {} di {}"]),
    ("board.empty", ["Nobody has set a time here yet.", "Hier hat noch niemand eine Zeit gefahren.", "Personne n'a encore signé de temps ici.", "Nadie ha marcado un tiempo aquí todavía.", "Nessuno ha ancora fatto un tempo qui."]),
    ("board.offline_hint", ["Go online in Settings → Online to see the world boards.", "Geh unter Einstellungen → Online online, um die Weltranglisten zu sehen.", "Passez en ligne dans Réglages → En ligne pour voir les classements.", "Conéctate en Ajustes → En línea para ver las clasificaciones.", "Vai online in Impostazioni → Online per vedere le classifiche."]),
    ("board.pick_country", ["Pick your country in Settings → Online.", "Wähle dein Land unter Einstellungen → Online.", "Choisissez votre pays dans Réglages → En ligne.", "Elige tu país en Ajustes → En línea.", "Scegli il tuo paese in Impostazioni → Online."]),
    ("board.pin_hint", ["Pin up to five rivals with X / P on any board.", "Merke dir bis zu fünf Rivalen mit X / P auf jeder Rangliste.", "Épinglez jusqu'à cinq rivaux avec X / P sur n'importe quel classement.", "Fija hasta cinco rivales con X / P en cualquier clasificación.", "Fissa fino a cinque rivali con X / P su qualsiasi classifica."]),
    ("board.offline", ["Offline: as it stood {} ago.", "Offline: Stand von vor {}.", "Hors ligne : état d'il y a {}.", "Sin conexión: como estaba hace {}.", "Offline: com'era {} fa."]),
    ("board.loading", ["Loading…", "Lädt…", "Chargement…", "Cargando…", "Caricamento…"]),
    ("board.records_title", ["Records   ·   {}", "Rekorde   ·   {}", "Records   ·   {}", "Récords   ·   {}", "Record   ·   {}"]),
    ("board.medal_totals", ["{} author  ·  {} gold  ·  {} silver  ·  {} bronze  ·  {} to go", "{} Autor  ·  {} Gold  ·  {} Silber  ·  {} Bronze  ·  {} offen", "{} auteur  ·  {} or  ·  {} argent  ·  {} bronze  ·  {} à faire", "{} autor  ·  {} oro  ·  {} plata  ·  {} bronce  ·  {} pendientes", "{} autore  ·  {} oro  ·  {} argento  ·  {} bronzo  ·  {} da fare"]),
    ("board.place_of", ["#{} of {}", "#{} von {}", "#{} sur {}", "#{} de {}", "#{} su {}"]),
    ("board.awards_title", ["Awards   ·   {} of {}", "Erfolge   ·   {} von {}", "Succès   ·   {} sur {}", "Logros   ·   {} de {}", "Traguardi   ·   {} su {}"]),
    ("board.km", ["{} km driven  ·  earned ones are lit", "{} km gefahren  ·  erreichte leuchten", "{} km parcourus  ·  ceux obtenus sont allumés", "{} km recorridos  ·  los conseguidos se iluminan", "{} km percorsi  ·  quelli ottenuti sono accesi"]),
    ("board.week_title", ["This week   ·   {}   ·   Regular   ·   {} left", "Diese Woche   ·   {}   ·   Normal   ·   noch {}", "Cette semaine   ·   {}   ·   Normal   ·   encore {}", "Esta semana   ·   {}   ·   Normal   ·   quedan {}", "Questa settimana   ·   {}   ·   Normale   ·   mancano {}"]),
    ("board.week_best", ["Your best this week: {}    ·    D / Y  Drive it", "Deine Bestzeit diese Woche: {}    ·    D / Y  Losfahren", "Votre meilleur temps cette semaine : {}    ·    D / Y  Y aller", "Tu mejor tiempo esta semana: {}    ·    D / Y  Conducir", "Il tuo migliore questa settimana: {}    ·    D / Y  Guida"]),
    ("board.week_none", ["No lap this week yet.    ·    D / Y  Drive it", "Diese Woche noch keine Runde.    ·    D / Y  Losfahren", "Pas encore de tour cette semaine.    ·    D / Y  Y aller", "Aún no hay vuelta esta semana.    ·    D / Y  Conducir", "Ancora nessun giro questa settimana.    ·    D / Y  Guida"]),
    ("board.downloading", ["Downloading {}'s lap…", "Lade die Runde von {}…", "Téléchargement du tour de {}…", "Descargando la vuelta de {}…", "Scarico il giro di {}…"]),
    ("time.days", ["{} days", "{} Tage", "{} jours", "{} días", "{} giorni"]),
    ("time.hours", ["{} h", "{} Std.", "{} h", "{} h", "{} ore"]),
    ("time.minutes", ["{} min", "{} Min.", "{} min", "{} min", "{} min"]),
    // The replay.
    ("replay.best", ["Your best", "Deine Bestzeit", "Votre meilleur", "Tu mejor", "Il tuo migliore"]),
    ("replay.last", ["Your last lap", "Deine letzte Runde", "Votre dernier tour", "Tu última vuelta", "Il tuo ultimo giro"]),
    ("replay.downloaded", ["Downloaded ghost", "Geladener Geist", "Fantôme téléchargé", "Fantasma descargado", "Fantasma scaricato"]),
    ("replay.trackside", ["Trackside", "Streckenrand", "Bord de piste", "Pie de pista", "Bordo pista"]),
    ("replay.paused", ["PAUSED", "PAUSIERT", "EN PAUSE", "EN PAUSA", "IN PAUSA"]),
    ("replay.nothing", ["{}    — nothing to replay yet", "{}    — noch nichts zum Wiederholen", "{}    — rien à revoir pour l'instant", "{}    — aún no hay nada que repetir", "{}    — ancora niente da rivedere"]),
    ("replay.hint", [
        "A / Enter  Play    ← →  Scrub    ↑ ↓  Speed    Q / E  Lap    Y  Camera    X / P  Photo    Esc / B  Back",
        "A / Enter  Abspielen    ← →  Spulen    ↑ ↓  Tempo    Q / E  Runde    Y  Kamera    X / P  Foto    Esc / B  Zurück",
        "A / Entrée  Lire    ← →  Défiler    ↑ ↓  Vitesse    Q / E  Tour    Y  Caméra    X / P  Photo    Échap / B  Retour",
        "A / Intro  Reproducir    ← →  Avanzar    ↑ ↓  Velocidad    Q / E  Vuelta    Y  Cámara    X / P  Foto    Esc / B  Volver",
        "A / Invio  Riproduci    ← →  Scorri    ↑ ↓  Velocità    Q / E  Giro    Y  Camera    X / P  Foto    Esc / B  Indietro",
    ]),
    // Going online.
    ("online.label", ["TODORA  /  WORLD LEADERBOARDS", "TODORA  /  WELTRANGLISTEN", "TODORA  /  CLASSEMENTS MONDIAUX", "TODORA  /  CLASIFICACIONES MUNDIALES", "TODORA  /  CLASSIFICHE MONDIALI"]),
    ("online.ask", ["Put your laps on the world boards?", "Deine Runden in die Weltranglisten stellen?", "Publier vos tours dans les classements mondiaux ?", "¿Subir tus vueltas a las clasificaciones mundiales?", "Mettere i tuoi giri nelle classifiche mondiali?"]),
    ("online.explain", [
        "Your best laps go to todora.lukschander.com, hosted in the EU, and are driven again there before they count. Stored: a random id, the name below, a country if you pick one, and your laps. No email, no account, no address. Settings → Online changes the name or deletes everything.",
        "Deine besten Runden gehen an todora.lukschander.com in der EU und werden dort nachgefahren, bevor sie zählen. Gespeichert: eine zufällige Kennung, der Name unten, ein Land, falls du eines wählst, und deine Runden. Keine E-Mail, kein Konto, keine Adresse. Unter Einstellungen → Online änderst du den Namen oder löschst alles.",
        "Vos meilleurs tours partent vers todora.lukschander.com, hébergé dans l'UE, où ils sont rejoués avant de compter. Conservés : un identifiant aléatoire, le nom ci-dessous, un pays si vous en choisissez un, et vos tours. Ni e-mail, ni compte, ni adresse. Réglages → En ligne change le nom ou efface tout.",
        "Tus mejores vueltas van a todora.lukschander.com, alojado en la UE, donde se vuelven a correr antes de contar. Se guarda: un identificador aleatorio, el nombre de abajo, un país si eliges uno y tus vueltas. Sin correo, sin cuenta, sin dirección. En Ajustes → En línea cambias el nombre o lo borras todo.",
        "I tuoi giri migliori vanno a todora.lukschander.com, ospitato nell'UE, dove vengono rifatti prima di valere. Si salvano: un identificativo casuale, il nome qui sotto, un paese se lo scegli, e i tuoi giri. Niente e-mail, account o indirizzo. Impostazioni → Online cambia il nome o cancella tutto.",
    ]),
    ("online.appear", ["You would appear as  {}", "Du würdest erscheinen als  {}", "Vous apparaîtriez sous le nom  {}", "Aparecerías como  {}", "Appariresti come  {}"]),
    ("online.choose", ["A / Enter  Go online        B / Esc  Not now", "A / Enter  Online gehen        B / Esc  Nicht jetzt", "A / Entrée  Passer en ligne        B / Échap  Pas maintenant", "A / Intro  Conectarse        B / Esc  Ahora no", "A / Invio  Vai online        B / Esc  Non ora"]),
    ("online.on", ["On  ·  {}", "An  ·  {}", "Oui  ·  {}", "Sí  ·  {}", "Sì  ·  {}"]),
    ("online.offline", ["Offline  ·  {}", "Offline  ·  {}", "Hors ligne  ·  {}", "Sin conexión  ·  {}", "Offline  ·  {}"]),
    ("online.sent", ["{}: {} — world {}", "{}: {} — Welt {}", "{} : {} — monde {}", "{}: {} — mundo {}", "{}: {} — mondo {}"]),
    ("online.on_board", ["on the board", "in der Rangliste", "au classement", "en la clasificación", "in classifica"]),
    ("online.refused", ["{}: lap not accepted — {}", "{}: Runde nicht angenommen — {}", "{} : tour refusé — {}", "{}: vuelta no aceptada — {}", "{}: giro non accettato — {}"]),
    ("online.ghost", ["Ghost {} downloaded ({} KB)", "Geist {} geladen ({} KB)", "Fantôme {} téléchargé ({} Ko)", "Fantasma {} descargado ({} KB)", "Fantasma {} scaricato ({} KB)"]),
    ("online.deleted", ["Your online data has been deleted.", "Deine Online-Daten wurden gelöscht.", "Vos données en ligne ont été supprimées.", "Tus datos en línea se han borrado.", "I tuoi dati online sono stati eliminati."]),
    ("rival.racing", ["Racing {}'s ghost. G changes which ghosts show.", "Du fährst gegen den Geist von {}. G wählt, welche Geister zu sehen sind.", "Vous affrontez le fantôme de {}. G choisit les fantômes affichés.", "Corres contra el fantasma de {}. G cambia qué fantasmas se ven.", "Sfidi il fantasma di {}. G sceglie quali fantasmi mostrare."]),
    ("rival.someone", ["the downloaded lap", "der geladenen Runde", "le tour téléchargé", "la vuelta descargada", "il giro scaricato"]),
    ("rival.bad", ["That ghost did not replay to a lap here, so it is not raced.", "Dieser Geist ergibt hier keine Runde und wird nicht gefahren.", "Ce fantôme ne donne pas de tour ici ; il n'est pas affronté.", "Ese fantasma no da una vuelta aquí, así que no se corre.", "Quel fantasma qui non dà un giro, quindi non si sfida."]),
    ("rival.other", ["That ghost is for another circuit or mode; pick one on this board.", "Dieser Geist gehört zu einer anderen Strecke oder einem anderen Modus; wähle einen aus dieser Rangliste.", "Ce fantôme est pour un autre circuit ou mode ; choisissez-en un dans ce classement.", "Ese fantasma es de otro circuito o modo; elige uno de esta clasificación.", "Quel fantasma è di un altro circuito o modalità; scegline uno da questa classifica."]),
    // Resetting ghosts.
    ("clear.confirm_one", ["Confirm again to erase this circuit/mode’s ghost and best time. Your lap will restart.", "Nochmals bestätigen, um Geist und Bestzeit dieser Strecke/dieses Modus zu löschen. Die Runde startet neu.", "Confirmez à nouveau pour effacer le fantôme et le record de ce circuit/mode. Le tour recommencera.", "Confirma otra vez para borrar el fantasma y el récord de este circuito/modo. La vuelta se reiniciará.", "Conferma di nuovo per cancellare fantasma e record di questo circuito/modalità. Il giro ricomincerà."]),
    ("clear.confirm_all", ["Confirm again to erase every circuit/mode’s ghost and best time. Your lap will restart.", "Nochmals bestätigen, um alle Geister und Bestzeiten zu löschen. Die Runde startet neu.", "Confirmez à nouveau pour effacer tous les fantômes et records. Le tour recommencera.", "Confirma otra vez para borrar todos los fantasmas y récords. La vuelta se reiniciará.", "Conferma di nuovo per cancellare tutti i fantasmi e i record. Il giro ricomincerà."]),
    ("clear.done_one", ["This ghost and best time reset. Ready for a fresh lap.", "Geist und Bestzeit zurückgesetzt. Bereit für eine neue Runde.", "Fantôme et record effacés. Prêt pour un nouveau tour.", "Fantasma y récord borrados. Listo para una vuelta nueva.", "Fantasma e record azzerati. Pronto per un nuovo giro."]),
    ("clear.done_all", ["All ghosts and best times reset. Ready for a fresh lap.", "Alle Geister und Bestzeiten zurückgesetzt. Bereit für eine neue Runde.", "Tous les fantômes et records effacés. Prêt pour un nouveau tour.", "Todos los fantasmas y récords borrados. Listo para una vuelta nueva.", "Tutti i fantasmi e i record azzerati. Pronto per un nuovo giro."]),
    ("clear.fail_one", ["Could not reset this ghost. Check save-folder permissions and try again.", "Geist konnte nicht zurückgesetzt werden. Prüfe die Rechte des Speicherordners.", "Impossible d'effacer ce fantôme. Vérifiez les droits du dossier de sauvegarde.", "No se pudo borrar este fantasma. Revisa los permisos de la carpeta de guardado.", "Impossibile azzerare questo fantasma. Controlla i permessi della cartella."]),
    ("clear.fail_all", ["This ghost was reset, but some saved ghosts could not be removed. Check save-folder permissions and retry.", "Dieser Geist wurde zurückgesetzt, andere konnten nicht gelöscht werden. Prüfe die Rechte des Speicherordners.", "Ce fantôme est effacé, mais d'autres n'ont pas pu l'être. Vérifiez les droits du dossier.", "Este fantasma se borró, pero otros no. Revisa los permisos de la carpeta.", "Questo fantasma è azzerato, ma altri no. Controlla i permessi della cartella."]),
    // Medals.
    ("medal.bronze", ["bronze", "Bronze", "bronze", "bronce", "bronzo"]),
    ("medal.silver", ["silver", "Silber", "argent", "plata", "argento"]),
    ("medal.gold", ["gold", "Gold", "or", "oro", "oro"]),
    ("medal.author", ["author", "Autor", "auteur", "autor", "autore"]),
    ("medal.beaten", ["{} — author time beaten", "{} — Autorenzeit geschlagen", "{} — temps de l'auteur battu", "{} — tiempo del autor batido", "{} — tempo dell'autore battuto"]),
    ("medal.gap", ["{} — {} to {}", "{} — {} bis {}", "{} — {} pour {}", "{} — {} para {}", "{} — {} per {}"]),
    ("medal.short", ["{} to {}", "{} bis {}", "{} pour {}", "{} para {}", "{} per {}"]),
    ("medal.in", ["{} in {}", "{} in {}", "{} en {}", "{} en {}", "{} in {}"]),
    ("medal.provisional", ["  ·  provisional", "  ·  vorläufig", "  ·  provisoire", "  ·  provisional", "  ·  provvisorio"]),
    ("ach.toast", ["ACHIEVEMENT   ·   {}", "ERFOLG   ·   {}", "SUCCÈS   ·   {}", "LOGRO   ·   {}", "TRAGUARDO   ·   {}"]),
    ("ach.gold_at", ["Gold at {}", "Gold in {}", "L'or à {}", "Oro en {}", "Oro a {}"]),
    ("ach.gold_at_what", ["Earn gold at {}, in any mode", "Hol Gold in {}, in jedem Modus", "Décrochez l'or à {}, dans n'importe quel mode", "Consigue el oro en {}, en cualquier modo", "Conquista l'oro a {}, in qualsiasi modalità"]),
    // The circuit menu and garage.
    ("menu.free_drive", ["/   FREE DRIVE", "/   FREIES FAHREN", "/   PILOTAGE LIBRE", "/   CONDUCCIÓN LIBRE", "/   GUIDA LIBERA"]),
    ("menu.tabs", ["LB / RB  Tabs", "LB / RB  Reiter", "LB / RB  Onglets", "LB / RB  Pestañas", "LB / RB  Schede"]),
    ("menu.tab_circuits", ["01   Circuits", "01   Strecken", "01   Circuits", "01   Circuitos", "01   Circuiti"]),
    ("menu.tab_garage", ["02   Garage & setup", "02   Garage & Setup", "02   Garage et réglages", "02   Garaje y reglaje", "02   Garage e assetto"]),
    ("menu.close", ["Close  /  Esc", "Schließen  /  Esc", "Fermer  /  Échap", "Cerrar  /  Esc", "Chiudi  /  Esc"]),
    ("menu.circuits_title", ["Find your next lap.", "Finde deine nächste Runde.", "Trouvez votre prochain tour.", "Encuentra tu próxima vuelta.", "Trova il tuo prossimo giro."]),
    ("menu.circuits_sub", ["Pick a circuit. Learn the corners. Chase your best.", "Wähle eine Strecke. Lerne die Kurven. Jage deine Bestzeit.", "Choisissez un circuit. Apprenez les virages. Battez votre record.", "Elige un circuito. Aprende las curvas. Persigue tu mejor tiempo.", "Scegli un circuito. Impara le curve. Insegui il tuo record."]),
    ("menu.garage_title", ["Make it your drive.", "Mach ihn zu deinem.", "À votre façon.", "Hazlo a tu manera.", "Fallo a modo tuo."]),
    ("menu.garage_sub", ["Three different characters. One setup that feels right to you.", "Drei Charaktere. Ein Setup, das sich richtig anfühlt.", "Trois caractères. Un réglage qui vous va.", "Tres caracteres. Un reglaje a tu medida.", "Tre caratteri. Un assetto che ti sta bene."]),
    ("menu.circuits_keys", ["Stick / D-pad / Arrows  Browse     PgUp / PgDn  Page     A / Enter  Drive", "Stick / Steuerkreuz / Pfeile  Blättern     Bild↑ / Bild↓  Seite     A / Enter  Fahren", "Stick / Croix / Flèches  Parcourir     Pg↑ / Pg↓  Page     A / Entrée  Piloter", "Stick / Cruceta / Flechas  Explorar     RePág / AvPág  Página     A / Intro  Conducir", "Levetta / Croce / Frecce  Sfoglia     PagSu / PagGiù  Pagina     A / Invio  Guida"]),
    ("menu.garage_keys", ["↑↓  Car     ←→  Setup     Y / M  Mode     A / Enter  Apply", "↑↓  Auto     ←→  Setup     Y / M  Modus     A / Enter  Übernehmen", "↑↓  Voiture     ←→  Réglage     Y / M  Mode     A / Entrée  Appliquer", "↑↓  Coche     ←→  Reglaje     Y / M  Modo     A / Intro  Aplicar", "↑↓  Auto     ←→  Assetto     Y / M  Modalità     A / Invio  Applica"]),
    ("menu.circuits_note", ["Changing circuit starts a new session. Start / B / Esc returns to your current lap.", "Eine neue Strecke startet eine neue Sitzung. Start / B / Esc führt zurück zur Runde.", "Changer de circuit lance une nouvelle session. Start / B / Échap revient au tour en cours.", "Cambiar de circuito empieza una sesión nueva. Start / B / Esc vuelve a tu vuelta.", "Cambiare circuito avvia una nuova sessione. Start / B / Esc torna al giro in corso."]),
    ("menu.garage_note", ["Apply restarts the lap. Best times and ghosts are saved separately for each mode.", "Übernehmen startet die Runde neu. Bestzeiten und Geister gelten je Modus.", "Appliquer relance le tour. Records et fantômes sont gardés par mode.", "Aplicar reinicia la vuelta. Récords y fantasmas se guardan por modo.", "Applica riavvia il giro. Record e fantasmi sono salvati per modalità."]),
    ("menu.drive", ["Drive this circuit  →", "Diese Strecke fahren  →", "Piloter ce circuit  →", "Conducir este circuito  →", "Guida questo circuito  →"]),
    ("menu.apply", ["Apply & drive  →", "Übernehmen & fahren  →", "Appliquer et piloter  →", "Aplicar y conducir  →", "Applica e guida  →"]),
    ("menu.search_empty", ["Search circuits… just start typing", "Strecken suchen… einfach lostippen", "Chercher un circuit… tapez simplement", "Buscar circuitos… empieza a escribir", "Cerca circuiti… inizia a scrivere"]),
    ("menu.search", ["Search: {}", "Suche: {}", "Recherche : {}", "Búsqueda: {}", "Cerca: {}"]),
    ("menu.clear", ["Clear", "Leeren", "Effacer", "Borrar", "Cancella"]),
    ("menu.none_found", ["No circuits found", "Keine Strecken gefunden", "Aucun circuit trouvé", "No se encontraron circuitos", "Nessun circuito trovato"]),
    ("menu.none_hint", ["Try a shorter name or clear your search.", "Versuch einen kürzeren Namen oder leere die Suche.", "Essayez un nom plus court ou effacez la recherche.", "Prueba un nombre más corto o borra la búsqueda.", "Prova un nome più corto o cancella la ricerca."]),
    ("menu.current", ["CURRENT CIRCUIT", "AKTUELLE STRECKE", "CIRCUIT ACTUEL", "CIRCUITO ACTUAL", "CIRCUITO ATTUALE"]),
    ("menu.selected", ["SELECTED", "AUSGEWÄHLT", "SÉLECTIONNÉ", "SELECCIONADO", "SELEZIONATO"]),
    ("menu.circuit", ["CIRCUIT", "STRECKE", "CIRCUIT", "CIRCUITO", "CIRCUITO"]),
    ("menu.page", ["{}–{} of {} circuits  /  Page {} of {}", "{}–{} von {} Strecken  /  Seite {} von {}", "{}–{} sur {} circuits  /  Page {} sur {}", "{}–{} de {} circuitos  /  Página {} de {}", "{}–{} di {} circuiti  /  Pagina {} di {}"]),
    ("menu.zero", ["0 circuits", "0 Strecken", "0 circuit", "0 circuitos", "0 circuiti"]),
    ("menu.previous", ["← Previous", "← Zurück", "← Précédent", "← Anterior", "← Precedente"]),
    ("menu.next", ["Next →", "Weiter →", "Suivant →", "Siguiente →", "Successivo →"]),
    ("menu.brief", ["CIRCUIT BRIEF", "STRECKENINFO", "FICHE CIRCUIT", "FICHA DEL CIRCUITO", "SCHEDA CIRCUITO"]),
    ("menu.brief_empty", ["A new favourite is out there.", "Irgendwo wartet eine neue Lieblingsstrecke.", "Un nouveau favori vous attend.", "Hay un nuevo favorito esperándote.", "Un nuovo preferito ti aspetta."]),
    ("menu.brief_hint", ["Search the circuit library to find your next drive.", "Durchsuche die Strecken nach deiner nächsten Fahrt.", "Parcourez les circuits pour trouver votre prochaine course.", "Busca en los circuitos tu próxima conducción.", "Cerca tra i circuiti la tua prossima guida."]),
    ("menu.outline", ["CIRCUIT OUTLINE   /   START IN LIME", "STRECKENVERLAUF   /   START IN GRÜN", "TRACÉ   /   DÉPART EN VERT", "TRAZADO   /   SALIDA EN VERDE", "TRACCIATO   /   PARTENZA IN VERDE"]),
    ("menu.in_game_lap", ["IN-GAME LAP", "RUNDENLÄNGE", "LONGUEUR DU TOUR", "LONGITUD DE VUELTA", "LUNGHEZZA GIRO"]),
    ("menu.your_best", ["YOUR BEST", "DEINE BESTZEIT", "VOTRE RECORD", "TU MEJOR", "IL TUO MIGLIORE"]),
    ("menu.driving", ["DRIVING", "FAHREN", "CONDUITE", "CONDUCCIÓN", "GUIDA"]),
    ("menu.handling_setup", ["HANDLING SETUP", "FAHRWERK", "RÉGLAGE DU COMPORTEMENT", "REGLAJE DEL COMPORTAMIENTO", "ASSETTO"]),
    ("menu.driving_mode", ["DRIVING MODE", "FAHRMODUS", "MODE DE CONDUITE", "MODO DE CONDUCCIÓN", "MODALITÀ DI GUIDA"]),
    ("menu.setup_keys", ["1 / 2 / 3 changes handling while driving.", "1 / 2 / 3 ändert das Fahrwerk während der Fahrt.", "1 / 2 / 3 change le réglage en roulant.", "1 / 2 / 3 cambia el reglaje al conducir.", "1 / 2 / 3 cambia l'assetto in guida."]),
    ("menu.circuits_key", ["T   Circuits", "T   Strecken", "T   Circuits", "T   Circuitos", "T   Circuiti"]),
    ("menu.garage_key", ["C   Garage & setup", "C   Garage & Setup", "C   Garage et réglages", "C   Garaje y reglaje", "C   Garage e assetto"]),
    ("setup.stable", ["Stable", "Stabil", "Stable", "Estable", "Stabile"]),
    ("setup.balanced", ["Balanced", "Ausgewogen", "Équilibré", "Equilibrado", "Bilanciato"]),
    ("setup.loose", ["Loose", "Lebhaft", "Vif", "Suelto", "Vivace"]),
    ("setup.stable_about", ["Gentler rotation. Easier to catch.", "Sanfteres Einlenken. Leichter abzufangen.", "Rotation plus douce. Plus facile à rattraper.", "Giro más suave. Más fácil de recoger.", "Rotazione più dolce. Più facile da riprendere."]),
    ("setup.balanced_about", ["The car’s natural handling.", "Das natürliche Fahrverhalten des Autos.", "Le comportement naturel de la voiture.", "El comportamiento natural del coche.", "Il comportamento naturale dell'auto."]),
    ("setup.loose_about", ["Eager rotation. Holds a slide longer.", "Williges Einlenken. Hält den Drift länger.", "Rotation vive. Tient la glisse plus longtemps.", "Giro rápido. Aguanta más el derrape.", "Rotazione pronta. Tiene la derapata più a lungo."]),
    ("mode.beginner", ["Beginner", "Anfänger", "Débutant", "Principiante", "Principiante"]),
    ("mode.regular", ["Regular", "Normal", "Normal", "Normal", "Normale"]),
    ("mode.pro", ["Pro", "Profi", "Pro", "Pro", "Pro"]),
    ("mode.minus", ["−20% speed", "−20% Tempo", "−20% de vitesse", "−20% de velocidad", "−20% di velocità"]),
    ("mode.same", ["100% speed", "100% Tempo", "100% de vitesse", "100% de velocidad", "100% di velocità"]),
    ("mode.plus", ["+20% speed", "+20% Tempo", "+20% de vitesse", "+20% de velocidad", "+20% di velocità"]),
    ("car.omarchy_tag", ["LE MANS 2014 · GTE AM", "LE MANS 2014 · GTE AM", "LE MANS 2014 · GTE AM", "LE MANS 2014 · GTE AM", "LE MANS 2014 · GTE AM"]),
    ("car.omarchy_about", ["DHH’s class-winning Omarchy GT.", "Der klassensiegreiche Omarchy GT von DHH.", "L'Omarchy GT de DHH, victorieuse de sa catégorie.", "El Omarchy GT de DHH, ganador de su clase.", "L'Omarchy GT di DHH, vincitrice di classe."]),
    ("car.clubman_tag", ["THE CORNER CARVER", "DER KURVENKÜNSTLER", "L'AS DES VIRAGES", "EL REY DE LAS CURVAS", "IL RE DELLE CURVE"]),
    ("car.clubman_about", ["More grip. More confidence through every bend.", "Mehr Grip. Mehr Vertrauen in jeder Kurve.", "Plus d'adhérence. Plus d'assurance dans chaque virage.", "Más agarre. Más confianza en cada curva.", "Più aderenza. Più fiducia in ogni curva."]),
    ("car.express_tag", ["THE STRAIGHT-LINE SPECIALIST", "DER GERADEAUS-SPEZIALIST", "LE SPÉCIALISTE DES LIGNES DROITES", "EL ESPECIALISTA EN RECTAS", "LO SPECIALISTA DEI RETTILINEI"]),
    ("car.express_about", ["Long legs on the straights. A lighter hold in the corners.", "Lange Beine auf den Geraden. Leichterer Halt in den Kurven.", "De l'allonge en ligne droite. Moins d'appui en virage.", "Mucha punta en las rectas. Menos agarre en las curvas.", "Allungo sui rettilinei. Meno tenuta in curva."]),
    ("car.handling", ["HANDLING", "HANDLING", "TENUE", "MANEJO", "GUIDABILITÀ"]),
    ("car.accel", ["ACCEL", "BESCHL.", "ACCÉL.", "ACEL.", "ACCEL."]),
    ("car.top", ["TOP", "SPITZE", "V. MAX", "PUNTA", "PUNTA"]),
];

/// The achievements' names and what each asks for, by id.
#[rustfmt::skip]
pub(crate) const ACHIEVEMENTS: &[(&str, [&str; 5], [&str; 5])] = &[
    ("first-lap", ["Off the line", "Losgefahren", "C'est parti", "En marcha", "Si parte"], ["Finish a lap", "Beende eine Runde", "Terminez un tour", "Termina una vuelta", "Completa un giro"]),
    ("first-valid", ["By the book", "Nach Vorschrift", "Dans les règles", "Según el reglamento", "Da regolamento"], ["Finish a lap that counts", "Beende eine gültige Runde", "Terminez un tour valable", "Termina una vuelta válida", "Completa un giro valido"]),
    ("first-best", ["Personal best", "Persönliche Bestzeit", "Record personnel", "Mejor marca personal", "Record personale"], ["Beat your own best lap", "Schlag deine eigene Bestzeit", "Battez votre meilleur tour", "Supera tu mejor vuelta", "Batti il tuo giro migliore"]),
    ("circuits-5", ["Tourist", "Tourist", "Touriste", "Turista", "Turista"], ["Finish a lap on 5 circuits", "Beende eine Runde auf 5 Strecken", "Terminez un tour sur 5 circuits", "Termina una vuelta en 5 circuitos", "Completa un giro su 5 circuiti"]),
    ("circuits-20", ["Travelling circus", "Wanderzirkus", "Cirque itinérant", "Circo ambulante", "Circo itinerante"], ["Finish a lap on 20 circuits", "Beende eine Runde auf 20 Strecken", "Terminez un tour sur 20 circuits", "Termina una vuelta en 20 circuitos", "Completa un giro su 20 circuiti"]),
    ("circuits-40", ["The whole calendar", "Der ganze Kalender", "Tout le calendrier", "Todo el calendario", "Tutto il calendario"], ["Finish a lap on all 40 circuits", "Beende eine Runde auf allen 40 Strecken", "Terminez un tour sur les 40 circuits", "Termina una vuelta en los 40 circuitos", "Completa un giro su tutti i 40 circuiti"]),
    ("bronze", ["On the podium", "Auf dem Podium", "Sur le podium", "En el podio", "Sul podio"], ["Earn a bronze medal", "Hol eine Bronzemedaille", "Décrochez une médaille de bronze", "Consigue una medalla de bronce", "Conquista una medaglia di bronzo"]),
    ("silver", ["Silverware", "Silberwaren", "Argenterie", "Platería", "Argenteria"], ["Earn a silver medal", "Hol eine Silbermedaille", "Décrochez une médaille d'argent", "Consigue una medalla de plata", "Conquista una medaglia d'argento"]),
    ("gold", ["Gold", "Gold", "L'or", "Oro", "Oro"], ["Earn a gold medal", "Hol eine Goldmedaille", "Décrochez une médaille d'or", "Consigue una medalla de oro", "Conquista una medaglia d'oro"]),
    ("author", ["Author, author", "Der Autor", "L'auteur", "El autor", "L'autore"], ["Beat an author time", "Schlag eine Autorenzeit", "Battez un temps de l'auteur", "Supera un tiempo del autor", "Batti un tempo dell'autore"]),
    ("gold-5", ["Five golds", "Fünfmal Gold", "Cinq médailles d'or", "Cinco oros", "Cinque ori"], ["Gold on 5 circuits in one mode", "Gold auf 5 Strecken in einem Modus", "L'or sur 5 circuits dans un mode", "Oro en 5 circuitos en un modo", "Oro su 5 circuiti in una modalità"]),
    ("gold-20", ["Twenty golds", "Zwanzigmal Gold", "Vingt médailles d'or", "Veinte oros", "Venti ori"], ["Gold on 20 circuits in one mode", "Gold auf 20 Strecken in einem Modus", "L'or sur 20 circuits dans un mode", "Oro en 20 circuitos en un modo", "Oro su 20 circuiti in una modalità"]),
    ("gold-40", ["Midas", "Midas", "Midas", "Midas", "Mida"], ["Gold on every circuit in one mode", "Gold auf jeder Strecke in einem Modus", "L'or sur tous les circuits dans un mode", "Oro en todos los circuitos en un modo", "Oro su ogni circuito in una modalità"]),
    ("author-5", ["Ghost writer", "Ghostwriter", "Prête-plume", "Escritor fantasma", "Ghostwriter"], ["Beat 5 author times", "Schlag 5 Autorenzeiten", "Battez 5 temps de l'auteur", "Supera 5 tiempos del autor", "Batti 5 tempi dell'autore"]),
    ("pro", ["Pro", "Profi", "Pro", "Pro", "Pro"], ["Finish a lap that counts in Pro", "Beende eine gültige Runde im Profi-Modus", "Terminez un tour valable en Pro", "Termina una vuelta válida en Pro", "Completa un giro valido in Pro"]),
    ("beginner", ["Everyone starts somewhere", "Jeder fängt mal an", "Il faut bien commencer", "Todos empiezan en algún sitio", "Tutti iniziano da qualche parte"], ["Finish a lap that counts in Beginner", "Beende eine gültige Runde im Anfängermodus", "Terminez un tour valable en Débutant", "Termina una vuelta válida en Principiante", "Completa un giro valido in Principiante"]),
    ("modes", ["Three speeds", "Drei Tempi", "Trois vitesses", "Tres velocidades", "Tre velocità"], ["Laps that count in all three modes on one circuit", "Gültige Runden in allen drei Modi auf einer Strecke", "Des tours valables dans les trois modes sur un circuit", "Vueltas válidas en los tres modos en un circuito", "Giri validi in tutte e tre le modalità su un circuito"]),
    ("cars", ["Garage tour", "Garagenrundgang", "Tour du garage", "Vuelta por el garaje", "Giro del garage"], ["Laps that count in all three cars", "Gültige Runden mit allen drei Autos", "Des tours valables avec les trois voitures", "Vueltas válidas con los tres coches", "Giri validi con tutte e tre le auto"]),
    ("clean-5", ["Consistent", "Beständig", "Régulier", "Constante", "Costante"], ["Five laps in a row that count", "Fünf gültige Runden in Folge", "Cinq tours valables d'affilée", "Cinco vueltas válidas seguidas", "Cinque giri validi di fila"]),
    ("clean-10", ["Metronome", "Metronom", "Métronome", "Metrónomo", "Metronomo"], ["Ten laps in a row that count", "Zehn gültige Runden in Folge", "Dix tours valables d'affilée", "Diez vueltas válidas seguidas", "Dieci giri validi di fila"]),
    ("purple", ["Purple patch", "Lila Phase", "Passe violette", "Racha morada", "Momento viola"], ["Drive a sector faster than ever", "Fahr einen Sektor so schnell wie nie", "Signez votre meilleur temps dans un secteur", "Haz un sector más rápido que nunca", "Fai un settore più veloce che mai"]),
    ("all-purple", ["Purple reign", "Alles lila", "Règne violet", "Reinado morado", "Regno viola"], ["Every sector of a lap purple", "Jeder Sektor einer Runde lila", "Tous les secteurs d'un tour en violet", "Todos los sectores de una vuelta en morado", "Tutti i settori di un giro in viola"]),
    ("km-10", ["Warm-up", "Aufwärmen", "Échauffement", "Calentamiento", "Riscaldamento"], ["Drive 10 km", "Fahr 10 km", "Parcourez 10 km", "Recorre 10 km", "Percorri 10 km"]),
    ("km-100", ["Long run", "Langer Stint", "Long relais", "Tanda larga", "Stint lungo"], ["Drive 100 km", "Fahr 100 km", "Parcourez 100 km", "Recorre 100 km", "Percorri 100 km"]),
    ("km-1000", ["Endurance", "Langstrecke", "Endurance", "Resistencia", "Endurance"], ["Drive 1,000 km", "Fahr 1.000 km", "Parcourez 1 000 km", "Recorre 1.000 km", "Percorri 1.000 km"]),
    ("online", ["Hello, world", "Hallo, Welt", "Bonjour le monde", "Hola, mundo", "Ciao, mondo"], ["Put a lap on the world boards", "Bring eine Runde in die Weltrangliste", "Publiez un tour au classement mondial", "Sube una vuelta a la clasificación mundial", "Metti un giro nella classifica mondiale"]),
    ("top-100", ["Top hundred", "Top hundert", "Top cent", "Top cien", "Top cento"], ["Place in the top 100 of a board", "Platz unter den besten 100 einer Rangliste", "Entrez dans le top 100 d'un classement", "Entra en el top 100 de una clasificación", "Entra nei primi 100 di una classifica"]),
    ("top-10", ["Top ten", "Top zehn", "Top dix", "Top diez", "Top dieci"], ["Place in the top 10 of a board", "Platz unter den besten 10 einer Rangliste", "Entrez dans le top 10 d'un classement", "Entra en el top 10 de una clasificación", "Entra nei primi 10 di una classifica"]),
    ("record", ["World record", "Weltrekord", "Record du monde", "Récord mundial", "Record mondiale"], ["Set a world record", "Stell einen Weltrekord auf", "Établissez un record du monde", "Bate un récord mundial", "Stabilisci un record mondiale"]),
    ("ghost-beaten", ["Ghostbuster", "Geisterjäger", "Chasseur de fantômes", "Cazafantasmas", "Acchiappafantasmi"], ["Beat a downloaded ghost's lap", "Schlag die Runde eines geladenen Geists", "Battez le tour d'un fantôme téléchargé", "Supera la vuelta de un fantasma descargado", "Batti il giro di un fantasma scaricato"]),
    ("weekly", ["This week's", "Die Woche", "Celui de la semaine", "El de la semana", "Quello della settimana"], ["Set a time in the weekly challenge", "Fahr eine Zeit in der Wochen-Herausforderung", "Signez un temps dans le défi de la semaine", "Marca un tiempo en el reto semanal", "Fai un tempo nella sfida settimanale"]),
    ("monaco-clean", ["Harbour master", "Hafenmeister", "Capitaine du port", "Capitán del puerto", "Capitano del porto"], ["A lap of Monaco that counts", "Eine gültige Runde in Monaco", "Un tour valable à Monaco", "Una vuelta válida en Mónaco", "Un giro valido a Monaco"]),
    ("suzuka-clean", ["Figure of eight", "Achterbahn", "Le huit", "El ocho", "L'otto"], ["A lap of Suzuka that counts", "Eine gültige Runde in Suzuka", "Un tour valable à Suzuka", "Una vuelta válida en Suzuka", "Un giro valido a Suzuka"]),
    ("baku-clean", ["Old town", "Altstadt", "Vieille ville", "Casco antiguo", "Città vecchia"], ["A lap of Baku that counts", "Eine gültige Runde in Baku", "Un tour valable à Bakou", "Una vuelta válida en Bakú", "Un giro valido a Baku"]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_achievement_is_named_in_every_language() {
        for (id, names, whats) in ACHIEVEMENTS {
            assert!(names.iter().chain(whats).all(|w| !w.is_empty()), "{id}");
        }
    }

    #[test]
    fn every_key_is_there_once_in_every_language() {
        let mut seen = std::collections::HashSet::new();
        for (key, words) in TABLE.iter().chain(MORE) {
            assert!(seen.insert(*key), "{key} twice");
            for (language, word) in Language::ALL.iter().zip(words) {
                assert!(!word.is_empty(), "{key} has no {language:?}");
                let holes = |w: &str| w.matches("{}").count();
                assert_eq!(
                    holes(word),
                    holes(words[0]),
                    "{key} in {language:?} fills in a different number"
                );
            }
        }
    }

    /// Through the pure lookups: the language in force is global, and other
    /// tests read it at the same time.
    #[test]
    fn a_translation_is_looked_up_filled_in_and_falls_back() {
        assert_eq!(t_in(Language::De, "pause.resume"), "Weiter");
        assert_eq!(
            fill(t_in(Language::De, "card.to_best"), &[&"+0.18"]),
            "+0.18 zur Bestzeit"
        );
        assert_eq!(t_in(Language::En, "pause.resume"), "Resume");
        assert_eq!(t_in(Language::It, "no.such.key"), "?");
        assert_eq!(Language::from_code("de_AT.UTF-8"), Some(Language::De));
        assert_eq!(Language::from_code("pt-BR"), None);
    }

    #[test]
    fn no_word_is_more_than_twice_as_long_as_the_english() {
        for (key, words) in TABLE.iter().chain(MORE) {
            let english = words[0].chars().count().max(8);
            for word in &words[1..] {
                assert!(
                    word.chars().count() <= english * 2 + 6,
                    "{key}: {word:?} will not fit where {:?} does",
                    words[0]
                );
            }
        }
    }
}
