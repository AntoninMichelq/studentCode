//! Raccourcis à la LazyVim : touche leader (Espace) avec la fenêtre which-key, recherche de
//! fichiers / texte / onglets façon Telescope, Harpoon, et commandes « : » de Vim.
use super::*;
use crate::vim::{Out, K};
use egui::{Rect, RichText, Sense, Stroke};

/// Ce que fait un raccourci leader.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum L {
    FindFiles, Grep, GrepWord, Buffers, Recent, NewFile, Config, Explorer, SplitBelow, SplitRight,
    CloseWindow, OtherWindow, Maximize, Harpoon(usize), HarpoonAdd, HarpoonMenu, AltBuffer, BufDelete,
    BufOthers, QuitAll, Terminal, LazyGit, Wrap, Numbers, Relative, Guides, Themes, Background, Keymaps,
    SearchBuffer, Replace, Settings, Libs, Scratch, Run, DbgBreak, DbgContinue, DbgInto, DbgOver, DbgOut,
    DbgPause, DbgStop, DbgRepl, DbgEval, Save, Ssh, Vpn, VimToggle,
}

/// Groupes (« +debug »…) : préfixe, nom.
const GROUPS: &[(&str, &str)] = &[
    ("b", "+buffer"), ("c", "+code"), ("d", "+debug"), ("f", "+file/find"), ("g", "+git"), ("q", "+quit/session"),
    ("s", "+search"), ("u", "+ui"), ("w", "+windows"), ("r", "+run"),
];

/// Raccourcis leader de LazyVim (et de l'extra Harpoon), avec leur équivalent dans SCode.
pub(crate) const KEYMAP: &[(&str, &str, L)] = &[
    (" ", "Find Files (Root Dir)", L::FindFiles),
    (",", "Switch Buffer", L::Buffers),
    ("/", "Grep (Root Dir)", L::Grep),
    ("`", "Switch to Other Buffer", L::AltBuffer),
    ("-", "Split Window Below", L::SplitBelow),
    ("|", "Split Window Right", L::SplitRight),
    (".", "Toggle Scratch Buffer", L::Scratch),
    ("e", "Explorer (Root Dir)", L::Explorer),
    ("E", "Explorer (cwd)", L::Explorer),
    ("1", "Harpoon to File 1", L::Harpoon(1)), ("2", "Harpoon to File 2", L::Harpoon(2)), ("3", "Harpoon to File 3", L::Harpoon(3)),
    ("4", "Harpoon to File 4", L::Harpoon(4)), ("5", "Harpoon to File 5", L::Harpoon(5)), ("6", "Harpoon to File 6", L::Harpoon(6)),
    ("7", "Harpoon to File 7", L::Harpoon(7)), ("8", "Harpoon to File 8", L::Harpoon(8)), ("9", "Harpoon to File 9", L::Harpoon(9)),
    ("H", "Harpoon File", L::HarpoonAdd),
    ("h", "Harpoon Quick Menu", L::HarpoonMenu),
    ("l", "Lazy (Paramètres)", L::Settings),
    ("bb", "Switch to Other Buffer", L::AltBuffer),
    ("bd", "Delete Buffer", L::BufDelete),
    ("bo", "Delete Other Buffers", L::BufOthers),
    ("cr", "Run File", L::Run),
    ("db", "Toggle Breakpoint", L::DbgBreak),
    ("dc", "Run/Continue", L::DbgContinue),
    ("de", "Eval", L::DbgEval),
    ("di", "Step Into", L::DbgInto),
    ("do", "Step Out", L::DbgOut),
    ("dO", "Step Over", L::DbgOver),
    ("dP", "Pause", L::DbgPause),
    ("dr", "Toggle REPL", L::DbgRepl),
    ("dt", "Terminate", L::DbgStop),
    ("fb", "Buffers", L::Buffers),
    ("fc", "Find Config File", L::Config),
    ("ff", "Find Files (Root Dir)", L::FindFiles),
    ("fF", "Find Files (cwd)", L::FindFiles),
    ("fn", "New File", L::NewFile),
    ("fr", "Recent", L::Recent),
    ("ft", "Terminal (Root Dir)", L::Terminal),
    ("fT", "Terminal (cwd)", L::Terminal),
    ("fs", "Save File", L::Save),
    ("gg", "Lazygit (Root Dir)", L::LazyGit),
    ("qq", "Quit All", L::QuitAll),
    ("rr", "Run File", L::Run),
    ("rs", "SSH (labs)", L::Ssh),
    ("rv", "VPN", L::Vpn),
    ("rl", "Python Libraries (pip)", L::Libs),
    ("sb", "Buffer Lines", L::SearchBuffer),
    ("sg", "Grep (Root Dir)", L::Grep),
    ("sk", "Keymaps", L::Keymaps),
    ("sr", "Search and Replace", L::Replace),
    ("sw", "Visual selection or word", L::GrepWord),
    ("ub", "Toggle Dark Background", L::Background),
    ("uC", "Colorschemes", L::Themes),
    ("ug", "Toggle Indent Guides", L::Guides),
    ("ul", "Toggle Line Numbers", L::Numbers),
    ("uL", "Toggle Relative Number", L::Relative),
    ("uw", "Toggle Wrap", L::Wrap),
    ("uv", "Toggle Vim Mode", L::VimToggle),
    ("uz", "Toggle Zen Mode", L::Maximize),
    ("wd", "Delete Window", L::CloseWindow),
    ("wm", "Toggle Zoom Mode", L::Maximize),
    ("ws", "Split Window Below", L::SplitBelow),
    ("wv", "Split Window Right", L::SplitRight),
    ("ww", "Other Window", L::OtherWindow),
    ("w-", "Split Window Below", L::SplitBelow),
    ("w|", "Split Window Right", L::SplitRight),
];

