//! Réglages de l'utilisateur (fenêtre « Paramètres », Ctrl+,) et raccourcis clavier modifiables.
//! Enregistrés dans settings.json, à côté de session.json.
use eframe::egui::{self, Key, Modifiers};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    // --- apparence
    /// taille de toute l'interface (1.0 = 100 %)
    pub ui_scale: f32,
    /// fichier de police du code (vide = choix automatique, comme la version Python)
    pub code_font: String,
    /// fichier de police de l'interface (vide = police du système)
    pub ui_font: String,
    pub show_statusbar: bool,
    /// boutons de la barre d'outils, dans l'ordre (« | » = séparateur)
    pub toolbar_order: Vec<String>,
    /// boutons masqués de la barre d'outils (identifiants)
    pub toolbar_hidden: Vec<String>,
    // --- disposition
    pub side_right: bool,
    pub bottom_right: bool,
    pub debug_left: bool,
    /// ordre des onglets du panneau latéral (« files », « ssh », « vpn »)
    pub side_order: Vec<String>,
    /// ordre des groupes d'onglets du bas (« out », « repl », « term », « ssh »)
    pub bottom_order: Vec<String>,
    // --- éditeur
    pub tab_width: usize,
    pub use_spaces: bool,
    pub line_numbers: bool,
    pub indent_guides: bool,
    pub caret_line: bool,
    pub bracket_match: bool,
    pub folding: bool,
    pub word_wrap: bool,
    pub auto_indent: bool,
    pub close_brackets: bool,
    /// 0 : jamais ; 1 : en changeant d'onglet ; 2 : une seconde après la dernière frappe
    pub autosave: u8,
    pub trim_on_save: bool,
    // --- terminal
    /// shell des terminaux (vide = automatique)
    pub shell: String,
    /// taille du texte des terminaux (0 = comme l'éditeur)
    pub term_font_size: f32,
    pub scrollback: usize,
    // --- exécution
    /// interpréteur Python (vide = automatique)
    pub python: String,
    pub clear_output: bool,
    pub save_before_run: bool,
    // --- raccourcis : action → touches (« Ctrl+Maj+S »), vide = aucun
    pub keys: BTreeMap<String, String>,
}

impl Settings {
    pub fn editor_opts(&self) -> crate::editor::EditorOpts {
        crate::editor::EditorOpts {
            tab_width: self.tab_width.clamp(1, 16), use_spaces: self.use_spaces, line_numbers: self.line_numbers,
            indent_guides: self.indent_guides, caret_line: self.caret_line, bracket_match: self.bracket_match,
            folding: self.folding, word_wrap: self.word_wrap, auto_indent: self.auto_indent, close_brackets: self.close_brackets,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ui_scale: 1.0,
            code_font: String::new(),
            ui_font: String::new(),
            show_statusbar: true,
            toolbar_order: TOOLBAR_DEFAULT.iter().map(|s| s.to_string()).collect(),
            toolbar_hidden: vec!["open_file".into(), "save_all".into(), "settings".into()],
            side_right: false,
            bottom_right: false,
            debug_left: false,
            side_order: vec!["files".into(), "ssh".into(), "vpn".into()],
            bottom_order: vec!["out".into(), "repl".into(), "term".into(), "ssh".into()],
            tab_width: 4,
            use_spaces: true,
            line_numbers: true,
            indent_guides: true,
            caret_line: true,
            bracket_match: true,
            folding: true,
            word_wrap: false,
            auto_indent: true,
            close_brackets: false,
            autosave: 0,
            trim_on_save: false,
            shell: String::new(),
            term_font_size: 0.0,
            scrollback: 5000,
            python: String::new(),
            clear_output: true,
            save_before_run: true,
            keys: BTreeMap::new(),
        }
    }
}

/// Réglages lus ailleurs que dans l'interface (lancement de Python, des terminaux).
#[derive(Default, Clone)]
pub struct Globals {
    pub python: String,
    pub shell: String,
    pub scrollback: usize,
}

