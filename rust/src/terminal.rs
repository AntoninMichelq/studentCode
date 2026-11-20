//! Terminal intégré : le shell de l'utilisateur (bash, zsh… ou PowerShell sous Windows) dans un
//! pseudo-terminal, affiché cellule par cellule avec les couleurs, la sélection et l'historique.
use crate::theme::Palette;
use eframe::egui::{self, text::LayoutJob, Color32, FontId, Key, Rect, Sense, TextFormat, Vec2};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub enum Kind {
    Local,
    Ssh(crate::ssh::Lab, String),
    /// interpréteur Python interactif (onglet « Interpréteur Python »)
    Repl,
}

enum SshMsg {
    Data(Vec<u8>),
    Resize(u16, u16),
    Close,
}

pub struct Terminal {
    parser: Arc<Mutex<vt100::Parser>>,
    /// partagé avec le fil de lecture, qui répond lui-même aux questions du terminal
    writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
    master: Option<Box<dyn MasterPty + Send>>,
    ssh: Option<std::sync::mpsc::Sender<SshMsg>>,
    alive: Arc<Mutex<bool>>,
    size: (u16, u16),
    sel: Option<((u16, u16), (u16, u16))>,
    pending_cd: Option<String>,
    pub kind: Kind,
    started: bool,
    /// le terminal a le clavier : les raccourcis de l'app (Ctrl+W, Ctrl+B…) lui sont laissés
    pub focused: bool,
    pub error: Arc<Mutex<Option<String>>>,
}

/// Réponses aux questions qu'un programme pose au terminal : position du curseur (ESC[6n, que
/// ConPTY envoie au démarrage sous Windows et dont il attend la réponse), état (ESC[5n) et type de
/// terminal (ESC[c). `tail` garde la fin du morceau précédent, une question pouvant être coupée en deux.
pub fn answer_queries(data: &[u8], tail: &mut Vec<u8>, cursor: (u16, u16)) -> Vec<u8> {
    let mut all = std::mem::take(tail);
    let skip = all.len();
    all.extend_from_slice(data);
    let mut out = Vec::new();
    let mut i = 0;
    while i < all.len() {
        if all[i] == 0x1b && all.get(i + 1) == Some(&b'[') {
            let rest = &all[i + 2..];
            let end = i + 2;
            let (q, len): (&[u8], usize) = if rest.starts_with(b"6n") { (b"6n", 2) }
                else if rest.starts_with(b"5n") { (b"5n", 2) }
                else if rest.starts_with(b"c") { (b"c", 1) }
                else if rest.starts_with(b"0c") { (b"c", 2) }
                else { (b"", 0) };
            // la question doit finir dans ce morceau-ci (sinon elle a déjà eu sa réponse)
            if len > 0 && end + len > skip {
                match q {
                    b"6n" => out.extend_from_slice(format!("\x1b[{};{}R", cursor.0 + 1, cursor.1 + 1).as_bytes()),
                    b"5n" => out.extend_from_slice(b"\x1b[0n"),
                    _ => out.extend_from_slice(b"\x1b[?1;2c"),
                }
                i = end + len;
                continue;
            }
        }
        i += 1;
    }
    // début possible d'une question coupée : on le garde pour le morceau suivant
    let keep = all.len().saturating_sub(3);
    if let Some(p) = all[keep..].iter().rposition(|&b| b == 0x1b) { *tail = all[keep + p..].to_vec(); }
    out
}

/// Autotest : un vrai shell dans un pseudo-terminal répond-il ? (`--selftest`)
pub fn probe() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut t = Terminal::new();
    t.start(&std::env::temp_dir(), &ctx)?;
    std::thread::sleep(std::time::Duration::from_millis(800));
    t.write(b"echo SC_TERM_OK\r");
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let txt = t.parser.lock().unwrap().screen().contents();
        if txt.lines().any(|l| l.trim() == "SC_TERM_OK") { t.kill(); return Ok(()); }
    }
    let txt = t.parser.lock().unwrap().screen().contents();
    t.kill();
    Err(format!("le shell ne répond pas (écran : {:?})", txt.chars().take(200).collect::<String>()))
}