/// Ordre de which-key : chiffres, lettres (minuscule puis majuscule), puis signes.
fn wk_order(c: char) -> (u8, char, bool) {
    if c.is_ascii_digit() { (0, c, false) } else if c.is_alphabetic() { (1, c.to_ascii_lowercase(), c.is_uppercase()) } else { (2, c, false) }
}

// ---------------------------------------------------------------------- recherche (Telescope)
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PickKind { Files, Grep, Buffers, Recent, Themes, Harpoon, Keymaps }

#[derive(Clone)]
pub(crate) enum PickAct { Open(PathBuf, Option<usize>), Tab(u64), Theme(String), Leader(String) }

#[derive(Clone)]
pub(crate) struct PItem { label: String, detail: String, act: PickAct }

pub(crate) struct Picker {
    kind: PickKind,
    title: String,
    query: String,
    items: Vec<PItem>,
    shown: Vec<usize>,
    sel: usize,
    first: bool,
    searched: Option<String>,
}

/// Score de correspondance « floue » (lettres dans l'ordre), None si elles n'y sont pas toutes.
fn fuzzy(q: &str, s: &str) -> Option<i32> {
    if q.is_empty() { return Some(0); }
    let sl: Vec<char> = s.to_lowercase().chars().collect();
    let mut score = 0i32;
    let mut pos = 0usize;
    let mut prev: Option<usize> = None;
    let name_start = s.rfind(['/', '\\']).map(|p| s[..p + 1].chars().count()).unwrap_or(0);
    for qc in q.to_lowercase().chars() {
        if qc == ' ' { continue; }
        let k = (pos..sl.len()).find(|&k| sl[k] == qc)?;
        score += 10;
        if prev == Some(k.wrapping_sub(1)) { score += 15; }
        if k >= name_start { score += 6; }
        if k == 0 || matches!(sl.get(k.wrapping_sub(1)), Some('/' | '_' | '-' | '.' | ' ')) { score += 8; }
        score -= (k - pos).min(20) as i32 / 4;
        prev = Some(k);
        pos = k + 1;
    }
    Some(score - sl.len() as i32 / 8)
}

/// Fichiers du projet (sans .git, target, node_modules…), comme le sélecteur de LazyVim.
fn project_files(root: &Path, limit: usize) -> Vec<PathBuf> {
    const SKIP: &[&str] = &[".git", "target", "node_modules", "__pycache__", ".venv", "venv", "build", "dist", ".idea", ".vscode", ".cache", ".gradle", ".mypy_cache", ".pytest_cache"];
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut ents: Vec<_> = rd.flatten().collect();
        ents.sort_by_key(|e| e.file_name());
        for e in ents {
            let name = e.file_name().to_string_lossy().to_string();
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !SKIP.contains(&name.as_str()) && !(name.starts_with('.') && name != ".github") { stack.push(p); }
            } else if ft.is_file() {
                out.push(p);
                if out.len() >= limit { return out; }
            }
        }
    }
    out
}

