//! Fenêtre principale de StudentCode : état de l'application et actions. Le dessin est dans
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
use std::time::Instant;

mod panes;
mod side;
mod ui;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const URCA: &str = "https://cours.univ-reims.fr/local/mycoursesurca/courses.php";

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
enum Bottom { Terminal, Output, Ssh(usize) }

#[derive(PartialEq, Clone, Copy)]
enum SideTab { Files, Ssh, Vpn, Web, Libs }

#[derive(PartialEq, Clone, Copy)]
enum Mode { Plain, PyConsole, Debug, Remote }

#[derive(Clone, Copy, PartialEq)]
enum OutKind { Normal, Error, Info, Echo }

enum Dialog {
    NewFile(PathBuf, String),
    NewDir(PathBuf, String),
    Rename(PathBuf, String),
    Delete(PathBuf),
    CloseUnsaved(u64),
    Quit,
    Args(String),
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
    term: Terminal,
    listing: HashMap<String, Vec<RemoteEntry>>,
    expanded: BTreeSet<String>,
    connected: bool,
    failed: Option<String>,
    selected: Option<String>,
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
    term: Terminal,
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
        theme::load_fonts(&cc.egui_ctx);
        let sess: SessionFile = config_dir().and_then(|d| std::fs::read_to_string(d.join("session.json")).ok())
            .and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let theme_name = if sess.theme.is_empty() { theme::THEMES[0].0.to_string() } else { sess.theme.clone() };
        let font_size = if sess.font_size >= 9.0 { sess.font_size } else { 14.0 };
        let p = theme::palette(&theme_name);
        theme::apply(&cc.egui_ctx, &p, font_size);
        let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        let mut app = App {
            p, theme: theme_name, font_size, root: home, tabs: Vec::new(), layout: Layout::Leaf(0),
            pane_active: HashMap::new(), focus_pane: 0, next_uid: 1, next_pane: 1, drag_tab: None,
            explorer: Explorer::new(), term: Terminal::new(), output: Vec::new(), out_input: String::new(),
            run: None, mode: Mode::Plain, repl: false, dbg: None, frames: Vec::new(), frame_idx: 0,
            locals: Vec::new(), globals: Vec::new(), children: HashMap::new(), pending_children: HashMap::new(),
            dbg_line: None, error_line: None, module: pyscripter::module_dir(), pdf_module: crate::pdf::module_lib().is_some(),
            bottom: if sess.output_tab { Bottom::Output } else { Bottom::Terminal },
            side: match sess.side { 1 => SideTab::Ssh, 2 => SideTab::Vpn, 3 => SideTab::Web, 4 => SideTab::Libs, _ => SideTab::Files },
            show_explorer: !sess.hide_explorer, show_bottom: !sess.hide_bottom, find_open: false,
            find_text: String::new(), replace_text: String::new(), find_info: String::new(), dialog: None,
            status: None, recent: sess.recent.clone(), run_args: sess.run_args.clone(), allow_quit: false,
            title: String::new(), new_count: 0, ctx: cc.egui_ctx.clone(), logo: None,
            labs: ssh::load_labs(), sessions: Vec::new(), ssh_sel: 0, quick: String::new(), diag: None,
            vpns: Vec::new(), vpn_sel: None, vpn_list: None, vpn_job: None, vpn_status: String::new(),
            vpn_creds: HashMap::new(), vpn_refreshed: None,
            bookmarks: if sess.bookmarks.is_empty() { vec![("Cours URCA".into(), URCA.into())] } else { sess.bookmarks.clone() },
            pkgs: Vec::new(), pkg_rx: None, pkg_log: String::new(), pkg_name: String::new(), pkg_filter: String::new(),
            pkg_busy: false, pkg_loaded: false, pane_ops: Vec::new(), session_sig: 0, pending_saves: HashMap::new(),
        };
        if app.side == SideTab::Libs && app.module.is_none() { app.side = SideTab::Files; }
        app.explorer.expanded = sess.expanded.iter().cloned().collect();
        match arg {
            // « studentcode dossier » : on ouvre ce dossier, sans restaurer les fichiers de la session
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
        let mut bps = BTreeMap::new();
        for t in &self.tabs {
            if let Some(d) = t.doc() {
                if let (Some(p), false) = (&d.path, d.breakpoints.is_empty()) {
                    bps.insert(p.to_string_lossy().to_string(), d.breakpoints.iter().copied().collect());
                }
            }
        }
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
            side: match self.side { SideTab::Files => 0, SideTab::Ssh => 1, SideTab::Vpn => 2, SideTab::Web => 3, SideTab::Libs => 4 },
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
        self.term.cd(&dir); // le terminal suit le dossier ouvert
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
        self.pane_active.insert(pane, uid);
        self.focus_pane = pane;
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
                Ok(d) => Content::Doc(d),
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
    }