fn nz(v: u16) -> NonZeroU16 {
    NonZeroU16::new(v.max(1)).unwrap()
}

impl Terminal {
    pub fn new() -> Terminal {
        Terminal {
            parser: Arc::new(Mutex::new(vt100::Parser::new(nz(24), nz(80), crate::settings::globals().scrollback))),
            writer: None,
            master: None,
            ssh: None,
            alive: Arc::new(Mutex::new(false)),
            size: (24, 80),
            sel: None,
            pending_cd: None,
            kind: Kind::Local,
            started: false,
            focused: false,
            error: Arc::new(Mutex::new(None)),
        }
    }

    /// Terminal sur une machine distante ; `dir` : dossier où il s'ouvre (sinon le dossier personnel).
    /// Interpréteur Python interactif.
    pub fn new_repl() -> Terminal {
        let mut t = Terminal::new();
        t.kind = Kind::Repl;
        t
    }

    pub fn new_ssh(lab: crate::ssh::Lab, password: String, dir: Option<String>) -> Terminal {
        let mut t = Terminal::new();
        t.kind = Kind::Ssh(lab, password);
        t.pending_cd = dir;
        t
    }

    /// Shell distant : une connexion SSH dédiée, lue sans bloquer dans un fil séparé.
    fn start_ssh(&mut self, ctx: &egui::Context) {
        let Kind::Ssh(lab, pw) = &self.kind else { return };
        let (lab, pw) = (lab.clone(), pw.clone());
        let (tx, rx) = std::sync::mpsc::channel::<SshMsg>();
        self.ssh = Some(tx);
        *self.parser.lock().unwrap() = vt100::Parser::new(nz(self.size.0), nz(self.size.1), crate::settings::globals().scrollback);
        self.parser.lock().unwrap().process(format!("Connexion à {}…\r\n", lab.label()).as_bytes());
        *self.alive.lock().unwrap() = true;
        let (parser, alive, err, ctx) = (self.parser.clone(), self.alive.clone(), self.error.clone(), ctx.clone());
        let size = self.size;
        let start_dir = self.pending_cd.take();
        std::thread::spawn(move || {
            let fail = |m: String| {
                parser.lock().unwrap().process(format!("\r\n\x1b[31m{m}\x1b[0m\r\n").as_bytes());
                *err.lock().unwrap() = Some(m);
                *alive.lock().unwrap() = false;
                ctx.request_repaint();
            };
            let s = match crate::ssh::connect(&lab, &pw) {
                Ok(s) => s,
                Err(e) => return fail(crate::ssh::friendly(&e, &lab)),
            };
            let mut ch = match s.channel_session() { Ok(c) => c, Err(e) => return fail(e.to_string()) };
            if let Err(e) = ch.request_pty("xterm-256color", None, Some((size.1 as u32, size.0 as u32, 0, 0))) {
                return fail(e.to_string());
            }
            if let Err(e) = ch.shell() { return fail(e.to_string()); }
            if let Some(d) = start_dir {
                // espace devant : la commande n'encombre pas l'historique ; clear : invite propre
                let _ = ch.write_all(format!(" cd '{}' && clear\n", d.replace('\'', "'\\''")).as_bytes());
            }
            s.set_blocking(false);
            let mut buf = [0u8; 16384];
            loop {
                let mut idle = true;
                match ch.read(&mut buf) {
                    Ok(n) if n > 0 => { idle = false; parser.lock().unwrap().process(&buf[..n]); ctx.request_repaint(); }
                    _ => {}
                }
                while let Ok(m) = rx.try_recv() {
                    idle = false;
                    match m {
                        SshMsg::Data(d) => {
                            let mut off = 0;
                            let mut tries = 0;
                            while off < d.len() && tries < 400 {
                                match ch.write(&d[off..]) { Ok(n) => off += n, Err(_) => { tries += 1; std::thread::sleep(std::time::Duration::from_millis(5)); } }
                            }
                        }
                        SshMsg::Resize(r, c) => { let _ = ch.request_pty_size(c as u32, r as u32, None, None); }
                        SshMsg::Close => { let _ = ch.close(); *alive.lock().unwrap() = false; ctx.request_repaint(); return; }
                    }
                }
                if ch.eof() { break; }
                if idle { std::thread::sleep(std::time::Duration::from_millis(12)); }
            }
            parser.lock().unwrap().process(b"\r\n[session SSH terminee]\r\n");
            *alive.lock().unwrap() = false;
            ctx.request_repaint();
        });
    }