impl App {
    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root).map(|r| r.to_string_lossy().to_string()).unwrap_or_else(|_| p.to_string_lossy().to_string())
    }

    pub(crate) fn open_picker(&mut self, kind: PickKind, query: &str) {
        let mut items = Vec::new();
        let title = match kind {
            PickKind::Files => {
                for p in project_files(&self.root.clone(), 20000) { items.push(PItem { label: self.rel(&p), detail: String::new(), act: PickAct::Open(p, None) }); }
                "Fichiers"
            }
            PickKind::Grep => "Grep",
            PickKind::Buffers => {
                for t in &self.tabs {
                    let detail = t.path().map(|p| self.rel(p)).unwrap_or_default();
                    items.push(PItem { label: t.title(), detail, act: PickAct::Tab(t.uid) });
                }
                "Buffers"
            }
            PickKind::Recent => {
                for p in self.recent_files.clone() { if p.exists() { items.push(PItem { label: self.rel(&p), detail: String::new(), act: PickAct::Open(p, None) }); } }
                "Fichiers récents"
            }
            PickKind::Themes => {
                for (n, _) in crate::theme::THEMES { items.push(PItem { label: n.to_string(), detail: if *n == self.theme { "actuel".into() } else { String::new() }, act: PickAct::Theme(n.to_string()) }); }
                "Thèmes"
            }
            PickKind::Harpoon => {
                for (k, p) in self.harpoon_list().iter().enumerate() {
                    items.push(PItem { label: format!("{}  {}", k + 1, self.rel(p)), detail: "Suppr : retirer".into(), act: PickAct::Open(p.clone(), None) });
                }
                "Harpoon"
            }
            PickKind::Keymaps => {
                for (k, d, _) in KEYMAP { items.push(PItem { label: format!("␣{}  {d}", k.replace(' ', "␣")), detail: String::new(), act: PickAct::Leader(k.to_string()) }); }
                "Raccourcis leader"
            }
        };
        let mut p = Picker { kind, title: title.into(), query: query.to_string(), items, shown: Vec::new(), sel: 0, first: true, searched: None };
        self.refilter(&mut p);
        self.picker = Some(p);
    }

    fn refilter(&self, p: &mut Picker) {
        if p.kind == PickKind::Grep {
            if p.searched.as_deref() == Some(p.query.as_str()) { return; }
            p.searched = Some(p.query.clone());
            p.items.clear();
            let q = p.query.clone();
            if q.chars().count() >= 2 {
                let ci = !q.chars().any(|c| c.is_uppercase());
                'files: for f in project_files(&self.root, 5000) {
                    let Ok(meta) = std::fs::metadata(&f) else { continue };
                    if meta.len() > 1_000_000 { continue; }
                    let Ok(txt) = std::fs::read_to_string(&f) else { continue };
                    for (n, line) in txt.lines().enumerate() {
                        let hit = if ci { !crate::editor::find_all_ci(line, &q, 1).is_empty() } else { line.contains(&q) };
                        if hit {
                            p.items.push(PItem { label: format!("{}:{}", self.rel(&f), n + 1), detail: line.trim().chars().take(160).collect(), act: PickAct::Open(f.clone(), Some(n + 1)) });
                            if p.items.len() >= 800 { break 'files; }
                        }
                    }
                }
            }
            p.shown = (0..p.items.len()).collect();
        } else {
            let mut v: Vec<(i32, usize)> = p.items.iter().enumerate()
                .filter_map(|(i, it)| fuzzy(&p.query, &it.label).map(|s| (s, i))).collect();
            if !p.query.is_empty() { v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1))); }
            p.shown = v.into_iter().map(|x| x.1).take(500).collect();
        }
        p.sel = p.sel.min(p.shown.len().saturating_sub(1));
    }

    pub(super) fn picker_window(&mut self, ctx: &egui::Context) {
        let Some(mut pk) = self.picker.take() else { return };
        let p = self.p;
        let mut close = false;
        let mut chosen: Option<usize> = None;
        let mut remove: Option<usize> = None;
        // clavier : ↑ ↓ (ou Ctrl+j / Ctrl+k / Ctrl+n / Ctrl+p), Entrée, Échap
        ctx.input_mut(|i| {
            let n = pk.shown.len();
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) { close = true; }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) || i.consume_key(egui::Modifiers::CTRL, egui::Key::J) || i.consume_key(egui::Modifiers::CTRL, egui::Key::N) {
                if n > 0 { pk.sel = (pk.sel + 1) % n; }
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) || i.consume_key(egui::Modifiers::CTRL, egui::Key::K) || i.consume_key(egui::Modifiers::CTRL, egui::Key::P) {
                if n > 0 { pk.sel = (pk.sel + n - 1) % n; }
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) && n > 0 { chosen = Some(pk.shown[pk.sel]); }
            if pk.kind == PickKind::Harpoon && i.consume_key(egui::Modifiers::NONE, egui::Key::Delete) && n > 0 { remove = Some(pk.shown[pk.sel]); }
        });
        let screen = ctx.content_rect();
        let w = (screen.width() * 0.7).clamp(360.0, 900.0);
        let h = (screen.height() * 0.62).clamp(220.0, 620.0);
        egui::Area::new(egui::Id::new("picker")).order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen.center().x - w / 2.0, screen.min.y + screen.height() * 0.14))
            .show(ctx, |ui| {
                egui::Frame::new().fill(p.bg_dark).stroke(Stroke::new(1.0, p.blue)).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
                    ui.set_width(w);
                    ui.set_height(h);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!(" {} ", pk.title)).strong().color(p.bg_dark).background_color(p.blue));
                        ui.label(RichText::new(format!("{}/{}", pk.shown.len(), pk.items.len())).color(p.comment));
                    });
                    ui.add_space(4.0);
                    let r = ui.add(egui::TextEdit::singleline(&mut pk.query).id(egui::Id::new("picker_q")).desired_width(f32::INFINITY)
                        .hint_text(if pk.kind == PickKind::Grep { "texte à chercher dans les fichiers du projet…" } else { "chercher…" })
                        .font(FontId::monospace(14.0)));
                    if pk.first { r.request_focus(); pk.first = false; }
                    if r.changed() { pk.sel = 0; self.refilter(&mut pk); }
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        for (row, &ix) in pk.shown.iter().enumerate() {
                            let it = &pk.items[ix];
                            let sel = row == pk.sel;
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
                            if sel || resp.hovered() { ui.painter().rect_filled(rect, 4.0, if sel { p.bg_hl } else { p.bg }); }
                            if sel { ui.painter().rect_filled(Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())), 1.0, p.blue); }
                            let font = FontId::monospace(13.0);
                            let g = ui.painter().layout_no_wrap(it.label.clone(), font.clone(), if sel { p.fg } else { p.fg.gamma_multiply(0.85) });
                            let lw = g.size().x;
                            ui.painter().galley(egui::pos2(rect.min.x + 10.0, rect.center().y - g.size().y / 2.0), g, p.fg);
                            if !it.detail.is_empty() {
                                ui.painter().with_clip_rect(rect).text(egui::pos2(rect.min.x + 24.0 + lw, rect.center().y), egui::Align2::LEFT_CENTER, &it.detail, font, p.comment);
                            }
                            if sel { resp.scroll_to_me(None); }
                            if resp.clicked() { chosen = Some(ix); }
                        }
                        if pk.shown.is_empty() {
                            ui.label(RichText::new(if pk.kind == PickKind::Grep && pk.query.chars().count() < 2 { "Tape au moins 2 caractères." } else { "Aucun résultat." }).color(p.comment));
                        }
                    });
                });
            });
        if let Some(ix) = remove {
            if let Some(PickAct::Open(path, _)) = pk.items.get(ix).map(|i| i.act.clone()) {
                let key = self.root.to_string_lossy().to_string();
                if let Some(v) = self.harpoon.get_mut(&key) { v.retain(|x| x != &path); }
                self.save_session();
                self.open_picker(PickKind::Harpoon, "");
                return;
            }
        }
        if let Some(ix) = chosen {
            let act = pk.items[ix].act.clone();
            self.picker = None;
            match act {
                PickAct::Open(path, line) => {
                    self.open_file(&path);
                    if let (Some(l), Some(d)) = (line, self.cur_doc()) { let c = ctx.clone(); d.goto_line(&c, l); }
                }
                PickAct::Tab(uid) => { if let Some(i) = self.tabs.iter().position(|t| t.uid == uid) { self.activate(i); } }
                PickAct::Theme(n) => self.set_theme(&n, ctx),
                PickAct::Leader(k) => self.leader_run(&k, ctx),
            }
            return;
        }
        if !close { self.picker = Some(pk); } else if let Some(d) = self.cur_doc() { d.want_focus = true; }
    }

    // ------------------------------------------------------------------ Harpoon
    fn harpoon_list(&self) -> Vec<PathBuf> {
        self.harpoon.get(&self.root.to_string_lossy().to_string()).cloned().unwrap_or_default()
    }

    // ------------------------------------------------------------------ touche leader
    /// Touches tapées après Espace : on cherche le raccourci, which-key s'affiche en attendant.
    pub(super) fn leader_input(&mut self, ctx: &egui::Context) {
        let Some((mut seq, since)) = self.leader.take() else { return };
        let mut cancel = false;
        let mut typed: Vec<char> = std::mem::take(&mut self.leader_pending);
        ctx.input_mut(|i| {
            let mut keep = Vec::new();
            for e in std::mem::take(&mut i.events) {
                match &e {
                    egui::Event::Key { key: egui::Key::Escape, pressed: true, .. } => cancel = true,
                    egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => { if seq.pop().is_none() { cancel = true; } }
                    egui::Event::Key { pressed: true, modifiers, .. } if !(modifiers.ctrl || modifiers.command) => {}
                    egui::Event::Key { .. } => keep.push(e),
                    egui::Event::Text(t) => typed.extend(t.chars()),
                    _ => keep.push(e),
                }
            }
            i.events = keep;
        });
        if cancel { if let Some(d) = self.cur_doc() { d.want_focus = true; } return; }
        for c in typed {
            seq.push(c);
            if let Some((_, _, _)) = KEYMAP.iter().find(|(k, _, _)| *k == seq) {
                let s = seq.clone();
                self.leader_run(&s, ctx);
                return;
            }
            if !KEYMAP.iter().any(|(k, _, _)| k.starts_with(seq.as_str())) {
                self.say(format!("Pas de raccourci pour ␣{}", seq.replace(' ', "␣")), self.p.yellow);
                return;
            }
        }
        self.leader = Some((seq, since));
        ctx.request_repaint_after(Duration::from_millis(220));
    }

    /// Fenêtre which-key : les touches possibles après ce qui a déjà été tapé.
    pub(super) fn which_key(&mut self, ctx: &egui::Context) {
        let Some((seq, since)) = self.leader.clone() else { return };
        if since.elapsed() < Duration::from_millis(200) { return; }
        let p = self.p;
        let mut rows: Vec<(char, String, bool)> = Vec::new();
        for (k, d, _) in KEYMAP {
            if let Some(rest) = k.strip_prefix(seq.as_str()) {
                let mut it = rest.chars();
                let Some(c) = it.next() else { continue };
                if rows.iter().any(|r| r.0 == c) { continue; }
                let prefix = format!("{seq}{c}");
                if it.next().is_some() {
                    let name = GROUPS.iter().find(|(g, _)| *g == prefix).map(|g| g.1).unwrap_or("+…");
                    rows.push((c, name.to_string(), true));
                } else {
                    rows.push((c, d.to_string(), false));
                }
            }
        }
        rows.sort_by_key(|r| wk_order(r.0));
        let title = if seq.is_empty() { "␣".to_string() } else {
            let g = GROUPS.iter().find(|(g, _)| *g == seq).map(|g| g.1).unwrap_or("");
            format!("␣ » {}{}", seq.replace(' ', "␣"), if g.is_empty() { String::new() } else { format!("  {g}") })
        };
        let font = FontId::monospace(13.5);
        let row_h = 20.0;
        let cols = if rows.len() > 22 { 2 } else { 1 };
        let per = rows.len().div_ceil(cols);
        let col_w = 290.0;
        let w = col_w * cols as f32 + 24.0;
        let h = per as f32 * row_h + 64.0;
        let screen = ctx.content_rect();
        let pos = egui::pos2(screen.max.x - w - 18.0, screen.max.y - h - 34.0);
        egui::Area::new(egui::Id::new("which_key")).order(egui::Order::Foreground).fixed_pos(pos).interactable(false).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
            let pt = ui.painter();
            pt.rect_filled(rect, 8.0, p.bg_dark);
            pt.rect_stroke(rect, 8.0, Stroke::new(1.0, p.blue), egui::StrokeKind::Inside);
            let tg = pt.layout_no_wrap(format!(" {title} "), font.clone(), p.blue);
            let tw = tg.size().x;
            pt.rect_filled(Rect::from_min_size(egui::pos2(rect.min.x + 16.0, rect.min.y - 8.0), Vec2::new(tw, 16.0)), 0.0, p.bg_dark);
            pt.galley(egui::pos2(rect.min.x + 16.0, rect.min.y - 9.0), tg, p.blue);
            for (k, (c, d, group)) in rows.iter().enumerate() {
                let col = k / per;
                let row = k % per;
                let x = rect.min.x + 14.0 + col as f32 * col_w;
                let y = rect.min.y + 18.0 + row as f32 * row_h;
                let key = if *c == ' ' { "␣".to_string() } else { c.to_string() };
                pt.text(egui::pos2(x, y), egui::Align2::LEFT_TOP, key, font.clone(), p.cyan);
                pt.text(egui::pos2(x + 22.0, y), egui::Align2::LEFT_TOP, "→", font.clone(), p.comment);
                pt.text(egui::pos2(x + 46.0, y), egui::Align2::LEFT_TOP, d, font.clone(), if *group { p.blue } else { p.magenta });
            }
            let fy = rect.max.y - 22.0;
            pt.text(egui::pos2(rect.min.x + 14.0, fy), egui::Align2::LEFT_TOP, "esc", FontId::monospace(11.0), p.cyan);
            pt.text(egui::pos2(rect.min.x + 44.0, fy), egui::Align2::LEFT_TOP, "fermer", FontId::monospace(12.0), p.comment);
            pt.text(egui::pos2(rect.min.x + 110.0, fy), egui::Align2::LEFT_TOP, "⌫", FontId::monospace(12.0), p.cyan);
            pt.text(egui::pos2(rect.min.x + 128.0, fy), egui::Align2::LEFT_TOP, "retour", FontId::monospace(12.0), p.comment);
        });
    }

    pub(super) fn leader_run(&mut self, seq: &str, ctx: &egui::Context) {
        use crate::settings::Action as A;
        let Some((_, _, act)) = KEYMAP.iter().find(|(k, _, _)| *k == seq).copied() else { return };
        let focus_editor = |app: &mut App| { if let Some(d) = app.cur_doc() { d.want_focus = true; } };
        match act {
            L::FindFiles => self.open_picker(PickKind::Files, ""),
            L::Grep => self.open_picker(PickKind::Grep, ""),
            L::GrepWord => {
                let w = self.cur_doc().and_then(|d| d.selected_text(ctx).or_else(|| d.word_at_cursor())).unwrap_or_default();
                self.open_picker(PickKind::Grep, &w);
            }
            L::Buffers => self.open_picker(PickKind::Buffers, ""),
            L::Recent => self.open_picker(PickKind::Recent, ""),
            L::Themes => self.open_picker(PickKind::Themes, ""),
            L::Keymaps => self.open_picker(PickKind::Keymaps, ""),
            L::HarpoonMenu => self.open_picker(PickKind::Harpoon, ""),
            L::Harpoon(n) => {
                match self.harpoon_list().get(n - 1).cloned() {
                    Some(p) => self.open_file(&p),
                    None => self.say(format!("Harpoon : pas de fichier n°{n} (␣H ajoute le fichier courant)"), self.p.yellow),
                }
            }
            L::HarpoonAdd => {
                let path = self.cur().and_then(|i| self.tabs[i].path().map(|p| p.to_path_buf()));
                match path {
                    Some(p) => {
                        let key = self.root.to_string_lossy().to_string();
                        let list = self.harpoon.entry(key).or_default();
                        if !list.contains(&p) { list.push(p.clone()); }
                        let n = list.iter().position(|x| *x == p).unwrap_or(0) + 1;
                        self.say(format!("Harpoon : {} → ␣{n}", p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()), self.p.green);
                        self.save_session();
                    }
                    None => self.say("Harpoon : enregistre d'abord le fichier", self.p.yellow),
                }
            }
            L::NewFile => self.run_action(A::New, ctx),
            L::Scratch => self.new_doc(),
            L::Save => self.run_action(A::Save, ctx),
            L::Config => { self.cfg.save(); if let Some(f) = crate::settings::Settings::path() { self.open_file(&f); } }
            L::Explorer => { self.show_explorer = !(self.show_explorer && self.side == SideTab::Files); self.side = SideTab::Files; }
            L::SplitBelow | L::SplitRight => self.split_current(act == L::SplitBelow),
            L::CloseWindow => self.close_window(),
            L::OtherWindow => self.window_nav('l'),
            L::Maximize => {
                match self.zen.take() {
                    Some((e, b)) => { self.show_explorer = e; self.show_bottom = b; }
                    None => { self.zen = Some((self.show_explorer, self.show_bottom)); self.show_explorer = false; self.show_bottom = false; }
                }
            }
            L::AltBuffer => {
                if let Some(uid) = self.prev_tab {
                    if let Some(i) = self.tabs.iter().position(|t| t.uid == uid) { self.activate(i); }
                }
            }
            L::BufDelete => self.run_action(A::CloseTab, ctx),
            L::BufOthers => { if let Some(i) = self.cur() { let u = self.tabs[i].uid; self.pane_ops.push(PaneOp::CloseOthers(u)); } }
            L::QuitAll => self.run_action(A::Quit, ctx),
            L::Terminal => { self.show_bottom = true; self.bottom = Bottom::Terminal(0); self.focus_terminal = true; }
            L::LazyGit => {
                if crate::runner::which("lazygit").is_some() {
                    self.new_local_terminal(Some(self.root.clone()));
                    let n = self.terms.len() - 1;
                    self.terms[n].write(b"lazygit\r");
                    self.show_bottom = true;
                    self.bottom = Bottom::Terminal(n);
                    self.focus_terminal = true;
                } else {
                    self.say("lazygit n'est pas installé (sudo apt install lazygit, ou scoop install lazygit sous Windows)", self.p.yellow);
                }
            }
            L::Wrap => { self.cfg.word_wrap = !self.cfg.word_wrap; self.cfg_dirty = true; }
            L::Numbers => { self.cfg.line_numbers = !self.cfg.line_numbers; self.cfg_dirty = true; }
            L::Relative => { self.cfg.relative_numbers = !self.cfg.relative_numbers; self.cfg_dirty = true; }
            L::Guides => { self.cfg.indent_guides = !self.cfg.indent_guides; self.cfg_dirty = true; }
            L::VimToggle => self.run_action(A::ToggleVim, ctx),
            L::Background => {
                let light = self.p.light;
                self.set_theme(if light { "Tokyo Night" } else { crate::theme::DEFAULT_THEME }, ctx);
            }
            L::SearchBuffer => self.run_action(A::Find, ctx),
            L::Replace => self.run_action(A::Replace, ctx),
            L::Settings => self.settings_open = true,
            L::Libs => self.run_action(A::Libs, ctx),
            L::Ssh => self.run_action(A::Ssh, ctx),
            L::Vpn => self.run_action(A::Vpn, ctx),
            L::Run => self.run_action(A::Run, ctx),
            L::DbgBreak => self.run_action(A::Breakpoint, ctx),
            L::DbgContinue => self.run_action(A::Debug, ctx),
            L::DbgInto => self.run_action(A::StepInto, ctx),
            L::DbgOver => self.run_action(A::StepOver, ctx),
            L::DbgOut => self.run_action(A::StepOut, ctx),
            L::DbgPause => self.dbg_cmd("pause"),
            L::DbgStop => self.run_action(A::Stop, ctx),
            L::DbgRepl => { self.show_bottom = true; self.bottom = Bottom::Repl; }
            L::DbgEval => ctx.memory_mut(|m| m.request_focus(egui::Id::new("dbg_eval"))),
        }
        if !matches!(act, L::Terminal | L::LazyGit | L::FindFiles | L::Grep | L::GrepWord | L::Buffers | L::Recent | L::Themes | L::Keymaps | L::HarpoonMenu | L::DbgEval) {
            focus_editor(self);
        }
    }

    /// Volet suivant / précédent (Ctrl+h/j/k/l, ␣ww). Ctrl+j sans volet en dessous : le terminal.
    pub(super) fn window_nav(&mut self, dir: char) {
        let panes = self.panes();
        if panes.len() <= 1 {
            if dir == 'j' { self.show_bottom = true; self.focus_terminal = true; }
            if dir == 'h' { self.show_explorer = true; }
            return;
        }
        let k = panes.iter().position(|p| *p == self.focus_pane).unwrap_or(0);
        let n = panes.len();
        let next = if dir == 'h' || dir == 'k' { panes[(k + n - 1) % n] } else { panes[(k + 1) % n] };
        self.focus_pane = next;
        if let Some(uid) = self.pane_active.get(&next).copied() {
            if let Some(t) = self.tabs.iter_mut().find(|t| t.uid == uid) { if let Some(d) = t.doc_mut() { d.want_focus = true; } }
        }
    }

    /// Scinder (:sp, :vs, ␣-, ␣|) : l'onglet courant part dans un nouveau volet. Un fichier n'est
    /// que dans un volet : avec un seul onglet, il n'y a rien à mettre de l'autre côté.
    fn split_current(&mut self, below: bool) {
        let Some(i) = self.cur() else { return };
        let (u, pa) = (self.tabs[i].uid, self.tabs[i].pane);
        if self.tabs.iter().filter(|t| t.pane == pa).count() < 2 {
            self.vim_msg("Un seul onglet dans ce volet : ouvre un autre fichier (␣␣) pour scinder");
            return;
        }
        self.split_tab(u, pa, below);
    }

    /// ␣wd : ferme le volet courant (ses onglets vont dans un autre volet).
    fn close_window(&mut self) {
        let panes = self.panes();
        if panes.len() <= 1 { return; }
        let cur = self.focus_pane;
        let Some(other) = panes.iter().copied().find(|p| *p != cur) else { return };
        let uids: Vec<u64> = self.tabs.iter().filter(|t| t.pane == cur).map(|t| t.uid).collect();
        for u in uids { self.move_tab(u, other); }
        self.focus_pane = other;
    }

    // ------------------------------------------------------------------ ce que Vim demande
    pub(super) fn vim_events(&mut self, outs: Vec<Out>, rest: Vec<K>, ctx: &egui::Context) {
        use crate::settings::Action as A;
        for o in outs {
            match o {
                Out::Leader => {
                    self.leader = Some((String::new(), Instant::now()));
                    self.leader_pending = rest.iter().filter_map(|k| if let K::Ch(c) = k { Some(*c) } else { None }).collect();
                    ctx.request_repaint();
                }
                Out::Buffer(d) => self.run_action(if d > 0 { A::NextTab } else { A::PrevTab }, ctx),
                Out::Window(c) => self.window_nav(c),
                Out::Terminal => {
                    // comme LazyVim : ouvre le terminal et lui donne le clavier (Ctrl+/ dans le terminal le masque)
                    self.show_bottom = true;
                    if !matches!(self.bottom, Bottom::Terminal(_) | Bottom::Ssh(..) | Bottom::Repl) { self.bottom = Bottom::Terminal(0); }
                    self.focus_terminal = true;
                }
                Out::Cmd(c) => self.ex_command(&c, ctx),
                _ => {}
            }
        }
    }

    fn vim_msg(&mut self, m: impl Into<String>) {
        let m = m.into();
        if let Some(d) = self.cur_doc() { d.vim.msg = Some(m); }
    }

    /// Commandes « : » de Vim qui concernent l'application.
    fn ex_command(&mut self, cmd: &str, ctx: &egui::Context) {
        use crate::settings::Action as A;
        let (name, arg) = match cmd.split_once(' ') { Some((a, b)) => (a, b.trim()), None => (cmd, "") };
        let Some(i) = self.cur() else { return };
        let dirty = self.tabs[i].dirty();
        match name {
            "w" | "write" => {
                if !arg.is_empty() {
                    let p = self.resolve(arg);
                    if let Some(d) = self.tabs[i].doc_mut() { d.remote = None; d.untitled = 0; let r = d.save_as(p); self.report_save(r); }
                } else if self.tabs[i].doc().map(|d| d.path.is_none() && d.remote.is_none()).unwrap_or(false) {
                    self.run_action(A::SaveAs, ctx);
                } else { self.save_tab(i); }
            }
            "q" | "quit" | "close" | "bd" | "bdelete" => {
                if dirty { self.vim_msg("E37: modifications non enregistrées (ajoute ! pour forcer)"); } else { self.close_tab(i, true); }
            }
            "q!" | "quit!" | "bd!" => self.close_tab(i, true),
            "wq" | "x" | "xit" | "exit" => { if self.save_tab(i) { if let Some(k) = self.cur() { self.close_tab(k, true); } } }
            "wa" | "wall" => self.run_action(A::SaveAll, ctx),
            "qa" | "qall" | "qa!" | "qall!" | "quitall" => {
                if name.ends_with('!') { self.allow_quit = true; }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            "wqa" | "xa" | "wqall" | "xall" => { self.run_action(A::SaveAll, ctx); ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
            "e" | "edit" => {
                if arg.is_empty() {
                    if let Some(d) = self.tabs[i].doc_mut() { d.reload_if_changed(); }
                } else { let p = self.resolve(arg); self.open_file(&p); }
            }
            "e!" | "edit!" => {
                if let Some(d) = self.tabs[i].doc_mut() {
                    if let Some(p) = d.path.clone() { if let Ok(t) = std::fs::read_to_string(&p) { d.replace_all(&t, ctx); } }
                }
            }
            "enew" | "new" => self.new_doc(),
            "sp" | "split" => self.split_current(true),
            "vs" | "vsp" | "vsplit" => self.split_current(false),
            "bn" | "bnext" => self.run_action(A::NextTab, ctx),
            "bp" | "bprevious" | "bprev" => self.run_action(A::PrevTab, ctx),
            "term" | "terminal" => { self.show_bottom = true; self.bottom = Bottom::Terminal(0); self.focus_terminal = true; }
            "set" | "se" => {
                for opt in arg.split_whitespace() {
                    match opt {
                        "nu" | "number" => self.cfg.line_numbers = true,
                        "nonu" | "nonumber" => self.cfg.line_numbers = false,
                        "rnu" | "relativenumber" => self.cfg.relative_numbers = true,
                        "nornu" | "norelativenumber" => self.cfg.relative_numbers = false,
                        "wrap" => self.cfg.word_wrap = true,
                        "nowrap" => self.cfg.word_wrap = false,
                        "list" | "nolist" => {}
                        o => { self.vim_msg(format!("E518: option inconnue : {o}")); return; }
                    }
                }
                self.cfg_dirty = true;
            }
            "run" | "make" => self.run_action(A::Run, ctx),
            "colorscheme" | "colo" => {
                if let Some((n, _)) = crate::theme::THEMES.iter().find(|(n, _)| n.to_lowercase().starts_with(&arg.to_lowercase())) { self.set_theme(n, ctx); }
                else { self.open_picker(PickKind::Themes, arg); }
            }
            "h" | "help" => self.open_picker(PickKind::Keymaps, arg),
            _ => self.vim_msg(format!("E492: pas une commande de l'éditeur : {cmd}")),
        }
    }

    /// Chemin tapé dans « :e » / « :w » : relatif au dossier du fichier, sinon au dossier ouvert.
    fn resolve(&self, arg: &str) -> PathBuf {
        let p = PathBuf::from(arg.replace('~', &std::env::var("HOME").unwrap_or_default()));
        if p.is_absolute() { return p; }
        let base = self.cur().and_then(|i| self.tabs[i].path().and_then(|f| f.parent().map(|d| d.to_path_buf()))).unwrap_or(self.root.clone());
        if base.join(&p).exists() { base.join(p) } else { self.root.join(p) }
    }
}
