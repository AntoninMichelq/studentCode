//! Fenêtre principale de SCode : état de l'application et actions. Le dessin est dans
//! les sous-modules (ui : barres et dialogues, panes : volets et onglets, side : panneaux).
use crate::editor::Doc;
use crate::explorer::{Action, Explorer};
use crate::imageview::{self, ImageView};
use crate::pdf::PdfView;
use crate::pyscripter::{self, DbgEvent, Session, Var, END_MARK};
use crate::runner::{self, Out, Proc};
use crate::ssh::{self, Conn, Lab, RemoteEntry, Req, Resp};
use crate::terminal::Terminal;
use crate::theme::{self, Palette};
use crate::{pip, vpn};
use eframe::egui::{self, Color32, FontId, Key, Modifiers, RichText, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

mod panes;
mod side;
mod ui;
mod prefs;
mod lazy;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const URCA: &str = "https://cours.univ-reims.fr/local/mycoursesurca/courses.php";

/// Favoris par défaut (les mêmes que la version Python).
fn default_bookmarks() -> Vec<(String, String)> {
    vec![("Mes cours URCA".into(), URCA.into()),
         ("Bureau virtuel URCA".into(), "https://ebureau.univ-reims.fr".into()),
         ("Site de l'URCA".into(), "https://www.univ-reims.fr".into()),
         ("Documentation Python".into(), "https://docs.python.org/fr/3/".into())]
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct SessionFile {
    root: Option<PathBuf>,
    files: Vec<(PathBuf, usize)>,
    layout: Option<LayoutSave>,
    focus: usize,
    active: Vec<(usize, PathBuf)>,
    theme: String,
    font_size: f32,
    expanded: Vec<PathBuf>,
    recent: Vec<PathBuf>,
    hide_explorer: bool,
    hide_bottom: bool,
    output_tab: bool,
    breakpoints: BTreeMap<String, Vec<usize>>,
    run_args: String,
    bookmarks: Vec<(String, String)>,
    side: u8,
    /// Harpoon : fichiers marqués, par dossier ouvert
    harpoon: BTreeMap<String, Vec<PathBuf>>,
    /// fichiers ouverts récemment (␣fr)
    recent_files: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone)]
enum LayoutSave {
    Leaf(usize),
    Split(bool, f32, Box<LayoutSave>, Box<LayoutSave>),
}

#[derive(Clone)]
pub(crate) enum Layout {
    Leaf(usize),
    Split { vertical: bool, ratio: f32, a: Box<Layout>, b: Box<Layout> },
}

impl Layout {
    fn save(&self) -> LayoutSave {
        match self {
            Layout::Leaf(p) => LayoutSave::Leaf(*p),
            Layout::Split { vertical, ratio, a, b } => LayoutSave::Split(*vertical, *ratio, Box::new(a.save()), Box::new(b.save())),
        }
    }
    fn load(s: &LayoutSave) -> Layout {
        match s {
            LayoutSave::Leaf(p) => Layout::Leaf(*p),
            LayoutSave::Split(v, r, a, b) => Layout::Split { vertical: *v, ratio: *r, a: Box::new(Layout::load(a)), b: Box::new(Layout::load(b)) },
        }
    }
    fn save_debug(&self) -> String {
        match self {
            Layout::Leaf(p) => format!("L{p}"),
            Layout::Split { vertical, ratio, a, b } => format!("S{vertical}{:.2}({},{})", ratio, a.save_debug(), b.save_debug()),
        }
    }
    fn panes(&self, out: &mut Vec<usize>) {
        match self {
            Layout::Leaf(p) => out.push(*p),
            Layout::Split { a, b, .. } => { a.panes(out); b.panes(out); }
        }
    }
    /// Remplace le volet `pane` par une séparation avec le nouveau volet `new`.
    fn split(&mut self, pane: usize, new: usize, vertical: bool) -> bool {
        match self {
            Layout::Leaf(p) if *p == pane => {
                *self = Layout::Split { vertical, ratio: 0.5, a: Box::new(Layout::Leaf(pane)), b: Box::new(Layout::Leaf(new)) };
                true
            }
            Layout::Leaf(_) => false,
            Layout::Split { a, b, .. } => a.split(pane, new, vertical) || b.split(pane, new, vertical),
        }
    }
    /// Retire un volet ; son voisin prend toute la place.
    fn remove(&mut self, pane: usize) -> bool {
        if let Layout::Split { a, b, .. } = self {
            if matches!(**a, Layout::Leaf(p) if p == pane) { *self = (**b).clone(); return true; }
            if matches!(**b, Layout::Leaf(p) if p == pane) { *self = (**a).clone(); return true; }
            return a.remove(pane) || b.remove(pane);
        }
        false
    }
}

const DEMO: &str = "# Bienvenue dans SCode !
#   F5  : exécuter          F6  : déboguer / continuer
#   F9  : point d'arrêt     F10 : pas à pas (par-dessus)
#   F11 : pas à pas (entrer dans la fonction)
# Clique dans la marge à gauche d'une ligne pour poser un point d'arrêt.

def fibonacci(n):
    suite = [0, 1]
    while len(suite) < n:
        suite.append(suite[-1] + suite[-2])
    return suite


def moyenne(valeurs):
    total = 0
    for v in valeurs:
        total += v
    return total / len(valeurs)


nombres = fibonacci(10)
print(\"Fibonacci :\", nombres)
print(\"Moyenne   :\", moyenne(nombres))
";

pub(crate) enum Content {
    Doc(Doc),
    Pdf(Box<PdfView>),
    Image(ImageView),
}

pub(crate) struct Tab {
    pub c: Content,
    pub pane: usize,
    pub uid: u64,
}

impl Tab {
    fn title(&self) -> String {
        match &self.c {
            Content::Doc(d) => d.title(),
            Content::Pdf(p) => p.title(),
            Content::Image(i) => i.title(),
        }
    }
    fn path(&self) -> Option<&Path> {
        match &self.c {
            Content::Doc(d) => d.path.as_deref(),
            Content::Pdf(p) => Some(&p.path),
            Content::Image(i) => Some(&i.path),
        }
    }
    fn dirty(&self) -> bool {
        matches!(&self.c, Content::Doc(d) if d.dirty())
    }
    fn doc(&self) -> Option<&Doc> {
        if let Content::Doc(d) = &self.c { Some(d) } else { None }
    }
    fn doc_mut(&mut self) -> Option<&mut Doc> {
        if let Content::Doc(d) = &mut self.c { Some(d) } else { None }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Bottom { Terminal(usize), Output, Ssh(usize, usize), Repl, SshNone }

#[derive(PartialEq, Clone, Copy)]
enum SideTab { Files, Ssh, Vpn }

#[derive(PartialEq, Clone, Copy)]
enum Mode { Plain, PyConsole, Debug, Remote }

#[derive(Clone, Copy, PartialEq)]
enum OutKind { Normal, Error, Info, Echo, Warn, Debug }

enum Dialog {
    NewFile(PathBuf, String),
    NewDir(PathBuf, String),
    Rename(PathBuf, String),
    Delete(PathBuf),
    CloseUnsaved(u64),
    Quit,
    Args(String),
    Goto(String),
    Password { lab: Lab, pw: String, save: bool, why: String },
    LabEdit { index: Option<usize>, lab: Lab },
    RemoteName { session: usize, dir: String, name: String, dir_kind: bool },
    RemoteRename { session: usize, path: String, name: String },
    RemoteDelete { session: usize, path: String },
    VpnCreds { vpn: vpn::Vpn, user: String, pw: String, save: bool, why: String, then_lab: Option<Lab> },
    Info(String, String),
    Bookmark(String, String),
}

pub(crate) struct SshSession {
    conn: Conn,
    /// terminaux de cette machine (le premier s'ouvre à la connexion, clic droit → « Nouveau terminal »)
    terms: Vec<Terminal>,
    listing: HashMap<String, Vec<RemoteEntry>>,
    expanded: BTreeSet<String>,
    connected: bool,
    failed: Option<String>,
    selected: Option<String>,
    /// dossier affiché dans l'explorateur distant (le dossier personnel au départ)
    root: String,
    path_edit: String,
}

pub struct App {
    p: Palette,
    theme: String,
    font_size: f32,
    root: PathBuf,
    tabs: Vec<Tab>,
    layout: Layout,
    pane_active: HashMap<usize, u64>,
    focus_pane: usize,
    next_uid: u64,
    next_pane: usize,
    drag_tab: Option<u64>,
    explorer: Explorer,
    /// terminaux locaux (le premier suit le dossier ouvert)
    terms: Vec<Terminal>,
    /// onglet « Interpréteur Python »
    repl_term: Terminal,
    output: Vec<(String, OutKind)>,
    out_input: String,
    run: Option<Box<dyn Proc>>,
    mode: Mode,
    repl: bool,
    dbg: Option<Session>,
    frames: Vec<pyscripter::Frame>,
    frame_idx: usize,
    locals: Vec<Var>,
    globals: Vec<Var>,
    children: HashMap<String, Vec<Var>>,
    pending_children: HashMap<u64, String>,
    dbg_line: Option<(PathBuf, usize)>,
    error_line: Option<(PathBuf, usize)>,
    module: Option<PathBuf>,
    pdf_module: bool,
    bottom: Bottom,
    side: SideTab,
    show_explorer: bool,
    show_bottom: bool,
    find_open: bool,
    find_text: String,
    replace_text: String,
    find_info: String,
    dialog: Option<Dialog>,
    status: Option<(String, Instant, Color32)>,
    recent: Vec<PathBuf>,
    run_args: String,
    allow_quit: bool,
    title: String,
    new_count: usize,
    ctx: egui::Context,
    logo: Option<egui::TextureHandle>,
    // SSH
    labs: Vec<Lab>,
    sessions: Vec<SshSession>,
    ssh_sel: usize,
    /// sous-onglet « Nouvelle connexion » affiché dans le panneau SSH
    ssh_new: bool,
    /// réglages (fenêtre Paramètres)
    pub(crate) cfg: crate::settings::Settings,
    settings_open: bool,
    settings_page: u8,
    /// action dont on attend le nouveau raccourci
    key_capture: Option<crate::settings::Action>,
    menu_request: bool,
    /// touche leader de LazyVim en cours : (touches tapées après Espace, moment)
    pub(crate) leader: Option<(String, Instant)>,
    pub(crate) leader_pending: Vec<char>,
    pub(crate) picker: Option<lazy::Picker>,
    pub(crate) harpoon: BTreeMap<String, Vec<PathBuf>>,
    pub(crate) recent_files: Vec<PathBuf>,
    /// mode zen (␣uz) : panneaux à remettre
    pub(crate) zen: Option<(bool, bool)>,
    /// onglet affiché avant le courant (␣`)
    pub(crate) prev_tab: Option<u64>,
    /// donner le clavier au terminal affiché
    pub(crate) focus_terminal: bool,
    /// demandes de Vim à faire après le dessin des volets
    pub(crate) vim_queue: Vec<(Vec<crate::vim::Out>, Vec<crate::vim::K>)>,
    /// fin de programme à annoncer (code, suite de la sortie, moment)
    pending_end: Option<(String, String, Instant)>,
    /// points d'arrêt de la session (fichiers ouverts ou non)
    bp_store: BTreeMap<String, Vec<usize>>,
    autosave_tab: Option<u64>,
    autosave_focus: bool,
    /// réglages modifiés, à enregistrer
    pub(crate) cfg_dirty: bool,
    /// dernière frappe (enregistrement automatique)
    last_edit: Option<std::time::Instant>,
    /// champ « Évaluer une expression » du débogueur
    dbg_eval: String,
    /// fichiers cachés visibles dans l'explorateur distant
    ssh_hidden: bool,
    /// champ « adresse web » de la page d'accueil
    start_url: String,
    /// fenêtre « Bibliothèques Python » (bouton violet de la barre d'outils)
    libs_open: bool,
    quick: String,
    diag: Option<Receiver<String>>,
    // VPN
    vpns: Vec<vpn::Vpn>,
    vpn_sel: Option<String>,
    vpn_list: Option<Receiver<Result<Vec<vpn::Vpn>, String>>>,
    vpn_job: Option<(vpn::Job, Option<Lab>, Option<vpn::Vpn>)>,
    vpn_status: String,
    vpn_creds: HashMap<String, (String, String)>,
    vpn_refreshed: Option<Instant>,
    // Web
    bookmarks: Vec<(String, String)>,
    // Bibliothèques Python
    pkgs: Vec<(String, String)>,
    pkg_sel: Option<String>,
    pkg_rx: Option<Receiver<pip::Msg>>,
    pkg_log: String,
    pkg_name: String,
    pkg_filter: String,
    pkg_busy: bool,
    pkg_loaded: bool,
    pane_ops: Vec<PaneOp>,
    session_sig: u64,
    pending_saves: HashMap<(String, String), String>,
}

pub(crate) enum PaneOp {
    Close(u64),
    CloseOthers(u64),
    Split(u64, usize, bool),
    Move(u64, usize),
    /// onglet glissé dans la barre : le placer avant / après un autre
    Reorder(u64, u64, bool),
}

fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "StudentCode").map(|d| d.config_dir().to_path_buf())
}

pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    let (a, b) = (dunce(a), dunce(b));
    if cfg!(windows) { a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()) } else { a == b }
}

pub(crate) fn dunce(p: &Path) -> PathBuf {
    let c = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = c.to_string_lossy().to_string();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

impl App {
    pub fn new(cc: &eframe::CreationContext, arg: Option<PathBuf>) -> App {
        let cfg = crate::settings::Settings::load();
        theme::load_fonts(&cc.egui_ctx, &cfg.code_font, &cfg.ui_font);
        cc.egui_ctx.options_mut(|o| {
            o.zoom_with_keyboard = false;
            // un clic lent reste un clic (les boutons acceptent aussi un clic qui bouge : icons::tolerant)
            o.input_options.max_click_duration = 2.5;
        });
        cc.egui_ctx.set_zoom_factor(cfg.ui_scale);
        let raw_sess = config_dir().and_then(|d| std::fs::read_to_string(d.join("session.json")).ok());
        let first_launch = raw_sess.is_none();
        let sess: SessionFile = raw_sess.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        // premier lancement : on reprend le thème choisi dans la version Python s'il y en a un
        let theme_name = if sess.theme.is_empty() { theme::DEFAULT_THEME.to_string() } else { sess.theme.clone() };
        let font_size = if sess.font_size >= 9.0 { sess.font_size } else { 14.67 }; // 11 pt, comme la version Python
        let p = theme::palette(&theme_name);
        theme::apply(&cc.egui_ctx, &p, font_size);
        let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        crate::runner::detect_python_version(cc.egui_ctx.clone());
        let mut app = App {
            p, theme: theme_name, font_size, root: home, tabs: Vec::new(), layout: Layout::Leaf(0),
            pane_active: HashMap::new(), focus_pane: 0, next_uid: 1, next_pane: 1, drag_tab: None,
            explorer: Explorer::new(), terms: vec![Terminal::new()], repl_term: Terminal::new_repl(), output: Vec::new(), out_input: String::new(),
            run: None, mode: Mode::Plain, repl: false, dbg: None, frames: Vec::new(), frame_idx: 0,
            locals: Vec::new(), globals: Vec::new(), children: HashMap::new(), pending_children: HashMap::new(),
            dbg_line: None, error_line: None, module: pyscripter::module_dir(), pdf_module: crate::pdf::module_lib().is_some(),
            bottom: if sess.output_tab { Bottom::Output } else { Bottom::Terminal(0) },
            side: match sess.side { 1 => SideTab::Ssh, 2 => SideTab::Vpn, _ => SideTab::Files },
            show_explorer: !sess.hide_explorer, show_bottom: !sess.hide_bottom, find_open: false,
            find_text: String::new(), replace_text: String::new(), find_info: String::new(), dialog: None,
            status: None, recent: sess.recent.clone(), run_args: sess.run_args.clone(), allow_quit: false,
            title: String::new(), new_count: 0, ctx: cc.egui_ctx.clone(), logo: None,
            labs: ssh::load_labs(), sessions: Vec::new(), ssh_sel: 0, ssh_new: false, cfg, settings_open: false, settings_page: 0, key_capture: None, menu_request: false, leader: None, leader_pending: Vec::new(), picker: None, harpoon: BTreeMap::new(), recent_files: Vec::new(), zen: None, prev_tab: None, focus_terminal: false, vim_queue: Vec::new(), pending_end: None, bp_store: BTreeMap::new(), autosave_tab: None, autosave_focus: true, cfg_dirty: false, last_edit: None, dbg_eval: String::new(), ssh_hidden: false, start_url: String::new(), libs_open: false, quick: String::new(), diag: None,
            vpns: Vec::new(), vpn_sel: None, vpn_list: None, vpn_job: None, vpn_status: String::new(),
            vpn_creds: HashMap::new(), vpn_refreshed: None,
            bookmarks: if sess.bookmarks.is_empty() { default_bookmarks() } else { sess.bookmarks.clone() },
            pkgs: Vec::new(), pkg_sel: None, pkg_rx: None, pkg_log: String::new(), pkg_name: String::new(), pkg_filter: String::new(),
            pkg_busy: false, pkg_loaded: false, pane_ops: Vec::new(), session_sig: 0, pending_saves: HashMap::new(),
        };
        app.bp_store = sess.breakpoints.clone();
        app.harpoon = sess.harpoon.clone();
        app.recent_files = sess.recent_files.clone();
        app.explorer.expanded = sess.expanded.iter().cloned().collect();
        // « studentcode . » dans le dossier de la dernière séance : on retrouve ses fichiers ouverts
        let arg = match arg {
            Some(a) if a.is_dir() && sess.root.as_ref().map(|r| same_path(&a, r)).unwrap_or(false) => None,
            x => x,
        };
        match arg {
            // « studentcode autre-dossier » : on ouvre ce dossier, sans restaurer les fichiers de la session
            Some(a) if a.is_dir() => app.set_root(dunce(&a)),
            Some(a) => {
                let a = dunce(&a);
                if let Some(parent) = a.parent() {
                    let keep = sess.root.as_ref().map(|r| a.starts_with(r)).unwrap_or(false);
                    app.set_root(if keep { sess.root.clone().unwrap() } else { parent.to_path_buf() });
                }
                app.open_file(&a);
            }
            None => {
                if let Some(r) = sess.root.clone().filter(|r| r.is_dir()) {
                    app.root = r;
                }
                if let Some(l) = &sess.layout {
                    app.layout = Layout::load(l);
                    let mut ps = Vec::new();
                    app.layout.panes(&mut ps);
                    app.next_pane = ps.iter().max().map(|m| m + 1).unwrap_or(1);
                }
                let mut ps = Vec::new();
                app.layout.panes(&mut ps);
                for (f, pane) in &sess.files {
                    let pdf = f.extension().map(|e| e.eq_ignore_ascii_case("pdf")).unwrap_or(false);
                    if f.exists() && !(pdf && !app.pdf_module) {
                        app.focus_pane = if ps.contains(pane) { *pane } else { ps[0] };
                        app.open_file(f);
                    }
                }
                for (pane, f) in &sess.active {
                    if let Some(t) = app.tabs.iter().find(|t| t.pane == *pane && t.path().map(|p| p == f).unwrap_or(false)) {
                        app.pane_active.insert(*pane, t.uid);
                    }
                }
                app.focus_pane = if ps.contains(&sess.focus) { sess.focus } else { ps[0] };
                app.cleanup_panes();
            }
        }
        // tout premier lancement : le fichier d'exemple, comme la version Python
        if first_launch && app.tabs.is_empty() { app.new_doc_with(DEMO); }
        for t in app.tabs.iter_mut() {
            if let Content::Doc(d) = &mut t.c {
                if let Some(b) = d.path.as_ref().and_then(|p| sess.breakpoints.get(&p.to_string_lossy().to_string())) {
                    d.breakpoints = b.iter().copied().collect();
                }
            }
        }
        app
    }

    fn save_session(&self) {
        let Some(dir) = config_dir() else { return };
        let _ = std::fs::create_dir_all(&dir);
        let mut bps = self.bp_store.clone();
        for t in &self.tabs {
            if let Some(d) = t.doc() {
                if let Some(p) = &d.path {
                    let k = p.to_string_lossy().to_string();
                    if d.breakpoints.is_empty() { bps.remove(&k); } else { bps.insert(k, d.breakpoints.iter().copied().collect()); }
                }
            }
        }
        bps.retain(|k, _| Path::new(k).exists());
        let s = SessionFile {
            root: Some(self.root.clone()),
            files: self.tabs.iter().filter_map(|t| t.path().map(|p| (p.to_path_buf(), t.pane))).collect(),
            layout: Some(self.layout.save()),
            focus: self.focus_pane,
            active: self.pane_active.iter().filter_map(|(pane, uid)| {
                self.tabs.iter().find(|t| t.uid == *uid).and_then(|t| t.path()).map(|p| (*pane, p.to_path_buf()))
            }).collect(),
            theme: self.theme.clone(),
            font_size: self.font_size,
            expanded: self.explorer.expanded.iter().cloned().collect(),
            recent: self.recent.clone(),
            hide_explorer: !self.show_explorer,
            hide_bottom: !self.show_bottom,
            output_tab: self.bottom == Bottom::Output,
            breakpoints: bps,
            run_args: self.run_args.clone(),
            bookmarks: self.bookmarks.clone(),
            side: match self.side { SideTab::Files => 0, SideTab::Ssh => 1, SideTab::Vpn => 2 },
            harpoon: self.harpoon.clone(),
            recent_files: self.recent_files.clone(),
        };
        if let Ok(j) = serde_json::to_string_pretty(&s) {
            let _ = std::fs::write(dir.join("session.json"), j);
        }
    }

    /// Enregistre la session dès qu'elle change (dossier, onglets, volets, réglages) : rien
    /// n'est perdu si l'ordinateur s'éteint ou si l'app est fermée brutalement.
    fn autosave_session(&mut self) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.root.hash(&mut h);
        for t in &self.tabs { t.path().hash(&mut h); t.pane.hash(&mut h); }
        let mut act: Vec<(usize, u64)> = self.pane_active.iter().map(|(a, b)| (*a, *b)).collect();
        act.sort();
        act.hash(&mut h);
        format!("{:?}", self.layout.save_debug()).hash(&mut h);
        (self.focus_pane, &self.theme, (self.font_size * 10.0) as i32, self.show_bottom, self.show_explorer).hash(&mut h);
        (self.bottom == Bottom::Output, &self.recent, &self.bookmarks, &self.run_args).hash(&mut h);
        self.explorer.expanded.hash(&mut h);
        let sig = h.finish();
        if sig != self.session_sig {
            self.session_sig = sig;
            self.save_session();
        }
    }

    fn say(&mut self, msg: impl Into<String>, col: Color32) {
        self.status = Some((msg.into(), Instant::now(), col));
    }

    fn set_root(&mut self, dir: PathBuf) {
        self.root = dir.clone();
        self.recent.retain(|r| r != &dir);
        self.recent.insert(0, dir.clone());
        self.recent.truncate(8);
        self.explorer.refresh();
        self.terms[0].cd(&dir); // le terminal suit le dossier ouvert
    }

    fn open_folder_dialog(&mut self) {
        if let Some(d) = rfd::FileDialog::new().set_directory(&self.root).pick_folder() {
            self.set_root(d);
            self.save_session();
        }
    }

    // ------------------------------------------------------------------ onglets et volets
    pub(crate) fn panes(&self) -> Vec<usize> {
        let mut v = Vec::new();
        self.layout.panes(&mut v);
        v
    }

    fn add_tab(&mut self, c: Content) -> usize {
        let pane = self.focus_pane;
        let uid = self.next_uid;
        self.next_uid += 1;
        self.tabs.push(Tab { c, pane, uid });
        self.pane_active.insert(pane, uid);
        if let Some(d) = self.tabs.last_mut().and_then(|t| t.doc_mut()) { d.want_focus = true; }
        self.tabs.len() - 1
    }

    /// Onglet affiché dans le volet actif.
    pub(crate) fn cur(&self) -> Option<usize> {
        self.active_in(self.focus_pane)
    }

    pub(crate) fn active_in(&self, pane: usize) -> Option<usize> {
        let uid = self.pane_active.get(&pane)?;
        self.tabs.iter().position(|t| t.uid == *uid && t.pane == pane)
            .or_else(|| self.tabs.iter().position(|t| t.pane == pane))
    }

    fn cur_doc(&mut self) -> Option<&mut Doc> {
        let i = self.cur()?;
        self.tabs[i].doc_mut()
    }

    fn activate(&mut self, i: usize) {
        let (pane, uid) = (self.tabs[i].pane, self.tabs[i].uid);
        if let Some(old) = self.pane_active.get(&self.focus_pane).copied() { if old != uid { self.prev_tab = Some(old); } }
        self.pane_active.insert(pane, uid);
        self.focus_pane = pane;
        // le clavier va au fichier qu'on vient de choisir
        if let Some(d) = self.tabs[i].doc_mut() { d.want_focus = true; }
    }

    pub fn open_file(&mut self, path: &Path) {
        if let Some(i) = self.tabs.iter().position(|t| t.path().map(|p| same_path(p, path)).unwrap_or(false)) {
            self.activate(i);
            return;
        }
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let content = if ext == "pdf" {
            if !self.pdf_module {
                let _ = open::that_detached(path);
                self.say("Module Lecteur PDF non installé : ouvert avec l'application par défaut.", self.p.yellow);
                return;
            }
            match PdfView::open(path) {
                Ok(v) => Content::Pdf(Box::new(v)),
                Err(e) => { self.say(format!("PDF : {e}"), self.p.red); return; }
            }
        } else if imageview::is_image(path) {
            match ImageView::open(path, &self.ctx) {
                Ok(v) => Content::Image(v),
                Err(e) => { self.say(e, self.p.red); return; }
            }
        } else {
            match Doc::open(path) {
                Ok(mut d) => {
                    // points d'arrêt gardés d'une fois sur l'autre, même pour un fichier rouvert plus tard
                    if let Some(b) = self.bp_store.get(&path.to_string_lossy().to_string()) { d.breakpoints = b.iter().copied().collect(); }
                    Content::Doc(d)
                }
                Err(e) => {
                    if e == "fichier binaire" {
                        let _ = open::that_detached(path);
                    } else {
                        self.say(format!("Impossible d'ouvrir {} : {e}", path.display()), self.p.red);
                    }
                    return;
                }
            }
        };
        self.add_tab(content);
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.to_path_buf());
        self.recent_files.truncate(60);
    }

    fn new_doc(&mut self) {
        self.new_doc_with("");
    }

    fn new_doc_with(&mut self, text: &str) {
        self.new_count += 1;
        self.add_tab(Content::Doc(Doc::untitled(self.new_count, text)));
    }

    fn save_tab(&mut self, i: usize) -> bool {
        let trim = self.cfg.trim_on_save;
        let Some(d) = self.tabs[i].doc_mut() else { return true };
        if trim { d.trim_trailing(); }
        if let Some((key, path, _)) = d.remote.clone() {
            let data = d.text.clone().into_bytes();
            match self.sessions.iter().find(|s| s.conn.lab.key() == key && s.connected) {
                Some(s) => {
                    // marqué enregistré seulement quand la machine a confirmé (Resp::Saved)
                    s.conn.send(Req::Write(path, data));
                    let h = self.tabs[i].doc().map(|d| d.text.clone()).unwrap_or_default();
                    self.pending_saves.insert((key, self.tabs[i].doc().and_then(|d| d.remote.as_ref()).map(|r| r.1.clone()).unwrap_or_default()), h);
                    return true;
                }
                None => {
                    self.say("La machine de ce fichier n'est plus connectée : reconnecte-la pour enregistrer.", self.p.red);
                    return false;
                }
            }
        }
        if d.path.is_none() {
            let Some(p) = rfd::FileDialog::new().set_directory(&self.root).save_file() else { return false };
            let r = d.save_as(p);
            return self.report_save(r);
        }
        let r = d.save();
        self.report_save(r)
    }

    fn clear_output_for_run(&mut self) {
        if self.cfg.clear_output || self.output.is_empty() {
            self.output.clear();
        } else if !self.output.last().map(|(t, _)| t.ends_with("\n\n")).unwrap_or(true) {
            self.out("\n", OutKind::Normal);
        }
    }

    /// Enregistrement automatique (Paramètres → Éditeur) : en quittant l'onglet ou la fenêtre, ou une
    /// seconde après la dernière frappe. Seulement les fichiers qui ont déjà un nom.
    fn autosave_files(&mut self, ctx: &egui::Context) {
        let mode = self.cfg.autosave;
        if mode == 0 { return; }
        let cur = self.cur().map(|i| self.tabs[i].uid);
        let focused = ctx.input(|i| i.focused);
        let now = match mode {
            1 => {
                let changed = cur != self.autosave_tab || (self.autosave_focus && !focused);
                self.autosave_tab = cur;
                self.autosave_focus = focused;
                changed
            }
            _ => match self.last_edit {
                Some(t) if t.elapsed() >= Duration::from_secs(1) => { self.last_edit = None; true }
                Some(t) => { ctx.request_repaint_after(Duration::from_secs(1).saturating_sub(t.elapsed())); false }
                None => false,
            },
        };
        if !now { return; }
        for k in 0..self.tabs.len() {
            let ok = self.tabs[k].doc().map(|d| d.dirty() && d.untitled == 0 && (d.path.is_some() || d.remote.is_some())).unwrap_or(false);
            if ok { self.save_tab(k); }
        }
    }

    fn report_save(&mut self, r: Result<(), String>) -> bool {
        match r {
            Ok(()) => { self.explorer.refresh(); true }
            Err(e) => { self.say(format!("Enregistrement impossible : {e}"), self.p.red); false }
        }
    }

    fn close_tab(&mut self, i: usize, force: bool) {
        if i >= self.tabs.len() { return; }
        if !force && self.tabs[i].dirty() {
            self.dialog = Some(Dialog::CloseUnsaved(self.tabs[i].uid));
            return;
        }
        if let Content::Pdf(p) = &self.tabs[i].c { p.save_position(); }
        let t = self.tabs.remove(i);
        if let Some(d) = t.doc() {
            if let Some(p) = &d.path {
                let k = p.to_string_lossy().to_string();
                if d.breakpoints.is_empty() { self.bp_store.remove(&k); } else { self.bp_store.insert(k, d.breakpoints.iter().copied().collect()); }
            }
        }
        if self.pane_active.get(&t.pane) == Some(&t.uid) {
            // l'onglet voisin du même volet devient visible
            if let Some(n) = self.tabs.iter_mut().rev().find(|x| x.pane == t.pane) {
                self.pane_active.insert(t.pane, n.uid);
                if let Some(d) = n.doc_mut() { d.want_focus = true; }
            } else {
                self.pane_active.remove(&t.pane);
            }
        }
        self.cleanup_panes();
        self.save_session();
    }

    /// Les volets vides disparaissent (sauf le dernier).
    fn cleanup_panes(&mut self) {
        loop {
            let panes = self.panes();
            if panes.len() <= 1 { break; }
            let Some(empty) = panes.iter().find(|p| !self.tabs.iter().any(|t| t.pane == **p)).copied() else { break };
            self.layout.remove(empty);
            self.pane_active.remove(&empty);
            if self.focus_pane == empty { self.focus_pane = self.panes()[0]; }
        }
        let panes = self.panes();
        if !panes.contains(&self.focus_pane) { self.focus_pane = panes[0]; }
        for t in self.tabs.iter_mut() {
            if !panes.contains(&t.pane) { t.pane = panes[0]; }
        }
    }

    /// Déplace un onglet dans un nouveau volet, à droite (ou en bas) de son volet.
    pub(crate) fn split_tab(&mut self, uid: u64, target_pane: usize, vertical: bool) {
        let Some(i) = self.tabs.iter().position(|t| t.uid == uid) else { return };
        let new = self.next_pane;
        self.next_pane += 1;
        if !self.layout.split(target_pane, new, vertical) { return; }
        let old = self.tabs[i].pane;
        self.tabs[i].pane = new;
        self.pane_active.insert(new, uid);
        if self.pane_active.get(&old) == Some(&uid) {
            if let Some(n) = self.tabs.iter().find(|t| t.pane == old) { self.pane_active.insert(old, n.uid); }
        }
        self.focus_pane = new;
        self.cleanup_panes();
        self.save_session();
    }

    pub(crate) fn move_tab(&mut self, uid: u64, pane: usize) {
        let Some(i) = self.tabs.iter().position(|t| t.uid == uid) else { return };
        let old = self.tabs[i].pane;
        if old == pane { return; }
        self.tabs[i].pane = pane;
        self.pane_active.insert(pane, uid);
        if self.pane_active.get(&old) == Some(&uid) {
            if let Some(n) = self.tabs.iter().find(|t| t.pane == old) { self.pane_active.insert(old, n.uid); }
        }
        self.focus_pane = pane;
        self.cleanup_panes();
        self.save_session();
    }

    // ------------------------------------------------------------------ exécution
    fn out(&mut self, text: &str, kind: OutKind) {
        for (k, part) in text.split('\n').enumerate() {
            if k == 0 {
                if let Some(last) = self.output.last_mut() {
                    if last.1 == kind && !last.0.ends_with('\n') {
                        last.0.push_str(part);
                        continue;
                    }
                }
                self.output.push((part.to_string(), kind));
            } else {
                if let Some(last) = self.output.last_mut() {
                    last.0.push('\n');
                }
                self.output.push((part.to_string(), kind));
            }
        }
        if self.output.len() > 6000 {
            self.output.drain(..1000);
        }
    }

    fn start(&mut self, debug: bool) {
        if self.dbg.is_some() && !self.frames.is_empty() {
            self.dbg_cmd("continue");
            return;
        }
        if self.running() && !self.repl {
            self.say("Un programme tourne déjà : arrête-le d'abord (Maj+F5).", self.p.yellow);
            return;
        }
        self.stop();
        let Some(i) = self.cur().filter(|i| self.tabs[*i].doc().is_some()) else {
            self.say("Ouvre d'abord un fichier de code à exécuter.", self.p.yellow);
            return;
        };
        // enregistrer avant d'exécuter, comme les autres IDE (le fichier exécuté toujours, les autres
        // selon les Paramètres)
        for k in 0..self.tabs.len() {
            if k != i && !self.cfg.save_before_run { continue; }
            let save = self.tabs[k].doc().map(|d| d.dirty() && (d.path.is_some() || d.remote.is_some())).unwrap_or(false);
            if save { self.save_tab(k); }
        }
        let args: Vec<String> = self.run_args.split_whitespace().map(String::from).collect();
        // fichier d'une machine distante : exécuté sur cette machine
        if let Some((key, rpath, label)) = self.tabs[i].doc().and_then(|d| d.remote.clone()) {
            if debug {
                self.say("Le débogueur pas à pas fonctionne sur les fichiers de cet ordinateur.", self.p.yellow);
                return;
            }
            let Some((lab, pw)) = self.sessions.iter().find(|s| s.conn.lab.key() == key).map(|s| (s.conn.lab.clone(), s.conn.password.clone())) else {
                self.say("La machine de ce fichier n'est plus connectée.", self.p.red);
                return;
            };
            let Some(line) = runner::remote_command(&rpath, &args) else {
                self.say("Je ne sais pas exécuter ce type de fichier.", self.p.yellow);
                return;
            };
            self.clear_output_for_run();
            self.show_bottom = true;
            self.bottom = Bottom::Output;
            self.out(&format!("── Exécution : {} ({label}) ──\n", rpath.rsplit('/').next().unwrap_or(&rpath)), OutKind::Info);
            self.run = Some(Box::new(ssh::RemoteRun::start(lab, pw, line, self.ctx.clone())));
            self.mode = Mode::Remote;
            return;
        }
        if self.tabs[i].doc().map(|d| d.path.is_none()).unwrap_or(true) && !self.save_tab(i) {
            return;
        }
        let path = self.tabs[i].doc().and_then(|d| d.path.clone()).unwrap();
        let folder = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let is_py = path.extension().map(|e| e == "py" || e == "pyw").unwrap_or(false);
        self.clear_output_for_run();
        self.error_line = None;
        self.repl = false;
        self.show_bottom = true;
        self.bottom = Bottom::Output;
        let py = runner::python();
        let cmd;
        if is_py && self.module.is_some() {
            let Some(py) = py else { self.out(runner::PYTHON_MISSING, OutKind::Error); return };
            let module = self.module.clone().unwrap();
            let mut c = std::process::Command::new(&py[0]);
            c.args(&py[1..]).arg("-u");
            if debug {
                let breaks: Vec<(String, usize)> = self.tabs.iter().filter_map(|t| t.doc())
                    .filter_map(|d| d.path.as_ref().map(|p| (p, &d.breakpoints)))
                    .flat_map(|(p, b)| b.iter().map(move |l| (p.to_string_lossy().to_string(), *l))).collect();
                let stop_entry = if breaks.is_empty() { "1" } else { "0" };
                match Session::listen(breaks, self.ctx.clone()) {
                    Ok((s, port)) => {
                        self.dbg = Some(s);
                        c.arg(module.join("dbg_backend.py")).arg(port.to_string()).arg(stop_entry);
                    }
                    Err(e) => { self.out(&format!("Débogueur indisponible : {e}\n"), OutKind::Error); return; }
                }
                self.mode = Mode::Debug;
            } else {
                c.arg(module.join("run_backend.py"));
                self.mode = Mode::PyConsole;
            }
            c.arg(&path).args(&args).current_dir(&folder);
            #[cfg(windows)]
            { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
            cmd = c;
        } else {
            if debug {
                self.say(if self.module.is_none() { "Le débogueur fait partie du module PyScripter, non installé." }
                         else { "Le débogueur pas à pas ne fonctionne que pour Python." }, self.p.yellow);
                return;
            }
            match runner::command_line(&path, &args, py.as_deref()) {
                Ok(line) => cmd = runner::shell(&line, &folder),
                Err(e) => { self.out(&format!("{e}\n"), OutKind::Error); return; }
            }
            self.mode = Mode::Plain;
        }
        let label = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        // mêmes messages que la version Python
        self.out(&format!("── {} : {label} ──\n", if debug { "Débogage" } else { "Exécution" }), OutKind::Info);
        match runner::spawn(cmd) {
            Ok(r) => self.run = Some(Box::new(r)),
            Err(e) => { self.out(&format!("Lancement impossible : {e}\n"), OutKind::Error); self.dbg = None; }
        }
    }

    fn stop(&mut self) {
        if let Some(r) = self.run.as_mut() {
            if !r.finished() {
                r.kill();
            }
        }
        self.run = None;
        self.dbg = None;
        self.frames.clear();
        self.dbg_line = None;
        self.repl = false;
        self.mode = Mode::Plain;
    }

    fn dbg_cmd(&mut self, c: &str) {
        if let Some(s) = self.dbg.as_mut() {
            s.cmd(c);
            self.frames.clear();
            self.dbg_line = None;
        }
    }

    fn poll(&mut self) {
        // sortie du programme : la fin est relevée avant de vider le canal, pour ne rien perdre
        let mut chunks = Vec::new();
        let mut exit = None;
        if let Some(r) = self.run.as_mut() {
            if let Some(code) = r.poll_exit() {
                exit = Some(code);
            }
            while let Some(o) = r.try_recv() {
                chunks.push(o);
            }
        }
        for o in chunks {
            match o {
                Out::Stdout(t) => {
                    let t = runner::strip_ansi(&t);
                    if let Some(pos) = t.find(END_MARK) {
                        let (before, rest) = t.split_at(pos);
                        self.out(before, OutKind::Normal);
                        let rest = &rest[END_MARK.len()..];
                        let (code, after) = rest.split_once('\x1e').unwrap_or((rest, ""));
                        // le message de fin attend un instant : les erreurs (autre canal) arrivent
                        // parfois juste après, et doivent s'afficher avant lui
                        self.pending_end = Some((code.to_string(), after.to_string(), Instant::now()));
                        self.repl = true;
                        self.dbg = None;
                        self.frames.clear();
                        self.dbg_line = None;
                    } else if let Some(p) = self.pending_end.as_mut() {
                        p.1.push_str(&t); // après le message de fin
                    } else {
                        self.out(&t, OutKind::Normal);
                    }
                }
                Out::Stderr(t) => self.out(&runner::strip_ansi(&t), OutKind::Error),
            }
        }
        if self.pending_end.as_ref().map(|p| p.2.elapsed() >= Duration::from_millis(80) || exit.is_some()).unwrap_or(false) {
            if let Some((code, after, _)) = self.pending_end.take() {
                self.out(&format!("\n── Programme terminé (code {code}) · interpréteur actif : les variables, fonctions et classes du programme sont disponibles ──\n"),
                         if code.trim() == "0" { OutKind::Info } else { OutKind::Warn });
                self.out(&after, OutKind::Normal);
            }
        }
                if let Some(code) = exit {
            let (msg, kind) = match code {
                Some(0) => ("\n── Programme terminé (code 0) ──\n".to_string(), OutKind::Info),
                Some(c) => (format!("\n── Programme terminé (code {c}) ──\n"), OutKind::Warn),
                None => ("\n── Programme arrêté ──\n".to_string(), OutKind::Warn),
            };
            if self.repl {
                self.out("\n── Interpréteur fermé ──\n", OutKind::Info);
            } else {
                self.out(&msg, kind);
            }
            self.repl = false;
            self.dbg = None;
            self.frames.clear();
            self.dbg_line = None;
            self.mode = Mode::Plain;
        }
        // débogueur
        let mut evs = Vec::new();
        if let Some(s) = self.dbg.as_mut() {
            while let Ok(e) = s.rx.try_recv() {
                evs.push(e);
            }
        }
        for e in evs {
            match e {
                DbgEvent::Connected => {}
                DbgEvent::Stopped(frames) => {
                    self.frames = frames;
                    self.frame_idx = 0;
                    self.children.clear();
                    self.pending_children.clear();
                    self.show_frame(0);
                }
                DbgEvent::Vars { locals, globals } => {
                    self.locals = locals;
                    self.globals = globals;
                }
                DbgEvent::Children { id, items } => {
                    if let Some(expr) = self.pending_children.remove(&id) {
                        self.children.insert(expr, items);
                    }
                }
                DbgEvent::Eval { text, error } => {
                    if !text.is_empty() {
                        self.out(&format!("{text}\n"), if error { OutKind::Error } else { OutKind::Normal });
                    }
                }
                DbgEvent::Exception { file, line, text } => {
                    if let (Some(f), Some(l)) = (file, line) {
                        let pb = PathBuf::from(&f);
                        self.open_file(&pb);
                        let ctx = self.ctx.clone();
                        if let Some(d) = self.cur_doc() { d.goto_line(&ctx, l); }
                        self.error_line = Some((pb, l));
                    }
                    self.say(format!("Exception : {text}"), self.p.red);
                }
                DbgEvent::Closed => {
                    self.frames.clear();
                    self.dbg_line = None;
                }
            }
        }
        self.poll_ssh();
        self.poll_vpn();
        self.poll_pip();
    }

    fn show_frame(&mut self, k: usize) {
        let Some(f) = self.frames.get(k) else { return };
        let (file, line) = (PathBuf::from(&f.file), f.line);
        self.frame_idx = k;
        self.open_file(&file);
        let ctx = self.ctx.clone();
        if let Some(d) = self.cur_doc() {
            d.goto_line(&ctx, line);
        }
        self.dbg_line = Some((file, line));
        if let Some(s) = self.dbg.as_mut() {
            s.request_vars(k);
        }
    }

    fn send_input(&mut self) {
        // comme la version Python : la ligne va au programme (l'évaluation au point d'arrêt
        // se fait dans le champ du panneau de débogage)
        let line = std::mem::take(&mut self.out_input);
        if let Some(r) = self.run.as_mut().filter(|r| !r.finished()) {
            r.write(&format!("{line}\n"));
            self.out(&format!("{line}\n"), OutKind::Echo);
        } else {
            self.say("Lance d'abord le programme (F5) : tu pourras ensuite taper du Python ici.", self.p.yellow);
        }
    }

    // ------------------------------------------------------------------ explorateur
    fn explorer_action(&mut self, a: Action) {
        match a {
            Action::Open(p) => self.open_file(&p),
            Action::NewFile(d) => self.dialog = Some(Dialog::NewFile(d, String::new())),
            Action::NewDir(d) => self.dialog = Some(Dialog::NewDir(d, String::new())),
            Action::Rename(p) => {
                let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.dialog = Some(Dialog::Rename(p, n));
            }
            Action::Delete(p) => self.dialog = Some(Dialog::Delete(p)),
            Action::Terminal(d) => {
                self.terms[0].cd(&d);
                self.show_bottom = true;
                self.bottom = Bottom::Terminal(0);
            }
            Action::Upload(p) => {
                match self.sessions.get(self.ssh_sel).filter(|s| s.connected) {
                    Some(s) => {
                        let dir = s.selected.clone().filter(|x| s.listing.contains_key(x)).or_else(|| s.conn.home.clone()).unwrap_or("/".into());
                        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        s.conn.send(Req::Upload(p, format!("{}/{name}", dir.trim_end_matches('/'))));
                        self.say(format!("Envoi vers {dir}…"), self.p.comment);
                    }
                    None => self.say("Connecte d'abord une machine dans l'onglet SSH.", self.p.yellow),
                }
            }
            Action::Moved(src, dir) => {
                let Some(name) = src.file_name() else { return };
                let dst = dir.join(name);
                if dst.exists() {
                    self.say(format!("« {} » existe déjà dans ce dossier.", name.to_string_lossy()), self.p.red);
                    return;
                }
                match std::fs::rename(&src, &dst) {
                    Ok(()) => {
                        self.retarget(&src, &dst);
                        self.say(format!("Déplacé vers {}", dir.display()), self.p.green);
                    }
                    Err(e) => self.say(format!("Déplacement impossible : {e}"), self.p.red),
                }
                self.explorer.refresh();
            }
        }
    }

    /// Les onglets ouverts suivent un fichier ou dossier déplacé / renommé.
    fn retarget(&mut self, from: &Path, to: &Path) {
        for t in self.tabs.iter_mut() {
            if let Some(d) = t.doc_mut() {
                if let Some(p) = d.path.clone() {
                    if let Ok(rest) = p.strip_prefix(from) {
                        d.path = Some(if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) });
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ terminaux
    /// Le terminal affiché en bas (s'il y en a un).
    fn shown_term(&self) -> Option<&Terminal> {
        match self.bottom {
            Bottom::Terminal(n) => self.terms.get(n),
            Bottom::Ssh(i, n) => self.sessions.get(i).and_then(|s| s.terms.get(n)),
            Bottom::Repl => Some(&self.repl_term),
            Bottom::Output | Bottom::SshNone => None,
        }
    }

    /// Nouveau terminal local (dans `dir`, sinon le dossier ouvert).
    fn new_local_terminal(&mut self, dir: Option<PathBuf>) {
        let mut t = Terminal::new();
        t.cd(dir.as_deref().unwrap_or(&self.root));
        self.terms.push(t);
        self.bottom = Bottom::Terminal(self.terms.len() - 1);
        self.show_bottom = true;
    }

    /// Nouveau terminal sur la machine `i` (dans `dir`, sinon le dossier personnel).
    fn new_ssh_terminal(&mut self, i: usize, dir: Option<String>) {
        let Some(s) = self.sessions.get_mut(i) else { return };
        let t = Terminal::new_ssh(s.conn.lab.clone(), s.conn.password.clone(), dir);
        s.terms.push(t);
        self.bottom = Bottom::Ssh(i, s.terms.len() - 1);
        self.show_bottom = true;
    }

    /// « Nouveau terminal » depuis un onglet : même machine que l'onglet.
    fn new_terminal_like(&mut self, b: Bottom) {
        match b {
            Bottom::Ssh(i, _) if self.sessions.get(i).map(|s| s.connected).unwrap_or(false) => self.new_ssh_terminal(i, None),
            _ => self.new_local_terminal(None),
        }
    }

    /// Ferme un terminal ouvert en plus (le premier de chaque machine reste).
    fn close_terminal(&mut self, b: Bottom) {
        match b {
            Bottom::Terminal(n) if n > 0 && n < self.terms.len() => {
                let mut t = self.terms.remove(n);
                t.kill();
                if let Bottom::Terminal(k) = self.bottom { if k >= n { self.bottom = Bottom::Terminal(k.saturating_sub(1).min(self.terms.len() - 1)); } }
            }
            Bottom::Ssh(i, n) if n > 0 => {
                let Some(s) = self.sessions.get_mut(i) else { return };
                if n >= s.terms.len() { return; }
                let mut t = s.terms.remove(n);
                t.kill();
                let len = s.terms.len();
                if let Bottom::Ssh(k, m) = self.bottom { if k == i && m >= n { self.bottom = Bottom::Ssh(i, m.saturating_sub(1).min(len - 1)); } }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ SSH
    pub(crate) fn connect_lab(&mut self, lab: Lab, password: Option<String>) {
        if let Some(i) = self.sessions.iter().position(|s| s.conn.lab.key() == lab.key()) {
            if self.sessions[i].connected || self.sessions[i].failed.is_none() {
                self.ssh_sel = i;
                self.ssh_new = false;
                self.side = SideTab::Ssh;
                return;
            }
            self.sessions.remove(i);
        }
        // VPN du lab d'abord
        if !lab.vpn.is_empty() && password.is_none() {
            if let Some(v) = self.vpns.iter().find(|v| v.name == lab.vpn).cloned() {
                if !v.active {
                    // l'adresse de la machine passe par le VPN (garder internet)
                    let mut o = vpn::opts(&v.id);
                    if lab.host.parse::<std::net::Ipv4Addr>().is_ok() && !o.hosts.contains(&lab.host) {
                        o.hosts.push(lab.host.clone());
                        vpn::save_opts(&v.id, &o);
                    }
                    self.vpn_connect(v, Some(lab), false);
                    return;
                }
            } else if self.vpns.is_empty() && self.vpn_refreshed.is_none() {
                self.refresh_vpns();
            }
        }
        let pw = password.or_else(|| crate::secrets::get(&lab.key()));
        match pw {
            Some(pw) => {
                let conn = Conn::open(lab.clone(), pw.clone(), self.ctx.clone());
                let term = Terminal::new_ssh(lab, pw, None);
                self.sessions.push(SshSession { conn, terms: vec![term], listing: HashMap::new(), expanded: BTreeSet::new(),
                    connected: false, failed: None, selected: None, root: String::new(), path_edit: String::new() });
                self.ssh_sel = self.sessions.len() - 1;
                self.ssh_new = false;
                self.side = SideTab::Ssh;
            }
            None => {
                let save = lab.save_password;
                self.dialog = Some(Dialog::Password { lab, pw: String::new(), save, why: String::new() });
            }
        }
    }

    pub(crate) fn disconnect(&mut self, i: usize) {
        if i >= self.sessions.len() { return; }
        let mut s = self.sessions.remove(i);
        for t in s.terms.iter_mut() { t.kill(); }
        if let Bottom::Ssh(k, n) = self.bottom {
            if k == i { self.bottom = Bottom::Terminal(0); } else if k > i { self.bottom = Bottom::Ssh(k - 1, n); }
        }
        self.ssh_sel = self.ssh_sel.min(self.sessions.len().saturating_sub(1));
    }

    fn poll_ssh(&mut self) {
        let mut events = Vec::new();
        for (i, s) in self.sessions.iter_mut().enumerate() {
            while let Ok(r) = s.conn.rx.try_recv() {
                events.push((i, r));
            }
        }
        let mut refused: Option<usize> = None;
        for (i, r) in events {
            if refused == Some(i) { continue; }
            let Some(s) = self.sessions.get_mut(i) else { continue };
            let lab = s.conn.lab.clone();
            match r {
                Resp::Connected { home, cleaned } => {
                    s.connected = true;
                    s.conn.home = Some(home.clone());
                    s.expanded.insert(home.clone());
                    if s.root.is_empty() { s.root = home.clone(); s.path_edit = home.clone(); }
                    s.conn.send(Req::List(s.root.clone()));
                    if lab.save_password { crate::secrets::set(&lab.key(), &s.conn.password); }
                    let msg = if cleaned.is_empty() { format!("Connecté à {}.", lab.label()) }
                              else { format!("Connecté à {} · {} processus d'une séance précédente arrêté(s).", lab.label(), cleaned.len()) };
                    self.say(msg, self.p.green);
                }
                Resp::Failed(e) => {
                    let failed_auth = e == "AUTH";
                    s.failed = Some(ssh::friendly(&e, &lab));
                    if failed_auth {
                        // la session est retirée après la boucle (les numéros des autres restent valables)
                        crate::secrets::delete(&lab.key());
                        refused = Some(i);
                        self.dialog = Some(Dialog::Password { lab: lab.clone(), pw: String::new(), save: lab.save_password,
                            why: "Mot de passe refusé — réessaie.".into() });
                        continue;
                    }
                    let (host, port) = (lab.host.clone(), lab.port);
                    let (tx, rx) = std::sync::mpsc::channel();
                    let ctx = self.ctx.clone();
                    std::thread::spawn(move || { let _ = tx.send(ssh::diagnose(&host, port)); ctx.request_repaint(); });
                    self.diag = Some(rx);
                    self.say(format!("Connexion à {} impossible : {}", lab.label(), ssh::friendly(&e, &lab)), self.p.red);
                }
                Resp::Listed(dir, res) => match res {
                    Ok(v) => { s.listing.insert(dir, v); }
                    Err(e) => self.say(format!("Lecture de {dir} impossible : {e}"), self.p.red),
                },
                Resp::Read(path, res) => match res {
                    Ok(text) => {
                        let key = lab.key();
                        if let Some(k) = self.tabs.iter().position(|t| t.doc().and_then(|d| d.remote.as_ref())
                            .map(|(kk, pp, _)| kk == &key && pp == &path).unwrap_or(false)) {
                            self.activate(k);
                        } else {
                            self.add_tab(Content::Doc(Doc::from_remote(&key, &path, &lab.label(), text)));
                        }
                    }
                    Err(e) => self.say(format!("Ouverture de {path} impossible : {e}"), self.p.red),
                },
                Resp::Done(msg) => {
                    let dirs: Vec<String> = s.listing.keys().cloned().collect();
                    for d in dirs { s.conn.send(Req::List(d)); }
                    if !msg.is_empty() { self.say(msg, self.p.green); }
                }
                Resp::Saved(path) => {
                    let key = lab.key();
                    if let Some(text) = self.pending_saves.remove(&(key.clone(), path.clone())) {
                        for t in self.tabs.iter_mut() {
                            if let Some(d) = t.doc_mut() {
                                if d.remote.as_ref().map(|(k, p, _)| *k == key && *p == path).unwrap_or(false) && d.text == text {
                                    d.mark_saved();
                                }
                            }
                        }
                    }
                    self.say(format!("Enregistré sur {} : {path}", lab.label()), self.p.green);
                }
                Resp::SaveFailed(path, e) => {
                    self.pending_saves.remove(&(lab.key(), path.clone()));
                    self.dialog = Some(Dialog::Info("Enregistrement impossible".into(),
                        format!("{path} n'a pas pu être enregistré sur {} :\n{e}\n\nTes modifications sont toujours dans l'onglet.", lab.label())));
                }
                Resp::Error(e) => self.say(e, self.p.red),
                Resp::Closed => { s.connected = false; }
            }
        }
        if let Some(i) = refused { self.disconnect(i); }
        if let Some(rx) = &self.diag {
            if let Ok(msg) = rx.try_recv() {
                self.diag = None;
                self.dialog = Some(Dialog::Info("Connexion SSH impossible".into(), msg));
            }
        }
    }

    // ------------------------------------------------------------------ VPN
    pub(crate) fn refresh_vpns(&mut self) {
        if vpn::available().is_err() { return; }
        self.vpn_list = Some(vpn::list_job(&self.ctx));
        self.vpn_refreshed = Some(Instant::now());
    }

    pub(crate) fn vpn_connect(&mut self, v: vpn::Vpn, then_lab: Option<Lab>, ask: bool) {
        if self.vpn_job.is_some() {
            self.say("Une opération VPN est déjà en cours.", self.p.yellow);
            return;
        }
        let saved = if ask { None } else {
            self.vpn_creds.get(&v.id).cloned().or_else(|| crate::secrets::get(&format!("vpn-{}", v.id))
                .and_then(|s| s.split_once('\n').map(|(a, b)| (a.to_string(), b.to_string()))))
        };
        if cfg!(windows) || saved.is_some() {
            let (u, pw) = saved.unwrap_or_default();
            self.vpn_status = format!("Connexion au VPN {}…", v.name);
            self.vpn_job = Some((vpn::connect(&v, &u, &pw, &self.ctx), then_lab, Some(v)));
        } else {
            let user = vpn::opts(&v.id).user;
            self.dialog = Some(Dialog::VpnCreds { vpn: v, user, pw: String::new(), save: true, why: String::new(), then_lab });
        }
    }

    fn poll_vpn(&mut self) {
        if let Some(rx) = &self.vpn_list {
            if let Ok(r) = rx.try_recv() {
                self.vpn_list = None;
                match r {
                    Ok(v) => {
                        self.vpns = v;
                        if self.vpn_sel.is_none() { self.vpn_sel = self.vpns.first().map(|v| v.id.clone()); }
                    }
                    Err(e) => self.vpn_status = vpn::nm_error(&e),
                }
            }
        }
        let done = self.vpn_job.as_ref().and_then(|(j, _, _)| j.rx.try_recv().ok());
        if let Some(r) = done {
            let (job, lab, v) = self.vpn_job.take().unwrap();
            match r {
                Ok(msg) => {
                    self.vpn_status = msg;
                    if job.what == "up" {
                        if let Some(v) = &v {
                            if let Some((u, pw)) = self.vpn_creds.get(&v.id).cloned() {
                                let _ = (u, pw);
                            }
                        }
                    }
                    if let Some(lab) = lab { self.connect_lab_after_vpn(lab); }
                }
                Err(e) => {
                    if job.what == "up" && e.contains("refusé") {
                        if let Some(v) = v.clone() {
                            crate::secrets::delete(&format!("vpn-{}", v.id));
                            self.vpn_creds.remove(&v.id);
                            let user = vpn::opts(&v.id).user;
                            self.dialog = Some(Dialog::VpnCreds { vpn: v, user, pw: String::new(), save: true,
                                why: "Identifiants refusés — réessaie.".into(), then_lab: lab });
                        }
                    } else {
                        self.vpn_status = format!("Échec : {e}");
                        self.say(format!("VPN : {e}"), self.p.red);
                    }
                }
            }
            self.refresh_vpns();
        }
        // état du VPN relu de temps en temps quand l'onglet est visible
        if self.side == SideTab::Vpn && self.show_explorer && self.vpn_list.is_none()
            && self.vpn_refreshed.map(|t| t.elapsed().as_secs() >= 6).unwrap_or(true) {
            self.refresh_vpns();
        }
    }

    fn connect_lab_after_vpn(&mut self, lab: Lab) {
        let pw = crate::secrets::get(&lab.key());
        match pw {
            Some(pw) => self.connect_lab(lab, Some(pw)),
            None => {
                let save = lab.save_password;
                self.dialog = Some(Dialog::Password { lab, pw: String::new(), save, why: String::new() });
            }
        }
    }

    // ------------------------------------------------------------------ pip
    pub(crate) fn pip_list(&mut self) {
        match runner::python() {
            Some(py) => { self.pkg_rx = Some(pip::list(py, &self.ctx)); self.pkg_busy = true; self.pkg_loaded = true; }
            None => self.pkg_log = runner::PYTHON_MISSING.into(),
        }
    }

    pub(crate) fn pip_run(&mut self, install: bool, names: Vec<String>) {
        if self.pkg_busy { return; }
        match runner::python() {
            Some(py) => { self.pkg_rx = Some(pip::run(py, install, names, &self.ctx)); self.pkg_busy = true; }
            None => self.pkg_log = runner::PYTHON_MISSING.into(),
        }
    }

    fn poll_pip(&mut self) {
        let mut relist = false;
        if let Some(rx) = &self.pkg_rx {
            while let Ok(m) = rx.try_recv() {
                match m {
                    pip::Msg::Line(l) => { self.pkg_log.push_str(&l); }
                    pip::Msg::Listed(v) => { self.pkgs = v; self.pkg_busy = false; }
                    pip::Msg::Done(ok) => {
                        self.pkg_log.push_str(if ok { "→ terminé\n" } else { "→ échec\n" });
                        self.pkg_busy = false;
                        relist = true;
                    }
                }
            }
        }
        if self.pkg_log.len() > 40_000 { self.pkg_log.drain(..20_000); }
        if relist { self.pip_list(); }
    }

    // ------------------------------------------------------------------ raccourcis
    /// Raccourcis : ceux de la fenêtre Paramètres (comparés exactement : Ctrl+S ne déclenche pas
    /// Ctrl+Maj+S). Quand un terminal a le clavier, seules quelques actions restent à l'app.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.key_capture.is_some() { return; } // la fenêtre Paramètres attend une touche
        if self.dialog.is_some() { return; } // une petite fenêtre (aller à la ligne, mot de passe…) a le clavier
        if self.picker.is_some() || self.leader.is_some() { return; }
        let in_term = self.shown_term().map(|t| t.focused).unwrap_or(false) && self.show_bottom;
        let mut todo = Vec::new();
        let editor_focus = self.cur().and_then(|i| self.tabs[i].doc()).map(|d| d.has_focus(ctx)).unwrap_or(false);
        // mode Vim (hors insertion) : Ctrl+d/u/r/h/j/k/l et Ctrl+/ sont à Vim et LazyVim
        let vim_keys = self.cfg.vim_mode && editor_focus
            && self.cur().and_then(|i| self.tabs[i].doc()).map(|d| d.vim.mode != crate::vim::Mode::Insert).unwrap_or(false);
        // Ctrl+/ dans le terminal : le masquer et revenir au code (LazyVim)
        if self.cfg.vim_mode && in_term && crate::settings::pressed(ctx, &crate::settings::Shortcut { ctrl: true, shift: false, alt: false, key: Key::Slash }) {
            self.show_bottom = false;
            if let Some(d) = self.cur_doc() { d.want_focus = true; }
            return;
        }
        for a in crate::settings::ACTIONS {
            if in_term && !a.works_in_terminal() { continue; }
            if a.needs_editor() && !editor_focus { continue; }
            for sc in self.cfg.shortcuts(*a) {
                if vim_keys && sc.ctrl && !sc.alt && !sc.shift
                    && matches!(sc.key, Key::D | Key::U | Key::R | Key::H | Key::J | Key::K | Key::L | Key::Slash | Key::Colon | Key::OpenBracket) { continue; }
                if crate::settings::pressed(ctx, &sc) { todo.push(*a); break; }
            }
        }
        for a in todo { self.run_action(a, ctx); }
        if self.find_open && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.find_open = false;
            if let Some(d) = self.cur_doc() { d.find = None; d.focus(ctx); }
        }
    }

    /// Exécute une action (raccourci, menu ou barre d'outils).
    pub(crate) fn run_action(&mut self, a: crate::settings::Action, ctx: &egui::Context) {
        use crate::settings::Action as A;
        let stopped = self.dbg.is_some() && !self.frames.is_empty();
        match a {
            A::New => self.open_picker(lazy::PickKind::NewFile, ""),
            A::OpenFile => { if let Some(f) = rfd::FileDialog::new().set_directory(&self.root).pick_file() { self.open_file(&f); } }
            A::OpenFolder => self.open_folder_dialog(),
            A::Save => { if let Some(i) = self.cur() { self.save_tab(i); } }
            A::SaveAs => self.save_as(),
            A::SaveAll => { for i in 0..self.tabs.len() { if self.tabs[i].dirty() { self.save_tab(i); } } }
            A::CloseTab => { if let Some(i) = self.cur() { self.close_tab(i, false); } }
            A::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            A::Find | A::Replace => {
                let pdf = self.cur().and_then(|i| if let Content::Pdf(v) = &mut self.tabs[i].c { Some(v) } else { None });
                if let Some(v) = pdf { v.open_search(ctx); } else if self.cur().is_some() {
                    // le texte sélectionné devient le texte cherché, comme partout
                    if let Some(t) = self.cur_doc().and_then(|d| d.selected_text(ctx)) { self.find_text = t; }
                    self.find_open = true;
                    let field = if a == A::Replace { "replace" } else { "find" };
                    ctx.memory_mut(|m| m.request_focus(egui::Id::new(field)));
                }
            }
            A::FindNext | A::FindPrev => {
                if let Some(t) = self.cur_doc().and_then(|d| d.selected_text(ctx)).filter(|_| self.find_text.is_empty()) { self.find_text = t; }
                if !self.find_text.is_empty() {
                    self.find_open = true;
                    let q = self.find_text.clone();
                    if let Some(d) = self.cur_doc() {
                        d.find = Some(q);
                        if a == A::FindNext { d.find_next = true; } else { d.find_back = true; }
                    }
                }
            }
            A::DeleteLine | A::MoveLineUp | A::MoveLineDown => {
                use crate::editor::EditCmd;
                let c = match a { A::DeleteLine => EditCmd::DeleteLine, A::MoveLineUp => EditCmd::MoveLineUp, _ => EditCmd::MoveLineDown };
                if let Some(d) = self.cur_doc() { d.pending_edit = Some(c); }
            }
            A::Goto => { if self.cur_doc().is_some() { self.dialog = Some(Dialog::Goto(String::new())); } }
            A::Comment => { if let Some(d) = self.cur_doc() { d.pending_comment = true; } }
            A::DuplicateLine => { if let Some(d) = self.cur_doc() { d.pending_duplicate = true; } }
            A::Run => self.start(false),
            A::Debug => { if stopped { self.dbg_cmd("continue"); } else { self.start(true); } }
            A::Stop => self.stop(),
            A::StepOver => { if stopped { self.dbg_cmd("next"); } }
            A::StepInto => { if stopped { self.dbg_cmd("step"); } }
            A::StepOut => { if stopped { self.dbg_cmd("return"); } }
            A::Breakpoint => self.toggle_bp_here(),
            A::ToggleSide => self.show_explorer = !self.show_explorer,
            A::ToggleBottom => self.show_bottom = !self.show_bottom,
            A::Terminal => { self.show_bottom = true; self.bottom = Bottom::Terminal(0); }
            A::NewTerminal => self.new_terminal_like(self.bottom),
            A::SplitRight => {
                if let Some(i) = self.cur() { let (u, p) = (self.tabs[i].uid, self.tabs[i].pane); self.split_tab(u, p, false); }
            }
            A::NextTab | A::PrevTab => {
                let pane = self.focus_pane;
                let ids: Vec<usize> = (0..self.tabs.len()).filter(|i| self.tabs[*i].pane == pane).collect();
                if !ids.is_empty() {
                    let cur = self.cur().and_then(|c| ids.iter().position(|x| *x == c)).unwrap_or(0);
                    let n = ids.len();
                    self.activate(ids[if a == A::NextTab { (cur + 1) % n } else { (cur + n - 1) % n }]);
                }
            }
            A::ZoomIn => self.zoom(1.0, ctx),
            A::ZoomOut => self.zoom(-1.0, ctx),
            A::ZoomReset => { self.font_size = 14.67; }
            A::Libs => {
                if self.module.is_some() {
                    self.libs_open = !self.libs_open;
                    if self.libs_open && !self.pkg_loaded { self.pip_list(); }
                }
            }
            A::Ssh => { self.show_explorer = true; self.side = SideTab::Ssh; }
            A::Vpn => { self.show_explorer = true; self.side = SideTab::Vpn; self.refresh_vpns(); }
            A::Settings => self.settings_open = !self.settings_open,
            A::ToggleVim => {
                self.cfg.vim_mode = !self.cfg.vim_mode;
                self.cfg_dirty = true;
                self.say(if self.cfg.vim_mode { "Mode Vim activé (Échap : mode Normal, i : insertion, Espace : raccourcis)" } else { "Mode Vim désactivé" }, self.p.blue);
            }
            A::Menu => self.menu_request = true,
        }
    }

    fn save_as(&mut self) {
        if let Some(i) = self.cur() {
            if self.tabs[i].doc().is_none() { return; }
            let name = self.tabs[i].title();
            if let Some(f) = rfd::FileDialog::new().set_directory(&self.root).set_file_name(&name).save_file() {
                if let Some(d) = self.tabs[i].doc_mut() { d.remote = None; d.untitled = 0; let r = d.save_as(f); self.report_save(r); }
            }
        }
    }

    /// Point d'arrêt sur la ligne du curseur (F9 ou bouton rouge).
    fn toggle_bp_here(&mut self) {
        if let Some(i) = self.cur() {
            if let Some(line) = self.tabs[i].doc().map(|d| d.cursor_line) { self.toggle_breakpoint(i, line); }
        }
    }

    fn toggle_breakpoint(&mut self, i: usize, line: usize) {
        let Some(d) = self.tabs[i].doc_mut() else { return };
        let on = !d.breakpoints.remove(&line);
        if on { d.breakpoints.insert(line); }
        self.sync_breakpoint(i, line);
    }

    fn sync_breakpoint(&mut self, i: usize, line: usize) {
        let Some(d) = self.tabs[i].doc() else { return };
        let on = d.breakpoints.contains(&line);
        if let (Some(s), Some(p)) = (self.dbg.as_mut(), d.path.clone()) {
            s.set_break(&p, line, on);
        }
        self.save_session();
    }

    fn zoom(&mut self, d: f32, ctx: &egui::Context) {
        self.font_size = (self.font_size + d).clamp(9.0, 28.0);
        theme::apply(ctx, &self.p, self.font_size);
    }

    fn set_theme(&mut self, name: &str, ctx: &egui::Context) {
        self.theme = name.to_string();
        self.p = theme::palette(name);
        theme::apply(ctx, &self.p, self.font_size);
        self.save_session();
    }

    fn is_python(&self) -> bool {
        match self.cur() {
            Some(i) => self.tabs[i].doc().map(|d| d.lang == crate::highlight::Lang::Python && d.remote.is_none()).unwrap_or(false),
            None => false,
        }
    }

    fn debugging(&self) -> bool {
        self.dbg.is_some()
    }

    fn running(&self) -> bool {
        self.run.as_ref().map(|r| !r.finished()).unwrap_or(false)
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        if self.pending_end.is_some() { ctx.request_repaint_after(Duration::from_millis(40)); }
        self.leader_input(&ctx);
        self.shortcuts(&ctx);
        if std::mem::take(&mut self.focus_terminal) {
            match self.bottom {
                Bottom::Terminal(n) => { if let Some(t) = self.terms.get_mut(n) { t.want_focus = true; } }
                Bottom::Ssh(i, n) => { if let Some(t) = self.sessions.get_mut(i).and_then(|s| s.terms.get_mut(n)) { t.want_focus = true; } }
                Bottom::Repl => self.repl_term.want_focus = true,
                _ => { self.bottom = Bottom::Terminal(0); if let Some(t) = self.terms.get_mut(0) { t.want_focus = true; } }
            }
        }
        self.window_edges(&ctx);
        let title = match self.cur() {
            Some(i) => {
                let t = &self.tabs[i];
                let name = t.title();
                format!("{}{name} — SCode", if t.dirty() { "* " } else { "" })
            }
            None => "Accueil — SCode".into(),
        };
        if title != self.title {
            self.title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_quit {
            self.save_session();
            if self.tabs.iter().any(|t| t.dirty()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.dialog = Some(Dialog::Quit);
            }
        }
        let p = self.p;
        self.top_bar(ui, &ctx);
        if self.cfg.show_statusbar { self.status_bar(ui); }
        // panneaux à la place choisie (Paramètres ou clic droit sur leurs onglets) ; 3 px de fond sombre
        // du côté de l'éditeur, comme la version Python
        let side_margin = |toward_right: bool| egui::Margin { left: if toward_right { 0 } else { 3 }, right: if toward_right { 3 } else { 0 }, top: 0, bottom: 0 };
        if self.show_explorer {
            let fr = egui::Frame::new().fill(p.bg_dark).inner_margin(side_margin(!self.cfg.side_right));
            let panel = if self.cfg.side_right { egui::Panel::right("side") } else { egui::Panel::left("side") };
            panel.resizable(true).default_size(262.0).size_range(215.0..=560.0).show_separator_line(false)
                .frame(fr).show(ui, |ui| self.side_panel(ui));
        }
        if self.debugging() {
            let fr = egui::Frame::new().fill(p.bg_dark).inner_margin(side_margin(self.cfg.debug_left));
            let panel = if self.cfg.debug_left { egui::Panel::left("debug") } else { egui::Panel::right("debug") };
            panel.resizable(true).default_size(380.0).size_range(220.0..=700.0).show_separator_line(false)
                .frame(fr).show(ui, |ui| self.debug_panel(ui));
        }
        if self.show_bottom {
            if self.cfg.bottom_right {
                egui::Panel::right("bottom_r").resizable(true).default_size(460.0).size_range(240.0..=1200.0)
                    .show_separator_line(false)
                    .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin { left: 3, right: 0, top: 0, bottom: 0 }))
                    .show(ui, |ui| self.bottom_panel(ui));
            } else {
                egui::Panel::bottom("bottom").resizable(true).default_size(212.0).size_range(90.0..=800.0)
                    .show_separator_line(false)
                    .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin { left: 0, right: 0, top: 3, bottom: 0 }))
                    .show(ui, |ui| self.bottom_panel(ui));
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg_dark)).show(ui, |ui| {
            if self.tabs.is_empty() {
                self.start_page(ui);
            } else {
                // la barre de recherche est sous les volets, comme dans la version Python
                let mut rect = ui.available_rect_before_wrap();
                if self.find_open {
                    let bar = egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 36.0), rect.max);
                    rect.max.y -= 36.0;
                    ui.painter().rect_filled(bar, 0.0, p.bg_dark);
                    let mut bu = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(Vec2::new(0.0, 4.0))));
                    self.find_bar(&mut bu);
                }
                let layout = self.layout.clone();
                let new_layout = self.layout_ui(ui, &layout, rect);
                self.layout = new_layout;
                self.apply_pane_ops();
                ui.allocate_rect(rect, egui::Sense::hover());
            }
        });
        if self.libs_open {
            let mut open = true;
            egui::Window::new("Bibliothèques Python").open(&mut open).default_size([720.0, 560.0]).collapsible(false)
                .pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.content_rect().center())
                .frame(egui::Frame::window(&ctx.global_style()).fill(p.bg_dark).inner_margin(egui::Margin::same(10)))
                .show(&ctx, |ui| self.libs_panel(ui));
            self.libs_open = open;
        }
        for (o, k) in std::mem::take(&mut self.vim_queue) { self.vim_events(o, k, &ctx); }
        self.settings_window(&ctx);
        self.picker_window(&ctx);
        self.which_key(&ctx);
        self.dialogs(&ctx);
        if ctx.input(|i| i.pointer.any_released()) { self.drag_tab = None; }
        if self.cfg_dirty && !ctx.input(|i| i.pointer.any_down()) {
            self.cfg.save();
            self.cfg_dirty = false;
        }
        self.autosave_files(&ctx);
        self.autosave_session();
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_session();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for t in &self.tabs {
            if let Content::Pdf(p) = &t.c { p.save_position(); }
        }
        self.save_session();
        self.stop();
        for t in self.terms.iter_mut() { t.kill(); }
        self.repl_term.kill();
        for s in self.sessions.iter_mut() { for t in s.terms.iter_mut() { t.kill(); } }
    }
}

#[allow(dead_code)]
fn _unused(_: FontId, _: RichText, _: Vec2) {}