static GLOBALS: std::sync::RwLock<Option<Globals>> = std::sync::RwLock::new(None);

pub fn set_globals(s: &Settings) {
    if let Ok(mut g) = GLOBALS.write() {
        *g = Some(Globals { python: s.python.trim().to_string(), shell: s.shell.trim().to_string(), scrollback: s.scrollback });
    }
}

pub fn globals() -> Globals {
    GLOBALS.read().ok().and_then(|g| g.clone()).unwrap_or(Globals { scrollback: 5000, ..Default::default() })
}

/// Boutons possibles de la barre d'outils (identifiant, nom affiché).
pub const TOOLBAR: &[(&str, &str)] = &[
    ("new", "Nouveau fichier"), ("open", "Ouvrir un dossier"), ("open_file", "Ouvrir un fichier"), ("save", "Enregistrer"),
    ("save_all", "Tout enregistrer"), ("run", "Exécuter"), ("debug", "Déboguer / continuer"),
    ("steps", "Pas à pas (pendant le débogage)"), ("stop", "Arrêter"), ("bp", "Point d'arrêt"), ("libs", "Bibliothèques Python"),
    ("search", "Rechercher"), ("terminal", "Terminal"), ("split_r", "Scinder à droite"), ("split_d", "Scinder en bas"),
    ("web", "Pages web"), ("ssh", "Connexion SSH"), ("vpn", "VPN"), ("settings", "Paramètres"),
];

/// Ordre de la version Python.
pub const TOOLBAR_DEFAULT: &[&str] = &[
    "new", "open", "open_file", "save", "save_all", "|", "run", "debug", "steps", "stop", "bp", "|", "libs", "search", "terminal", "|",
    "split_r", "split_d", "web", "ssh", "vpn", "settings",
];

fn file() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("", "", "StudentCode").map(|d| d.config_dir().join("settings.json"))
}

impl Settings {
    pub fn load() -> Settings {
        let mut s: Settings = file().and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        s.fix();
        set_globals(&s);
        s
    }

    pub fn path() -> Option<std::path::PathBuf> { file() }

    pub fn save(&self) {
        set_globals(self);
        if let Some(f) = file() {
            if let Some(d) = f.parent() { let _ = std::fs::create_dir_all(d); }
            if let Ok(t) = serde_json::to_string_pretty(self) { let _ = std::fs::write(f, t); }
        }
    }

    /// Valeurs hors limites (fichier modifié à la main) ramenées dans les bornes.
    pub fn fix(&mut self) {
        self.ui_scale = if self.ui_scale.is_finite() { self.ui_scale.clamp(0.6, 2.5) } else { 1.0 };
        // barre d'outils : identifiants connus, chaque bouton une seule fois, les nouveaux à la fin
        let mut seen: Vec<String> = Vec::new();
        self.toolbar_order.retain(|x| x == "|" || (TOOLBAR.iter().any(|(id, _)| id == x) && !seen.contains(x) && { seen.push(x.clone()); true }));
        for (id, _) in TOOLBAR { if !self.toolbar_order.iter().any(|x| x == id) { self.toolbar_order.push(id.to_string()); } }
        self.tab_width = self.tab_width.clamp(1, 12);
        self.scrollback = self.scrollback.clamp(100, 100_000);
        self.term_font_size = if self.term_font_size.is_finite() { self.term_font_size.clamp(0.0, 40.0) } else { 0.0 };
        if self.autosave > 2 { self.autosave = 0; }
        for (list, all) in [(&mut self.side_order, &["files", "ssh", "vpn"][..]), (&mut self.bottom_order, &["out", "repl", "term", "ssh"][..])] {
            list.retain(|x| all.contains(&x.as_str()));
            let mut seen = Vec::new();
            list.retain(|x| if seen.contains(x) { false } else { seen.push(x.clone()); true });
            for a in all { if !list.iter().any(|x| x == a) { list.push(a.to_string()); } }
        }
    }

