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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_there_once_in_every_language() {
        let mut seen = std::collections::HashSet::new();
        for (key, words) in TABLE {
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
        for (key, words) in TABLE {
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