    pub fn running(&self) -> bool {
        *self.alive.lock().unwrap()
    }

    pub fn start(&mut self, cwd: &Path, ctx: &egui::Context) -> Result<(), String> {
        let pty = native_pty_system();
        let pair = pty.openpty(PtySize { rows: self.size.0, cols: self.size.1, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
        let py = if matches!(self.kind, Kind::Repl) { crate::runner::python() } else { None };
        let mut cmd = if let Some(py) = py {
            let mut c = CommandBuilder::new(&py[0]);
            for a in &py[1..] { c.arg(a); }
            c.arg("-q");
            c.arg("-i");
            c.env("PYTHONIOENCODING", "utf-8");
            c
        } else if let Some(sh) = Some(crate::settings::globals().shell).filter(|s| !s.is_empty()) {
            // shell choisi dans les Paramètres (avec ses arguments éventuels)
            let mut it = sh.split_whitespace();
            let mut c = CommandBuilder::new(it.next().unwrap_or("sh"));
            for a in it { c.arg(a); }
            c
        } else if cfg!(windows) {
            let ps = ["pwsh.exe", "powershell.exe"].into_iter()
                .find(|p| crate::runner::which(p.trim_end_matches(".exe")).is_some()).unwrap_or("cmd.exe");
            let mut c = CommandBuilder::new(ps);
            if ps != "cmd.exe" {
                c.arg("-NoLogo");
            }
            c
        } else {
            let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
            let mut c = CommandBuilder::new(sh);
            c.arg("-i");
            c
        };
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(|e| e.to_string())?));
        self.writer = Some(writer.clone());
        self.master = Some(pair.master);
        *self.parser.lock().unwrap() = vt100::Parser::new(nz(self.size.0), nz(self.size.1), crate::settings::globals().scrollback);
        *self.alive.lock().unwrap() = true;
        let parser = self.parser.clone();
        let alive = self.alive.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut child = child;
            let mut buf = [0u8; 16384];
            let mut tail = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let cursor = {
                            let mut p = parser.lock().unwrap();
                            p.process(&buf[..n]);
                            p.screen().cursor_position()
                        };
                        // Windows (ConPTY) demande où est le curseur au démarrage et n'affiche rien
                        // tant qu'on ne répond pas : sans ça, PowerShell reste sur un écran vide
                        let reply = answer_queries(&buf[..n], &mut tail, cursor);
                        if !reply.is_empty() {
                            if let Ok(mut w) = writer.lock() { let _ = w.write_all(&reply); let _ = w.flush(); }
                        }
                        ctx.request_repaint();
                    }
                }
            }
            let _ = child.wait();
            *alive.lock().unwrap() = false;
            ctx.request_repaint();
        });
        Ok(())
    }

    pub fn write(&mut self, data: &[u8]) {
        if let Some(t) = self.ssh.as_ref() {
            let _ = t.send(SshMsg::Data(data.to_vec()));
            return;
        }
        if let Some(w) = self.writer.as_ref() {
            if let Ok(mut w) = w.lock() {
                let _ = w.write_all(data);
                let _ = w.flush();
            }
        }
    }

    /// Place le shell dans `dir` (à l'ouverture d'un dossier) ; la ligne en cours est effacée.
    pub fn cd(&mut self, dir: &Path) {
        let d = dir.to_string_lossy().to_string();
        let line = if cfg!(windows) {
            format!("\x1bSet-Location -LiteralPath '{}'\r", d.replace('\'', "''"))
        } else {
            format!("\x15 cd '{}'\r", d.replace('\'', "'\\''"))
        };
        if self.running() {
            self.write(line.as_bytes());
        } else {
            self.pending_cd = Some(d);
        }
    }

    pub fn kill(&mut self) {
        if let Some(t) = self.ssh.take() {
            let _ = t.send(SshMsg::Close);
        }
        self.writer = None;
        self.master = None;
        self.started = false;
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        if (rows, cols) == self.size || rows < 2 || cols < 10 {
            return;
        }
        self.size = (rows, cols);
        self.parser.lock().unwrap().screen_mut().set_size(nz(rows), nz(cols));
        if let Some(m) = self.master.as_ref() {
            let _ = m.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        }
        if let Some(t) = self.ssh.as_ref() {
            let _ = t.send(SshMsg::Resize(rows, cols));
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, p: &Palette, font: &FontId, cwd: &Path) {
        if !self.started {
            self.started = true;
            match self.kind {
                Kind::Local | Kind::Repl => {
                    let dir = self.pending_cd.take().map(std::path::PathBuf::from).unwrap_or_else(|| cwd.to_path_buf());
                    if let Err(e) = self.start(&dir, ui.ctx()) {
                        self.parser.lock().unwrap().process(format!("Impossible de lancer le shell : {e}\r\n").as_bytes());
                    }
                }
                Kind::Ssh(..) => self.start_ssh(ui.ctx()),
            }
        }
        let (cw, ch) = ui.fonts_mut(|f| (f.glyph_width(font, 'M'), f.row_height(font)));
        let avail = ui.available_size();
        let cols = ((avail.x - 8.0) / cw).floor().max(10.0) as u16;
        let rows = (avail.y / ch).floor().max(2.0) as u16;
        self.resize(rows, cols);
        let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        let id = resp.id;
        if resp.clicked() || resp.drag_started() {
            resp.request_focus();
        }
        let focused = resp.has_focus();
        self.focused = focused;
        if focused {
            ui.memory_mut(|m| m.set_focus_lock_filter(id, egui::EventFilter {
                tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true }));
        }
        let origin = rect.min + Vec2::new(4.0, 0.0);
        let cell_at = |pos: egui::Pos2| -> (u16, u16) {
            let c = ((pos.x - origin.x) / cw).floor().clamp(0.0, (cols - 1) as f32) as u16;
            let r = ((pos.y - origin.y) / ch).floor().clamp(0.0, (rows - 1) as f32) as u16;
            (r, c)
        };
        // sélection à la souris
        if resp.drag_started() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let c = cell_at(pos);
                self.sel = Some((c, c));
            }
        } else if resp.dragged() {
            if let (Some(pos), Some((a, _))) = (resp.interact_pointer_pos(), self.sel) {
                self.sel = Some((a, cell_at(pos)));
            }
        } else if resp.clicked() {
            self.sel = None;
        }
        // défilement de l'historique
        if resp.hovered() {
            let dy = ui.input(|i| i.smooth_scroll_delta.y);
            if dy.abs() > 0.5 {
                let mut prs = self.parser.lock().unwrap();
                let cur = prs.screen().scrollback() as i64;
                let lines = (dy / ch).round() as i64;
                let n = (cur + lines).max(0) as usize;
                prs.screen_mut().set_scrollback(n);
            }
        }
        if focused {
            if !self.running() && self.writer.is_none() && self.ssh.is_none() {
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    self.started = false;
                }
            } else {
                self.handle_input(ui);
            }
        }
        // dessin
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.bg);
        let prs = self.parser.lock().unwrap();
        let screen = prs.screen();
        let sel = self.sel.map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
        for r in 0..rows {
            let mut job = LayoutJob::default();
            let y = origin.y + r as f32 * ch;
            for c in 0..cols {
                let Some(cell) = screen.cell(r, c) else { continue };
                if cell.is_wide_continuation() {
                    continue;
                }
                let mut fg = conv(cell.fgcolor(), p, true);
                let mut bg = conv(cell.bgcolor(), p, false);
                if cell.inverse() {
                    std::mem::swap(&mut fg, &mut bg);
                    if bg == Color32::TRANSPARENT { bg = p.fg; }
                    if fg == Color32::TRANSPARENT { fg = p.bg; }
                }
                if cell.bold() && fg == p.fg {
                    fg = Color32::WHITE.lerp_to_gamma(p.fg, 0.3);
                }
                if let Some(((r0, c0), (r1, c1))) = sel {
                    let inside = (r > r0 || (r == r0 && c >= c0)) && (r < r1 || (r == r1 && c <= c1));
                    if inside { bg = p.sel; }
                }
                if bg != Color32::TRANSPARENT {
                    let x = origin.x + c as f32 * cw;
                    let w = if cell.is_wide() { cw * 2.0 } else { cw };
                    painter.rect_filled(Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, ch)), 0.0, bg);
                }
                let s = cell.contents();
                let s = if s.is_empty() { " " } else { s };
                let mut f = TextFormat::simple(font.clone(), if fg == Color32::TRANSPARENT { p.fg } else { fg });
                f.italics = cell.italic();
                if cell.underline() { f.underline = egui::Stroke::new(1.0, f.color); }
                job.append(s, 0.0, f);
            }
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            painter.galley(egui::pos2(origin.x, y), galley, p.fg);
        }
        // curseur
        if !screen.hide_cursor() && screen.scrollback() == 0 {
            let (cr, cc) = screen.cursor_position();
            let r = Rect::from_min_size(egui::pos2(origin.x + cc as f32 * cw, origin.y + cr as f32 * ch), Vec2::new(cw, ch));
            if focused {
                painter.rect_filled(r, 1.0, p.fg.gamma_multiply(0.75));
            } else {
                painter.rect_stroke(r, 1.0, egui::Stroke::new(1.0, p.comment), egui::StrokeKind::Inside);
            }
        }
        drop(prs);
        // menu clic droit
        resp.context_menu(|ui| {
            if ui.button("Copier").clicked() {
                if let Some(t) = self.selection_text() { ui.ctx().copy_text(t); }
                ui.close();
            }
            if ui.button("Coller").clicked() {
                if let Some(t) = crate::clip::get() { self.paste(&t); }
                ui.close();
            }
            if ui.button("Effacer l'écran").clicked() {
                self.write(if cfg!(windows) { b"cls\r" } else { b"\x0c" });
                ui.close();
            }
            if ui.button("Relancer").clicked() {
                self.kill();
                ui.close();
            }
        });
        if !self.running() && (self.writer.is_some() || self.ssh.is_some()) {
            self.writer = None;
            self.master = None;
            self.ssh = None;
            self.parser.lock().unwrap().process("\r\n[terminé — Entrée pour relancer]\r\n".as_bytes());
        }
    }

    /// Colle du texte (mode « collage encadré » si le programme le demande, comme bash).
    fn paste(&mut self, t: &str) {
        self.scroll_to_bottom();
        let bracketed = self.parser.lock().unwrap().screen().bracketed_paste();
        if bracketed {
            self.write(b"\x1b[200~");
            self.write(t.as_bytes());
            self.write(b"\x1b[201~");
        } else {
            self.write(t.replace("\r\n", "\r").replace('\n', "\r").as_bytes());
        }
    }

    fn selection_text(&self) -> Option<String> {
        let ((r0, c0), (r1, c1)) = self.sel.map(|(a, b)| if a <= b { (a, b) } else { (b, a) })?;
        let prs = self.parser.lock().unwrap();
        Some(prs.screen().contents_between(r0, c0, r1, c1 + 1))
    }

    fn handle_input(&mut self, ui: &mut egui::Ui) {
        let events = ui.input(|i| i.events.clone());
        let app_cursor = self.parser.lock().unwrap().screen().application_cursor();
        let bracketed = self.parser.lock().unwrap().screen().bracketed_paste();
        for ev in events {
            match ev {
                egui::Event::Text(t) => {
                    self.scroll_to_bottom();
                    self.write(t.as_bytes());
                }
                egui::Event::Paste(t) => {
                    let _ = bracketed;
                    self.paste(&t);
                }
                egui::Event::Copy => {
                    if let Some(t) = self.selection_text() {
                        ui.ctx().copy_text(t);
                    } else {
                        self.write(b"\x03"); // Ctrl+C sans sélection : interrompre
                    }
                }
                egui::Event::Cut => self.write(b"\x18"),
                // Ctrl+Maj+C / Ctrl+Maj+V : copier / coller, comme dans les terminaux Linux
                egui::Event::Key { key: Key::C, pressed: true, modifiers, .. } if modifiers.ctrl && modifiers.shift => {
                    if let Some(t) = self.selection_text() { ui.ctx().copy_text(t); }
                }
                egui::Event::Key { key: Key::V, pressed: true, modifiers, .. } if modifiers.ctrl && modifiers.shift => {
                    if let Some(t) = crate::clip::get() { self.paste(&t); }
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    let seq: Option<Vec<u8>> = if modifiers.ctrl && !modifiers.shift {
                        let name = key.name();
                        if name.len() == 1 && name.as_bytes()[0].is_ascii_alphabetic() {
                            Some(vec![name.as_bytes()[0].to_ascii_lowercase() - b'a' + 1])
                        } else {
                            None
                        }
                    } else {
                        let arrow = |c: u8| if app_cursor { vec![0x1b, b'O', c] } else { vec![0x1b, b'[', c] };
                        match key {
                            Key::Enter => Some(b"\r".to_vec()),
                            Key::Backspace => Some(vec![0x7f]),
                            Key::Tab => Some(if modifiers.shift { b"\x1b[Z".to_vec() } else { b"\t".to_vec() }),
                            Key::Escape => Some(vec![0x1b]),
                            Key::ArrowUp => Some(arrow(b'A')),
                            Key::ArrowDown => Some(arrow(b'B')),
                            Key::ArrowRight => Some(arrow(b'C')),
                            Key::ArrowLeft => Some(arrow(b'D')),
                            Key::Home => Some(b"\x1b[H".to_vec()),
                            Key::End => Some(b"\x1b[F".to_vec()),
                            Key::Delete => Some(b"\x1b[3~".to_vec()),
                            Key::PageUp => Some(b"\x1b[5~".to_vec()),
                            Key::PageDown => Some(b"\x1b[6~".to_vec()),
                            Key::Insert => Some(b"\x1b[2~".to_vec()),
                            _ => None,
                        }
                    };
                    if let Some(s) = seq {
                        self.scroll_to_bottom();
                        self.sel = None;
                        self.write(&s);
                    }
                }
                _ => {}
            }
        }
    }

    fn scroll_to_bottom(&mut self) {
        self.parser.lock().unwrap().screen_mut().set_scrollback(0);
    }
}