    /// Touches d'une action (une ou plusieurs, séparées par « ou ») : celles choisies par
    /// l'utilisateur, sinon celles par défaut.
    pub fn shortcuts(&self, a: Action) -> Vec<Shortcut> {
        let s = self.keys.get(a.id()).map(|s| s.as_str()).unwrap_or(a.default_keys());
        s.split(" ou ").filter_map(Shortcut::parse).collect()
    }

    pub fn shortcut(&self, a: Action) -> Option<Shortcut> {
        self.shortcuts(a).into_iter().next()
    }

    /// Texte du premier raccourci (menus, bulles d'aide).
    pub fn shortcut_text(&self, a: Action) -> String {
        self.shortcut(a).map(|s| s.to_string()).unwrap_or_default()
    }

    /// Tous les raccourcis de l'action (fenêtre Paramètres).
    pub fn shortcuts_text(&self, a: Action) -> String {
        self.shortcuts(a).iter().map(|s| s.to_string()).collect::<Vec<_>>().join(" ou ")
    }
}

/// Actions qui ont un raccourci modifiable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    New, OpenFile, OpenFolder, Save, SaveAs, SaveAll, CloseTab, Quit,
    Find, Goto, Comment, DuplicateLine,
    Run, Debug, Stop, StepOver, StepInto, StepOut, Breakpoint,
    ToggleSide, ToggleBottom, Terminal, NewTerminal, SplitRight, NextTab, PrevTab,
    ZoomIn, ZoomOut, ZoomReset, Libs, Ssh, Vpn, Settings, Menu,
}

pub const ACTIONS: &[Action] = &[
    Action::New, Action::OpenFile, Action::OpenFolder, Action::Save, Action::SaveAs, Action::SaveAll, Action::CloseTab, Action::Quit,
    Action::Find, Action::Goto, Action::Comment, Action::DuplicateLine,
    Action::Run, Action::Debug, Action::Stop, Action::StepOver, Action::StepInto, Action::StepOut, Action::Breakpoint,
    Action::ToggleSide, Action::ToggleBottom, Action::Terminal, Action::NewTerminal, Action::SplitRight, Action::NextTab, Action::PrevTab,
    Action::ZoomIn, Action::ZoomOut, Action::ZoomReset, Action::Libs, Action::Ssh, Action::Vpn, Action::Settings, Action::Menu,
];