    fn new_doc(&mut self) {
        self.new_count += 1;
        self.add_tab(Content::Doc(Doc::new_empty(self.new_count)));
    }

    fn save_tab(&mut self, i: usize) -> bool {
        let Some(d) = self.tabs[i].doc_mut() else { return true };
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
        if self.pane_active.get(&t.pane) == Some(&t.uid) {
            // l'onglet voisin du même volet devient visible
            if let Some(n) = self.tabs.iter().rev().find(|x| x.pane == t.pane) {
                self.pane_active.insert(t.pane, n.uid);
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
        // enregistrer avant d'exécuter, comme les autres IDE
        for k in 0..self.tabs.len() {
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
            self.output.clear();
            self.show_bottom = true;
            self.bottom = Bottom::Output;
            self.out(&format!("▶ {} sur {label}\n", rpath), OutKind::Info);
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
        self.output.clear();
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
        self.out(&format!("▶ {}{label}\n", if debug { "Débogage de " } else { "" }), OutKind::Info);
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
                        self.out(&format!("\n— Programme terminé (code {code}) · console Python ouverte sur ses variables —\n"), OutKind::Info);
                        self.repl = true;
                        self.dbg = None;
                        self.frames.clear();
                        self.dbg_line = None;
                        self.out(after, OutKind::Normal);
                    } else {
                        self.out(&t, OutKind::Normal);
                    }
                }
                Out::Stderr(t) => self.out(&runner::strip_ansi(&t), OutKind::Error),
            }
        }
        if let Some(code) = exit {
            let msg = match code {
                Some(0) => "\n— Terminé —\n".to_string(),
                Some(c) => format!("\n— Terminé avec le code {c} —\n"),
                None => "\n— Arrêté —\n".to_string(),
            };
            if !self.repl {
                self.out(&msg, OutKind::Info);
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
        let line = std::mem::take(&mut self.out_input);
        if let Some(s) = self.dbg.as_mut().filter(|_| !self.frames.is_empty()) {
            // en pause dans le débogueur : la ligne est évaluée dans le contexte courant
            self.output.push((format!("» {line}\n"), OutKind::Echo));
            s.eval(self.frame_idx, &line);
            if let Some(s) = self.dbg.as_mut() { s.request_vars(self.frame_idx); }
            return;
        }
        if let Some(r) = self.run.as_mut().filter(|r| !r.finished()) {
            r.write(&format!("{line}\n"));
            self.out(&format!("{line}\n"), OutKind::Echo);
        } else {
            self.say("Aucun programme en cours : lance-le avec F5.", self.p.yellow);
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
                self.term.cd(&d);
                self.show_bottom = true;
                self.bottom = Bottom::Terminal;
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

    // ------------------------------------------------------------------ SSH
    pub(crate) fn connect_lab(&mut self, lab: Lab, password: Option<String>) {
        if let Some(i) = self.sessions.iter().position(|s| s.conn.lab.key() == lab.key()) {
            if self.sessions[i].connected || self.sessions[i].failed.is_none() {
                self.ssh_sel = i;
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
                let term = Terminal::new_ssh(lab, pw);
                self.sessions.push(SshSession { conn, term, listing: HashMap::new(), expanded: BTreeSet::new(),
                    connected: false, failed: None, selected: None });
                self.ssh_sel = self.sessions.len() - 1;
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
        s.term.kill();
        if self.bottom == Bottom::Ssh(i) { self.bottom = Bottom::Terminal; }
        if let Bottom::Ssh(k) = self.bottom { if k > i { self.bottom = Bottom::Ssh(k - 1); } }
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
                    s.conn.send(Req::List(home));
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
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let k = |m: Modifiers, key: Key| ctx.input_mut(|i| i.consume_key(m, key));
        // dans un terminal, les touches Ctrl+lettre appartiennent au shell (Ctrl+W, Ctrl+R…) :
        // seuls F5 / F6 / Maj+F5 et Ctrl+` restent à l'app
        let in_term = self.term.focused && self.bottom == Bottom::Terminal
            || matches!(self.bottom, Bottom::Ssh(i) if self.sessions.get(i).map(|s| s.term.focused).unwrap_or(false));
        if in_term && self.show_bottom {
            if k(Modifiers::COMMAND, Key::Backtick) { self.show_bottom = false; }
            if k(Modifiers::COMMAND, Key::S) { if let Some(i) = self.cur() { self.save_tab(i); } }
            if k(Modifiers::SHIFT, Key::F5) { self.stop(); }
            if k(Modifiers::NONE, Key::F5) { self.start(false); }
            if k(Modifiers::NONE, Key::F6) { self.start(true); }
            return;
        }
        if k(Modifiers::COMMAND, Key::S) {
            if let Some(i) = self.cur() { self.save_tab(i); }
        }
        if k(Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
            for i in 0..self.tabs.len() { if self.tabs[i].dirty() { self.save_tab(i); } }
        }
        if k(Modifiers::COMMAND, Key::O) { self.open_folder_dialog(); }
        if k(Modifiers::COMMAND, Key::N) { self.new_doc(); }
        if k(Modifiers::COMMAND, Key::W) {
            if let Some(i) = self.cur() { self.close_tab(i, false); }
        }
        if k(Modifiers::COMMAND, Key::F) {
            self.find_open = true;
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("find")));
        }
        if self.find_open && k(Modifiers::NONE, Key::Escape) {
            self.find_open = false;
            if let Some(d) = self.cur_doc() { d.find = None; d.focus(ctx); }
        }
        if k(Modifiers::COMMAND, Key::B) { self.show_explorer = !self.show_explorer; }
        if k(Modifiers::COMMAND, Key::J) || k(Modifiers::COMMAND, Key::Backtick) { self.show_bottom = !self.show_bottom; }
        if k(Modifiers::COMMAND, Key::Backslash) {
            if let Some(i) = self.cur() { let (u, p) = (self.tabs[i].uid, self.tabs[i].pane); self.split_tab(u, p, false); }
        }
        if k(Modifiers::COMMAND, Key::Tab) {
            let pane = self.focus_pane;
            let ids: Vec<usize> = (0..self.tabs.len()).filter(|i| self.tabs[*i].pane == pane).collect();
            if !ids.is_empty() {
                let cur = self.cur().and_then(|c| ids.iter().position(|x| *x == c)).unwrap_or(0);
                self.activate(ids[(cur + 1) % ids.len()]);
            }
        }
        if k(Modifiers::COMMAND, Key::Plus) || k(Modifiers::COMMAND, Key::Equals) { self.zoom(1.0, ctx); }
        if k(Modifiers::COMMAND, Key::Minus) { self.zoom(-1.0, ctx); }
        if k(Modifiers::SHIFT, Key::F5) { self.stop(); }
        if k(Modifiers::NONE, Key::F5) { self.start(false); }
        if k(Modifiers::NONE, Key::F6) { self.start(true); }
        if self.dbg.is_some() && !self.frames.is_empty() {
            if k(Modifiers::NONE, Key::F10) { self.dbg_cmd("next"); }
            if k(Modifiers::SHIFT, Key::F11) { self.dbg_cmd("return"); }
            if k(Modifiers::NONE, Key::F11) { self.dbg_cmd("step"); }
        }
        if k(Modifiers::NONE, Key::F9) {
            if let Some(i) = self.cur() {
                if let Some(line) = self.tabs[i].doc().map(|d| d.cursor_line) { self.toggle_breakpoint(i, line); }
            }
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
        self.shortcuts(&ctx);
        self.window_edges(&ctx);
        let title = match self.cur() {
            Some(i) => {
                let t = &self.tabs[i];
                let remote = t.doc().and_then(|d| d.remote.as_ref()).map(|(_, _, l)| format!(" — {l}")).unwrap_or_default();
                format!("{}{}{remote} — StudentCode", if t.dirty() { "* " } else { "" }, t.title())
            }
            None => "StudentCode".into(),
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
        self.status_bar(ui);
        if self.show_explorer {
            egui::Panel::left("side").resizable(true).default_size(250.0).size_range(160.0..=560.0)
                .frame(egui::Frame::new().fill(p.bg_dark).inner_margin(egui::Margin::symmetric(4, 4)))
                .show(ui, |ui| self.side_panel(ui));
        }
        if self.debugging() {
            egui::Panel::right("debug").resizable(true).default_size(300.0).size_range(200.0..=600.0)
                .frame(egui::Frame::new().fill(p.bg_dark).inner_margin(egui::Margin::same(6)))
                .show(ui, |ui| self.debug_panel(ui));
        }
        if self.show_bottom {
            egui::Panel::bottom("bottom").resizable(true).default_size(230.0).size_range(90.0..=800.0)
                .frame(egui::Frame::new().fill(p.bg_dark).inner_margin(egui::Margin::symmetric(4, 2)))
                .show(ui, |ui| self.bottom_panel(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            if self.tabs.is_empty() {
                self.start_page(ui);
            } else {
                if self.find_open { self.find_bar(ui); }
                let rect = ui.available_rect_before_wrap();
                let layout = self.layout.clone();
                let new_layout = self.layout_ui(ui, &layout, rect);
                self.layout = new_layout;
                self.apply_pane_ops();
                ui.allocate_rect(rect, egui::Sense::hover());
            }
        });
        self.dialogs(&ctx);
        if ctx.input(|i| i.pointer.any_released()) { self.drag_tab = None; }
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
        self.term.kill();
        for s in self.sessions.iter_mut() { s.term.kill(); }
    }
}

#[allow(dead_code)]
fn _unused(_: FontId, _: RichText, _: Vec2) {}