/// Couleur d'une cellule : les 16 couleurs ANSI suivent le thème.
fn conv(c: vt100::Color, p: &Palette, fg: bool) -> Color32 {
    match c {
        vt100::Color::Default => if fg { p.fg } else { Color32::TRANSPARENT },
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        vt100::Color::Idx(i) => match i {
            0 => p.bg_hl, 1 => p.red, 2 => p.green, 3 => p.yellow, 4 => p.blue, 5 => p.magenta, 6 => p.cyan,
            7 => p.fg, 8 => p.comment, 9 => p.red, 10 => p.green, 11 => p.yellow, 12 => p.blue,
            13 => p.magenta, 14 => p.cyan, 15 => Color32::WHITE,
            16..=231 => {
                let i = i - 16;
                let v = |x: u8| if x == 0 { 0 } else { 55 + x * 40 };
                Color32::from_rgb(v(i / 36), v((i / 6) % 6), v(i % 6))
            }
            _ => {
                let g = 8 + (i - 232) * 10;
                Color32::from_rgb(g, g, g)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_cursor_query() {
        let mut tail = Vec::new();
        assert_eq!(answer_queries(b"abc\x1b[6n", &mut tail, (0, 0)), b"\x1b[1;1R");
        // question coupée entre deux lectures
        assert!(answer_queries(b"x\x1b[", &mut tail, (2, 4)).is_empty());
        assert_eq!(answer_queries(b"6ny", &mut tail, (2, 4)), b"\x1b[3;5R");
        assert!(answer_queries(b"\x1b[31mrouge\x1b[0m", &mut tail, (0, 0)).is_empty());
        assert_eq!(answer_queries(b"\x1b[c", &mut tail, (0, 0)), b"\x1b[?1;2c");
    }

    #[test]
    fn local_shell_answers() {
        probe().unwrap();
    }
}