impl Action {
    pub fn id(self) -> &'static str {
        use Action::*;
        match self {
            New => "new", OpenFile => "open_file", OpenFolder => "open_folder", Save => "save", SaveAs => "save_as",
            SaveAll => "save_all", CloseTab => "close_tab", Quit => "quit", Find => "find", Goto => "goto",
            Comment => "comment", DuplicateLine => "duplicate", Run => "run", Debug => "debug", Stop => "stop",
            StepOver => "step_over", StepInto => "step_into", StepOut => "step_out", Breakpoint => "breakpoint",
            ToggleSide => "toggle_side", ToggleBottom => "toggle_bottom", Terminal => "terminal",
            NewTerminal => "new_terminal", SplitRight => "split_right", NextTab => "next_tab", PrevTab => "prev_tab",
            ZoomIn => "zoom_in", ZoomOut => "zoom_out", ZoomReset => "zoom_reset", Libs => "libs", Ssh => "ssh",
            Vpn => "vpn", Settings => "settings", Menu => "menu",
        }
    }

    pub fn label(self) -> &'static str {
        use Action::*;
        match self {
            New => "Nouveau fichier", OpenFile => "Ouvrir un fichier", OpenFolder => "Ouvrir un dossier",
            Save => "Enregistrer", SaveAs => "Enregistrer sous", SaveAll => "Tout enregistrer", CloseTab => "Fermer l'onglet",
            Quit => "Quitter", Find => "Rechercher / Remplacer", Goto => "Aller à la ligne", Comment => "Commenter / décommenter",
            DuplicateLine => "Dupliquer la ligne", Run => "Exécuter", Debug => "Déboguer / continuer", Stop => "Arrêter",
            StepOver => "Pas à pas (par-dessus)", StepInto => "Entrer dans la fonction", StepOut => "Sortir de la fonction",
            Breakpoint => "Point d'arrêt", ToggleSide => "Afficher / masquer le panneau latéral",
            ToggleBottom => "Afficher / masquer le panneau du bas", Terminal => "Aller au terminal",
            NewTerminal => "Nouveau terminal", SplitRight => "Scinder à droite", NextTab => "Onglet suivant",
            PrevTab => "Onglet précédent", ZoomIn => "Agrandir le texte", ZoomOut => "Réduire le texte",
            ZoomReset => "Taille du texte par défaut", Libs => "Bibliothèques Python", Ssh => "Connexion SSH", Vpn => "VPN",
            Settings => "Paramètres", Menu => "Menu",
        }
    }

    /// Raccourcis par défaut (ceux de la version Python).
    pub fn default_keys(self) -> &'static str {
        use Action::*;
        match self {
            New => "Ctrl+N", OpenFile => "Ctrl+O", OpenFolder => "Ctrl+Maj+O", Save => "Ctrl+S", SaveAs => "Ctrl+Maj+S",
            SaveAll => "", CloseTab => "Ctrl+W", Quit => "Ctrl+Q", Find => "Ctrl+F", Goto => "Ctrl+G",
            Comment => "Ctrl+/ ou Ctrl+:", DuplicateLine => "Ctrl+D", Run => "F5", Debug => "F6", Stop => "Maj+F5",
            StepOver => "F10", StepInto => "F11", StepOut => "Maj+F11", Breakpoint => "F9", ToggleSide => "Ctrl+B",
            ToggleBottom => "Ctrl+J ou Ctrl+`", Terminal => "Alt+T", NewTerminal => "Ctrl+Maj+`", SplitRight => "Ctrl+\\",
            NextTab => "Ctrl+Tab", PrevTab => "Ctrl+Maj+Tab", ZoomIn => "Ctrl+= ou Ctrl++", ZoomOut => "Ctrl+-", ZoomReset => "Ctrl+0",
            Libs => "Ctrl+Maj+L", Ssh => "Ctrl+Alt+S", Vpn => "", Settings => "Ctrl+,", Menu => "Alt+M",
        }
    }

    /// Les raccourcis de ces actions restent actifs quand le terminal a le clavier.
    pub fn works_in_terminal(self) -> bool {
        matches!(self, Action::Run | Action::Debug | Action::Stop | Action::Save | Action::NewTerminal
                 | Action::ToggleBottom | Action::Settings)
    }
}

/// Une combinaison de touches, comparée exactement (Ctrl+S ne déclenche pas Ctrl+Maj+S).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl Shortcut {
    pub fn parse(s: &str) -> Option<Shortcut> {
        let s = s.trim();
        if s.is_empty() { return None; }
        let (mut ctrl, mut shift, mut alt) = (false, false, false);
        let parts: Vec<&str> = if s.ends_with("++") {
            let mut v: Vec<&str> = s[..s.len() - 2].split('+').collect();
            v.push("+");
            v
        } else { s.split('+').collect() };
        let mut key = None;
        for p in parts {
            match p.trim().to_lowercase().as_str() {
                "ctrl" | "cmd" | "control" => ctrl = true,
                "maj" | "shift" => shift = true,
                "alt" => alt = true,
                "" => {}
                k => key = key_from(k),
            }
        }
        key.map(|key| Shortcut { ctrl, shift, alt, key })
    }

    /// Vrai si l'évènement correspond exactement (mêmes touches de modification).
    /// Pour les signes qui demandent Maj selon le clavier (« + », « : », « / » en AZERTY…), Maj est ignorée.
    pub fn matches(&self, key: Key, m: Modifiers) -> bool {
        let shifted_sign = matches!(key, Key::Plus | Key::Colon | Key::Slash | Key::Questionmark | Key::Exclamationmark
                                         | Key::Pipe | Key::Period | Key::Num0 | Key::Num1 | Key::Num2 | Key::Num3
                                         | Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7 | Key::Num8 | Key::Num9);
        key == self.key && (m.ctrl || m.command || m.mac_cmd) == self.ctrl && m.alt == self.alt
            && (m.shift == self.shift || (shifted_sign && !self.shift))
    }

    pub fn modifiers(&self) -> Modifiers {
        let mut m = Modifiers::NONE;
        if self.ctrl { m = m | Modifiers::COMMAND; }
        if self.shift { m = m | Modifiers::SHIFT; }
        if self.alt { m = m | Modifiers::ALT; }
        m
    }

    pub fn from_event(key: Key, m: Modifiers) -> Shortcut {
        Shortcut { ctrl: m.ctrl || m.command || m.mac_cmd, shift: m.shift, alt: m.alt, key }
    }
}

impl std::fmt::Display for Shortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ctrl { write!(f, "Ctrl+")?; }
        if self.alt { write!(f, "Alt+")?; }
        if self.shift { write!(f, "Maj+")?; }
        write!(f, "{}", key_name(self.key))
    }
}

/// Touche de modification seule (Ctrl, Maj, Alt, Super) : pas un raccourci à elle seule.
pub fn is_modifier(k: Key) -> bool {
    matches!(k, Key::ShiftLeft | Key::ShiftRight | Key::ControlLeft | Key::ControlRight | Key::AltLeft | Key::AltRight
             | Key::SuperLeft | Key::SuperRight)
}

fn key_name(k: Key) -> String {
    match k {
        Key::Backtick => "`".into(),
        Key::Backslash => "\\".into(),
        Key::Slash => "/".into(),
        Key::Comma => ",".into(),
        Key::Period => ".".into(),
        Key::Semicolon => ";".into(),
        Key::Minus => "-".into(),
        Key::Equals => "=".into(),
        Key::Plus => "+".into(),
        Key::OpenBracket => "[".into(),
        Key::CloseBracket => "]".into(),
        Key::Quote => "'".into(),
        Key::Colon => ":".into(),
        Key::Pipe => "|".into(),
        Key::Questionmark => "?".into(),
        Key::Exclamationmark => "!".into(),
        Key::OpenCurlyBracket => "{".into(),
        Key::CloseCurlyBracket => "}".into(),
        Key::Tab => "Tab".into(),
        Key::Enter => "Entrée".into(),
        Key::Space => "Espace".into(),
        Key::Escape => "Échap".into(),
        Key::Delete => "Suppr".into(),
        Key::Backspace => "Retour".into(),
        Key::PageUp => "PgPréc".into(),
        Key::PageDown => "PgSuiv".into(),
        Key::ArrowUp => "Haut".into(),
        Key::ArrowDown => "Bas".into(),
        Key::ArrowLeft => "Gauche".into(),
        Key::ArrowRight => "Droite".into(),
        Key::Home => "Début".into(),
        Key::End => "Fin".into(),
        Key::Insert => "Inser".into(),
        other => other.name().to_string(),
    }
}

fn key_from(s: &str) -> Option<Key> {
    let k = match s {
        "`" => Key::Backtick, "\\" => Key::Backslash, "/" => Key::Slash, "," => Key::Comma, "." => Key::Period,
        ";" => Key::Semicolon, "-" => Key::Minus, "=" => Key::Equals, "+" => Key::Plus, "[" => Key::OpenBracket,
        "]" => Key::CloseBracket, "'" => Key::Quote, ":" => Key::Colon, "|" => Key::Pipe,
        "?" => Key::Questionmark, "!" => Key::Exclamationmark, "{" => Key::OpenCurlyBracket, "}" => Key::CloseCurlyBracket, "tab" => Key::Tab, "entrée" | "enter" => Key::Enter,
        "espace" | "space" => Key::Space, "échap" | "esc" | "escape" => Key::Escape, "suppr" | "delete" => Key::Delete,
        "retour" | "backspace" => Key::Backspace, "pgpréc" | "pageup" => Key::PageUp, "pgsuiv" | "pagedown" => Key::PageDown,
        "haut" | "up" => Key::ArrowUp, "bas" | "down" => Key::ArrowDown, "gauche" | "left" => Key::ArrowLeft,
        "droite" | "right" => Key::ArrowRight, "début" | "home" => Key::Home, "fin" | "end" => Key::End,
        "inser" | "insert" => Key::Insert,
        other => return Key::from_name(&other.to_uppercase()).or_else(|| Key::from_name(other)),
    };
    Some(k)
}

/// Raccourci pressé pendant cette image (comparaison exacte), consommé pour ne pas aller plus loin.
pub fn pressed(ctx: &egui::Context, sc: &Shortcut) -> bool {
    ctx.input_mut(|i| {
        let mut hit = None;
        for (n, e) in i.events.iter().enumerate() {
            if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                if sc.matches(*key, *modifiers) { hit = Some(n); break; }
            }
        }
        match hit {
            Some(n) => { i.events.remove(n); true }
            None => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_print() {
        for s in ["Ctrl+Maj+S", "F5", "Maj+F11", "Ctrl+\\", "Ctrl+Maj+`", "Ctrl+,", "Alt+M", "Ctrl+Tab", "Ctrl+=", "Ctrl+-", "Ctrl+/"] {
            let sc = Shortcut::parse(s).unwrap_or_else(|| panic!("{s}"));
            assert_eq!(sc.to_string(), s);
        }
        assert_eq!(Shortcut::parse("Ctrl++").map(|s| s.key), Some(Key::Plus));
        assert!(Shortcut::parse("").is_none());
        // chaque action a un raccourci par défaut lisible, sans doublon
        let mut seen = Vec::new();
        for a in ACTIONS {
            if let Some(sc) = Shortcut::parse(a.default_keys()) {
                assert!(!seen.contains(&sc), "doublon : {}", a.default_keys());
                seen.push(sc);
            }
        }
    }

    #[test]
    fn alternatives_and_azerty() {
        let c = Settings::default();
        assert_eq!(c.shortcuts(Action::Comment).len(), 2);
        let sl = Shortcut::parse("Ctrl+/").unwrap();
        // « / » demande Maj en AZERTY
        assert!(sl.matches(Key::Slash, Modifiers::COMMAND | Modifiers::SHIFT));
        let s = Shortcut::parse("Ctrl+S").unwrap();
        assert!(!s.matches(Key::S, Modifiers::COMMAND | Modifiers::SHIFT));
        assert_eq!(Shortcut::parse("Ctrl++").unwrap().key, Key::Plus);
        assert_eq!(c.shortcuts_text(Action::ZoomIn), "Ctrl+= ou Ctrl++");
    }

    #[test]
    fn exact_match() {
        let save = Shortcut::parse("Ctrl+S").unwrap();
        assert!(save.matches(Key::S, Modifiers::COMMAND));
        assert!(!save.matches(Key::S, Modifiers::COMMAND | Modifiers::SHIFT));
        let fixed = { let mut s = Settings { tab_width: 99, ..Default::default() }; s.side_order = vec!["vpn".into(), "x".into(), "vpn".into()]; s.fix(); s };
        assert_eq!(fixed.tab_width, 12);
        assert_eq!(fixed.side_order, vec!["vpn", "files", "ssh"]);
    }
}
