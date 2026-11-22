//! Mode Vim de l'éditeur, réglé comme LazyVim : modes Normal / Insertion / Visuel / Visuel-ligne,
//! mouvements, opérateurs (d c y > < gc gu gU g~), objets de texte, compteurs, registre partagé
//! avec le presse-papiers du système (clipboard=unnamedplus), « . », recherche (/ ? n N * #) et
//! commandes « : ». Le moteur travaille sur le texte seul ; l'éditeur (editor.rs) lui passe les
//! touches et applique ce qu'il renvoie (annuler, copier, enregistrer…).
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
        }
    }
}

/// Une touche telle que Vim la voit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum K {
    Ch(char),
    Esc,
    Enter,
    Back,
    Tab,
    Del,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Ctrl(char),
    Alt(char),
}

/// Ce que l'éditeur ou l'application doit faire après une touche.
#[derive(Clone, PartialEq, Debug)]
pub enum Out {
    Undo(usize),
    Redo(usize),
    /// texte copié (registre « + » : presse-papiers du système)
    Copy(String),
    /// Espace en mode Normal : touche leader (les touches suivantes vont à l'application)
    Leader,
    /// commande « : » que l'application exécute (w, q, wq, e fichier, sp, vs, bn…)
    Cmd(String),
    /// changer d'onglet (H / L, [b ]b) : -1 ou +1
    Buffer(i32),
    /// Ctrl+h/j/k/l : passer au volet voisin
    Window(char),
    /// Ctrl+/ : afficher / masquer le terminal
    Terminal,
    /// surligner les occurrences de ce texte (None : plus de surlignage)
    Highlight(Option<String>),
    /// zz / zt / zb : placer la ligne du curseur au milieu / en haut / en bas
    Scroll(char),
}

/// Le texte et le curseur, le temps d'une touche.
pub struct Buf {
    pub t: Vec<char>,
    pub cur: usize,
    /// autre bout de la sélection en mode Visuel
    pub anchor: usize,
    pub changed: bool,
    /// lignes visibles (Ctrl+d / Ctrl+u)
    pub page: usize,
    /// contenu du presse-papiers du système (lu seulement si p / P est tapé)
    pub clip: Option<String>,
    /// ce qu'insère un niveau d'indentation
    pub indent: String,
    /// commentaires du langage : (ligne, début de bloc, fin de bloc)
    pub comment: (String, String, String),
}

impl Buf {
    pub fn new(text: &str, cur: usize) -> Buf {
        Buf { t: text.chars().collect(), cur, anchor: cur, changed: false, page: 30, clip: None,
              indent: "    ".into(), comment: ("#".into(), String::new(), String::new()) }
    }
    pub fn text(&self) -> String { self.t.iter().collect() }
    fn len(&self) -> usize { self.t.len() }
    fn line_start(&self, i: usize) -> usize {
        let i = i.min(self.len());
        self.t[..i].iter().rposition(|c| *c == '\n').map(|p| p + 1).unwrap_or(0)
    }
    /// position du '\n' de la ligne (ou fin du texte)
    fn line_end(&self, i: usize) -> usize {
        let i = i.min(self.len());
        self.t[i..].iter().position(|c| *c == '\n').map(|p| p + i).unwrap_or(self.len())
    }
    fn line_of(&self, i: usize) -> usize { self.t[..i.min(self.len())].iter().filter(|c| **c == '\n').count() }
    fn lines(&self) -> usize { self.t.iter().filter(|c| **c == '\n').count() + 1 }
    /// début de la ligne n (à partir de 0)
    fn line_pos(&self, n: usize) -> usize {
        if n == 0 { return 0; }
        let mut k = 0;
        for (i, c) in self.t.iter().enumerate() {
            if *c == '\n' { k += 1; if k == n { return i + 1; } }
        }
        self.line_start(self.len())
    }
    fn col(&self, i: usize) -> usize { i - self.line_start(i) }
    fn first_non_blank(&self, i: usize) -> usize {
        let (s, e) = (self.line_start(i), self.line_end(i));
        (s..e).find(|&k| !matches!(self.t[k], ' ' | '\t')).unwrap_or(e)
    }
    /// curseur du mode Normal : jamais sur le '\n' (sauf ligne vide)
    fn clamp_normal(&mut self) {
        self.cur = self.cur.min(self.len());
        let (s, e) = (self.line_start(self.cur), self.line_end(self.cur));
        if self.cur >= e && e > s { self.cur = e - 1; }
    }
    fn get(&self, s: usize, e: usize) -> String { self.t[s.min(self.len())..e.min(self.len())].iter().collect() }
    fn delete(&mut self, s: usize, e: usize) -> String {
        let (s, e) = (s.min(self.len()), e.min(self.len()));
        if s >= e { return String::new(); }
        self.changed = true;
        self.t.drain(s..e).collect()
    }
    fn insert(&mut self, at: usize, s: &str) {
        let at = at.min(self.len());
        let v: Vec<char> = s.chars().collect();
        if v.is_empty() { return; }
        self.changed = true;
        self.t.splice(at..at, v);
    }
    fn indent_of(&self, i: usize) -> String {
        let s = self.line_start(i);
        self.t[s..self.first_non_blank(i)].iter().collect()
    }
}

fn cls(c: char, big: bool) -> u8 {
    if c.is_whitespace() { 0 } else if big || c.is_alphanumeric() || c == '_' { 1 } else { 2 }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind { Excl, Incl, Line }

#[derive(Clone, PartialEq, Debug)]
enum M {
    Left, Right, Up, Down, Word(bool), Back(bool), End(bool), BackEnd(bool),
    LineStart, FirstNonBlank, LineEnd, Goto(Option<usize>), FileStart, Find(char, char), RepeatFind(bool),
    Match, ParaF, ParaB, Search(bool), Star(bool), HalfDown, HalfUp, Mark(char, bool), CurLine,
}

/// Ce que l'analyse des touches tapées a reconnu.
enum P {
    More,
    Bad,
    Motion(usize, M),
    Op(usize, String, Target),
    Cmd(usize, String, Option<char>),
}

enum Target { M(usize, M), Obj(bool, char), Line(usize) }

pub struct Vim {
    pub mode: Mode,
    /// touches du commande en cours (affichées en bas à droite, comme « showcmd »)
    pub pending: Vec<K>,
    /// ligne de commande ouverte : ":…", "/…" ou "?…"
    pub cmdline: Option<String>,
    /// message à afficher dans la ligne de commande
    pub msg: Option<String>,
    reg: (String, bool),
    want_col: Option<usize>,
    last_find: Option<(char, char)>,
    last_search: Option<(String, bool)>,
    rec: Vec<K>,
    last_change: Vec<K>,
    in_insert_rec: bool,
    replaying: bool,
    marks: HashMap<char, usize>,
    last_visual: Option<(usize, usize, Mode)>,
}

impl Default for Vim {
    fn default() -> Self { Vim::new() }
}

impl Vim {
    pub fn new() -> Vim {
        Vim { mode: Mode::Normal, pending: Vec::new(), cmdline: None, msg: None, reg: (String::new(), false), want_col: None,
              last_find: None, last_search: None, rec: Vec::new(), last_change: Vec::new(), in_insert_rec: false,
              replaying: false, marks: HashMap::new(), last_visual: None }
    }

    /// Touches en attente, pour l'affichage (« 2d », « ci »…).
    pub fn showcmd(&self) -> String {
        self.pending.iter().map(|k| match k { K::Ch(c) => c.to_string(), K::Ctrl(c) => format!("^{}", c.to_ascii_uppercase()), _ => String::new() }).collect()
    }

    /// Une touche tapée en mode Insertion (le texte est écrit par l'éditeur) : gardée pour « . ».
    pub fn record_insert(&mut self, k: K) {
        if self.in_insert_rec { self.rec.push(k); }
    }

    pub fn search_text(&self) -> Option<&str> { self.last_search.as_ref().map(|s| s.0.as_str()) }

    pub fn key(&mut self, b: &mut Buf, k: K) -> Vec<Out> {
        let mut out = Vec::new();
        if self.cmdline.is_some() {
            self.cmdline_key(b, k, &mut out);
            return out;
        }
        match self.mode {
            Mode::Insert => self.insert_key(b, k, &mut out),
            _ => {
                if self.pending.is_empty() && !self.replaying && !self.in_insert_rec { self.rec.clear(); }
                self.pending.push(k.clone());
                if !self.replaying && !self.in_insert_rec { self.rec.push(k); }
                let keys = self.pending.clone();
                match parse(&keys, self.mode != Mode::Normal) {
                    P::More => {}
                    P::Bad => { self.pending.clear(); }
                    p => {
                        self.pending.clear();
                        let before = b.changed;
                        b.changed = false;
                        self.run(b, p, &mut out);
                        let changed = b.changed;
                        b.changed |= before;
                        let is_dot = matches!(keys.last(), Some(K::Ch('.')));
                        if !self.replaying && !is_dot {
                            if self.mode == Mode::Insert && !self.in_insert_rec {
                                self.in_insert_rec = true;
                            } else if changed && self.mode == Mode::Normal && !self.in_insert_rec {
                                self.last_change = self.rec.clone();
                            }
                        }
                    }
                }
            }
        }
        if self.mode == Mode::Normal { b.clamp_normal(); b.anchor = b.cur; }
        out
    }

    fn insert_key(&mut self, b: &mut Buf, k: K, out: &mut Vec<Out>) {
        let _ = out;
        match k {
            K::Esc => {
                if self.in_insert_rec && !self.replaying {
                    self.rec.push(K::Esc);
                    self.last_change = std::mem::take(&mut self.rec);
                }
                self.in_insert_rec = false;
                self.mode = Mode::Normal;
                if b.cur > b.line_start(b.cur) { b.cur -= 1; }
            }
            // le reste n'arrive ici qu'en rejouant « . » (sinon l'éditeur écrit lui-même)
            K::Ch(c) => { b.insert(b.cur, &c.to_string()); b.cur += 1; }
            K::Enter => {
                let ind = b.indent_of(b.cur);
                let s = format!("\n{ind}");
                b.insert(b.cur, &s);
                b.cur += s.chars().count();
            }
            K::Tab => { let u = b.indent.clone(); b.insert(b.cur, &u); b.cur += u.chars().count(); }
            K::Back => { if b.cur > 0 { b.delete(b.cur - 1, b.cur); b.cur -= 1; } }
            K::Del => { b.delete(b.cur, b.cur + 1); }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ ligne de commande
    fn cmdline_key(&mut self, b: &mut Buf, k: K, out: &mut Vec<Out>) {
        let Some(line) = self.cmdline.as_mut() else { return };
        match k {
            K::Esc => { self.cmdline = None; }
            K::Back => { line.pop(); if line.is_empty() { self.cmdline = None; } }
            K::Ch(c) => line.push(c),
            K::Enter => {
                let line = self.cmdline.take().unwrap_or_default();
                if self.mode != Mode::Normal { self.leave_visual(b); }
                let (head, rest) = line.split_at(line.chars().next().map(|c| c.len_utf8()).unwrap_or(0));
                match head {
                    "/" | "?" => {
                        let fwd = head == "/";
                        let q = if rest.is_empty() { self.last_search.as_ref().map(|s| s.0.clone()).unwrap_or_default() } else { rest.to_string() };
                        if q.is_empty() { return; }
                        self.last_search = Some((q.clone(), fwd));
                        out.push(Out::Highlight(Some(q)));
                        if let Some(p) = search(b, b.cur, &self.last_search.clone().unwrap(), false) { b.cur = p; } else { self.not_found(); }
                    }
                    _ => self.ex(b, rest.trim(), out),
                }
            }
            _ => {}
        }
    }

    fn not_found(&mut self) {
        self.msg = Some(format!("E486: motif introuvable : {}", self.last_search.as_ref().map(|s| s.0.as_str()).unwrap_or("")));
    }

    /// Commandes « : » faites ici (numéro de ligne, :s, :noh…) ; les autres vont à l'application.
    fn ex(&mut self, b: &mut Buf, cmd: &str, out: &mut Vec<Out>) {
        if cmd.is_empty() { return; }
        if let Ok(n) = cmd.parse::<usize>() {
            b.cur = b.first_non_blank(b.line_pos(n.saturating_sub(1)));
            return;
        }
        if cmd == "$" { b.cur = b.first_non_blank(b.line_pos(b.lines() - 1)); return; }
        if cmd == "noh" || cmd == "nohlsearch" { out.push(Out::Highlight(None)); return; }
        // :s/a/b/g, :%s/a/b/g, :'<,'>s/a/b/g
        let (range, rest) = if let Some(r) = cmd.strip_prefix('%') { ("%", r) }
            else if let Some(r) = cmd.strip_prefix("'<,'>") { ("v", r) } else { ("", cmd) };
        if let Some(sub) = rest.strip_prefix("s").or_else(|| rest.strip_prefix("substitute")) {
            if let Some(sep) = sub.chars().next().filter(|c| !c.is_alphanumeric() && *c != ' ') {
                let parts: Vec<&str> = sub[sep.len_utf8()..].split(sep).collect();
                let pat = parts.first().copied().unwrap_or("");
                let rep = parts.get(1).copied().unwrap_or("");
                let flags = parts.get(2).copied().unwrap_or("");
                let pat = if pat.is_empty() { self.last_search.as_ref().map(|s| s.0.clone()).unwrap_or_default() } else { pat.to_string() };
                if pat.is_empty() { return; }
                let (l1, l2) = match range {
                    "%" => (0, b.lines() - 1),
                    "v" => match self.last_visual { Some((a, c, _)) => (b.line_of(a.min(c)), b.line_of(a.max(c))), None => (b.line_of(b.cur), b.line_of(b.cur)) },
                    _ => (b.line_of(b.cur), b.line_of(b.cur)),
                };
                let ci = flags.contains('i') || (!flags.contains('I') && pat.chars().all(|c| !c.is_uppercase()));
                let mut n = 0;
                for l in l1..=l2 {
                    let s = b.line_pos(l);
                    let e = b.line_end(s);
                    let line: String = b.get(s, e);
                    let (newl, k) = replace_in(&line, &pat, rep, flags.contains('g'), ci);
                    if k > 0 {
                        b.delete(s, e);
                        b.insert(s, &newl);
                        n += k;
                    }
                }
                self.last_search = Some((pat.clone(), true));
                if n == 0 { self.msg = Some(format!("E486: motif introuvable : {pat}")); }
                else { self.msg = Some(format!("{n} remplacement(s)")); b.cur = b.first_non_blank(b.line_pos(l1)); }
                return;
            }
        }
        out.push(Out::Cmd(cmd.to_string()));
    }

    // ------------------------------------------------------------------ exécution
    fn leave_visual(&mut self, b: &mut Buf) {
        self.last_visual = Some((b.anchor, b.cur, self.mode));
        self.mode = Mode::Normal;
    }

    /// bornes de la sélection visuelle : (début, fin exclue, par lignes)
    fn visual_range(&self, b: &Buf) -> (usize, usize, bool) {
        let (a, c) = (b.anchor.min(b.cur), b.anchor.max(b.cur));
        if self.mode == Mode::VisualLine {
            let e = b.line_end(c);
            (b.line_start(a), (e + 1).min(b.len()).max(e), true)
        } else {
            (a, (c + 1).min(b.len()), false)
        }
    }

    fn run(&mut self, b: &mut Buf, p: P, out: &mut Vec<Out>) {
        let visual = self.mode != Mode::Normal;
        match p {
            P::Motion(n, m) => {
                if let Some((to, _)) = self.motion(b, &m, n, None) {
                    if !matches!(m, M::Up | M::Down | M::HalfDown | M::HalfUp) { self.want_col = None; }
                    b.cur = to;
                    if !visual { b.clamp_normal(); }
                }
                if matches!(m, M::Search(_) | M::Star(_)) { if let Some(s) = &self.last_search { out.push(Out::Highlight(Some(s.0.clone()))); } }
            }
            P::Op(n, op, target) => {
                let range = if visual {
                    let (s, e, lw) = self.visual_range(b);
                    self.leave_visual(b);
                    Some((s, e, lw))
                } else {
                    self.target_range(b, &op, n, target)
                };
                if let Some((s, e, lw)) = range { self.operate(b, &op, s, e, lw, out); }
            }
            P::Cmd(n, c, arg) => self.command(b, n, &c, arg, out),
            _ => {}
        }
    }

    /// Zone visée par un opérateur : (début, fin exclue, par lignes).
    fn target_range(&mut self, b: &mut Buf, op: &str, n: usize, t: Target) -> Option<(usize, usize, bool)> {
        match t {
            Target::Line(n) => {
                let l = b.line_of(b.cur);
                let last = (l + n - 1).min(b.lines() - 1);
                let s = b.line_pos(l);
                let e = b.line_end(b.line_pos(last));
                Some((s, (e + 1).min(b.len()).max(e), true))
            }
            Target::Obj(inner, c) => text_object(b, inner, c, n),
            Target::M(n, m) => {
                // « cw » change jusqu'à la fin du mot (comme Vim)
                let m = if op == "c" { match m { M::Word(big) if b.t.get(b.cur).map(|c| !c.is_whitespace()).unwrap_or(false) => M::End(big), x => x } } else { m };
                let (to, kind) = self.motion(b, &m, n, Some(op))?;
                let from = b.cur;
                match kind {
                    Kind::Line => {
                        let (a, c) = (from.min(to), from.max(to));
                        let e = b.line_end(c);
                        Some((b.line_start(a), (e + 1).min(b.len()).max(e), true))
                    }
                    Kind::Incl => { let (a, c) = (from.min(to), from.max(to)); Some((a, (c + 1).min(b.len()), false)) }
                    Kind::Excl => {
                        let (a, mut c) = (from.min(to), from.max(to));
                        // « dw » sur le dernier mot d'une ligne ne prend pas la ligne suivante
                        if matches!(m, M::Word(_)) && b.line_of(c) > b.line_of(a) {
                            c = b.line_end(a).max(a);
                        }
                        Some((a, c, false))
                    }
                }
            }
        }
    }

    fn yank(&mut self, text: String, linewise: bool, out: &mut Vec<Out>) {
        if text.is_empty() { return; }
        self.reg = (text.clone(), linewise);
        out.push(Out::Copy(text));
    }

    fn operate(&mut self, b: &mut Buf, op: &str, s: usize, e: usize, lw: bool, out: &mut Vec<Out>) {
        match op {
            "d" | "c" => {
                let mut s = s;
                let mut e = e;
                let mut shifted = false;
                if lw && op == "d" && e == b.len() && s > 0 && !b.get(s, e).ends_with('\n') {
                    // dernières lignes : on enlève aussi le '\n' qui les précède
                    s -= 1;
                    shifted = true;
                }
                if lw && op == "c" {
                    // « cc » garde l'indentation et la ligne
                    let ind = b.indent_of(s);
                    if e > s && b.t.get(e - 1) == Some(&'\n') { e -= 1; }
                    let txt = b.get(s, e);
                    self.yank(format!("{txt}\n"), true, out);
                    b.delete(s, e);
                    b.insert(s, &ind);
                    b.cur = s + ind.chars().count();
                    self.mode = Mode::Insert;
                    return;
                }
                let mut txt = b.get(if shifted { s + 1 } else { s }, e);
                if lw && !txt.ends_with('\n') { txt.push('\n'); }
                self.yank(txt, lw, out);
                b.delete(s, e);
                b.cur = s.min(b.len());
                if op == "c" { self.mode = Mode::Insert; }
                else if lw { b.cur = b.first_non_blank(b.cur.min(b.len())); }
            }
            "y" => {
                let mut txt = b.get(s, e);
                if lw && !txt.ends_with('\n') { txt.push('\n'); }
                self.yank(txt, lw, out);
                if !lw || s < b.line_start(b.cur) { b.cur = s; }
            }
            ">" | "<" => {
                let (l1, l2) = (b.line_of(s), b.line_of(e.saturating_sub(1).max(s)));
                for l in l1..=l2 {
                    let ls = b.line_pos(l);
                    if op == ">" {
                        if b.line_end(ls) > ls { let u = b.indent.clone(); b.insert(ls, &u); }
                    } else {
                        let w = b.indent.chars().count().max(1);
                        let n = (0..w).take_while(|k| b.t.get(ls + k) == Some(&' ')).count();
                        let n = if n == 0 && b.t.get(ls) == Some(&'\t') { 1 } else { n };
                        b.delete(ls, ls + n);
                    }
                }
                b.cur = b.first_non_blank(b.line_pos(l1));
            }
            "gc" => {
                let (l1, l2) = (b.line_of(s), b.line_of(e.saturating_sub(1).max(s)));
                toggle_comment(b, l1, l2);
                b.cur = b.first_non_blank(b.line_pos(l1));
            }
            "gu" | "gU" | "g~" => {
                let txt: String = b.get(s, e);
                let new: String = txt.chars().map(|c| match op {
                    "gu" => c.to_lowercase().collect::<String>(),
                    "gU" => c.to_uppercase().collect(),
                    _ => if c.is_uppercase() { c.to_lowercase().collect() } else { c.to_uppercase().collect() },
                }).collect();
                if new != txt { b.delete(s, e); b.insert(s, &new); }
                b.cur = s;
            }
            _ => {}
        }
    }

    fn command(&mut self, b: &mut Buf, n: usize, c: &str, arg: Option<char>, out: &mut Vec<Out>) {
        let visual = self.mode != Mode::Normal;
        let line_s = b.line_start(b.cur);
        let line_e = b.line_end(b.cur);
        match c {
            "i" => { self.mode = Mode::Insert; }
            "a" => { if b.cur < line_e { b.cur += 1; } self.mode = Mode::Insert; }
            "I" => { b.cur = b.first_non_blank(b.cur); self.mode = Mode::Insert; }
            "A" => { b.cur = line_e; self.mode = Mode::Insert; }
            "o" | "O" => {
                let mut ind = b.indent_of(b.cur);
                if c == "o" {
                    let t = b.get(line_s, line_e);
                    let tt = t.trim_end();
                    if tt.ends_with(':') || tt.ends_with('{') || tt.ends_with('(') || tt.ends_with('[') { ind.push_str(&b.indent.clone()); }
                    let s = format!("\n{ind}");
                    b.insert(line_e, &s);
                    b.cur = line_e + s.chars().count();
                } else {
                    let s = format!("{ind}\n");
                    b.insert(line_s, &s);
                    b.cur = line_s + ind.chars().count();
                }
                self.mode = Mode::Insert;
            }
            "v" | "V" => {
                let want = if c == "v" { Mode::Visual } else { Mode::VisualLine };
                if self.mode == want { self.leave_visual(b); }
                else { if self.mode == Mode::Normal { b.anchor = b.cur; } self.mode = want; }
            }
            "gv" => {
                if let Some((a, cc, m)) = self.last_visual { b.anchor = a.min(b.len()); b.cur = cc.min(b.len()); self.mode = m; }
            }
            "o_v" => { std::mem::swap(&mut b.anchor, &mut b.cur); }
            "\x1b" => {
                if visual { self.leave_visual(b); } else { out.push(Out::Highlight(None)); }
            }
            "x" | "X" | "s" | "Del" => {
                if visual { return self.op_visual(b, if c == "s" { "c" } else { "d" }, out); }
                let (s, e) = if c == "X" { (b.cur.saturating_sub(n).max(line_s), b.cur) } else { (b.cur, (b.cur + n).min(line_e)) };
                if s < e { let t = b.delete(s, e); self.yank(t, false, out); b.cur = s; }
                if c == "s" { self.mode = Mode::Insert; }
            }
            "S" => { self.mode = Mode::Normal; let t = Target::Line(n); if let Some((s, e, lw)) = self.target_range(b, "c", n, t) { self.operate(b, "c", s, e, lw, out); } }
            "D" | "C" => {
                if visual { return self.op_visual(b, if c == "D" { "d" } else { "c" }, out); }
                let e = if n > 1 { b.line_end(b.line_pos((b.line_of(b.cur) + n - 1).min(b.lines() - 1))) } else { line_e };
                let t = b.delete(b.cur, e);
                self.yank(t, false, out);
                if c == "C" { self.mode = Mode::Insert; }
            }
            "Y" => {
                if visual { return self.op_visual(b, "y", out); }
                if let Some((s, e, _)) = self.target_range(b, "y", n, Target::Line(n)) {
                    let mut t = b.get(s, e);
                    if !t.ends_with('\n') { t.push('\n'); }
                    self.yank(t, true, out);
                }
            }
            "r" => {
                let Some(ch) = arg else { return };
                if visual {
                    let (s, e, _) = self.visual_range(b);
                    for k in s..e { if b.t[k] != '\n' { b.t[k] = ch; } }
                    b.changed = true;
                    self.leave_visual(b);
                    b.cur = s;
                    return;
                }
                if b.cur + n <= line_e {
                    let rep: String = std::iter::repeat(ch).take(n).collect();
                    b.delete(b.cur, b.cur + n);
                    if ch == '\n' { b.insert(b.cur, "\n"); b.cur += 1; } else { b.insert(b.cur, &rep); b.cur += n - 1; }
                }
            }
            "J" | "gJ" => {
                let (s0, e0) = if visual { let (s, e, _) = self.visual_range(b); self.leave_visual(b); (b.line_of(s), b.line_of(e.saturating_sub(1)).max(b.line_of(s) + 1)) }
                    else { (b.line_of(b.cur), b.line_of(b.cur) + n.max(2) - 1) };
                for _ in s0..e0.min(b.lines() - 1) {
                    let e = b.line_end(b.line_pos(s0));
                    if e >= b.len() { break; }
                    let mut k = e + 1;
                    while k < b.len() && matches!(b.t[k], ' ' | '\t') { k += 1; }
                    let join = if c == "gJ" || b.t.get(k) == Some(&')') || k >= b.len() || b.t.get(k) == Some(&'\n') || e == b.line_start(e) { "" } else { " " };
                    b.delete(e, k);
                    b.insert(e, join);
                    b.cur = e;
                }
            }
            "~" => {
                if visual { return self.op_visual(b, "g~", out); }
                let e = (b.cur + n).min(line_e);
                let t: String = b.get(b.cur, e).chars().map(|c| if c.is_uppercase() { c.to_lowercase().collect::<String>() } else { c.to_uppercase().collect() }).collect();
                if b.cur < e { b.delete(b.cur, e); b.insert(b.cur, &t); }
                b.cur = e.min(line_e);
            }
            "u_v" => self.op_visual(b, "gu", out),
            "U_v" => self.op_visual(b, "gU", out),
            "p" | "P" => {
                let (text, lw) = match &b.clip {
                    Some(c) if !c.is_empty() && *c != self.reg.0 => (c.clone(), c.ends_with('\n')),
                    _ => self.reg.clone(),
                };
                if text.is_empty() { return; }
                if visual {
                    let (s, e, vlw) = self.visual_range(b);
                    self.leave_visual(b);
                    let old = b.delete(s, e);
                    let mut t = text.clone();
                    if lw && !vlw { t = t.trim_end_matches('\n').to_string(); }
                    if vlw && !lw { t.push('\n'); }
                    b.insert(s, &t);
                    b.cur = s;
                    self.reg = (old, vlw);
                    return;
                }
                let t: String = text.repeat(n);
                if lw {
                    let at = if c == "p" { (line_e + 1).min(b.len()) } else { line_s };
                    let t = if c == "p" && line_e == b.len() { format!("\n{}", t.trim_end_matches('\n')) } else { t };
                    b.insert(at, &t);
                    b.cur = b.first_non_blank(if c == "p" && line_e == b.len() { at + 1 } else { at });
                } else {
                    let at = if c == "p" && b.cur < line_e { b.cur + 1 } else { b.cur };
                    b.insert(at, &t);
                    b.cur = at + t.chars().count() - 1;
                }
            }
            "u" => out.push(Out::Undo(n)),
            "^r" => out.push(Out::Redo(n)),
            "." => {
                let keys = self.last_change.clone();
                if keys.is_empty() { return; }
                self.replaying = true;
                for _ in 0..n.max(1) {
                    for k in keys.iter().cloned() {
                        let o = self.key(b, k);
                        out.extend(o.into_iter().filter(|o| matches!(o, Out::Copy(_))));
                    }
                    if self.mode == Mode::Insert { let _ = self.key(b, K::Esc); }
                }
                self.replaying = false;
            }
            ":" => {
                self.cmdline = Some(if visual { self.last_visual = Some((b.anchor, b.cur, self.mode)); ":'<,'>".into() } else { ":".into() });
            }
            "/" | "?" => { self.cmdline = Some(c.to_string()); }
            "m" => { if let Some(ch) = arg { self.marks.insert(ch, b.cur); } }
            "leader" => out.push(Out::Leader),
            "H" | "[b" | "gT" => out.push(Out::Buffer(-1)),
            "L" | "]b" | "gt" => out.push(Out::Buffer(1)),
            "^h" | "^j" | "^k" | "^l" => out.push(Out::Window(c.chars().nth(1).unwrap_or('l'))),
            "^/" => out.push(Out::Terminal),
            "zz" | "zt" | "zb" => out.push(Out::Scroll(c.chars().nth(1).unwrap_or('z'))),
            "ZZ" => out.push(Out::Cmd("x".into())),
            "ZQ" => out.push(Out::Cmd("q!".into())),
            "gcc" => { let l = b.line_of(b.cur); toggle_comment(b, l, (l + n - 1).min(b.lines() - 1)); b.cur = b.first_non_blank(b.line_pos(l)); }
            "gc_v" => self.op_visual(b, "gc", out),
            "a-j" | "a-k" => {
                // Alt+j / Alt+k : déplacer la ligne (ou les lignes sélectionnées), comme LazyVim
                let (l1, l2) = if visual { let (s, e, _) = self.visual_range(b); (b.line_of(s), b.line_of(e.saturating_sub(1).max(s))) } else { (b.line_of(b.cur), b.line_of(b.cur)) };
                let down = c == "a-j";
                if (down && l2 + 1 >= b.lines()) || (!down && l1 == 0) { return; }
                let col = b.col(b.cur);
                let acol = b.col(b.anchor);
                let (al, cl) = (b.line_of(b.anchor), b.line_of(b.cur));
                let s = b.line_pos(l1);
                let e = b.line_end(b.line_pos(l2));
                let block = b.get(s, e);
                if down {
                    let ne = b.line_end(e + 1);
                    let next = b.get(e + 1, ne);
                    b.delete(s, ne);
                    b.insert(s, &format!("{next}\n{block}"));
                } else {
                    let ps = b.line_pos(l1 - 1);
                    let prev = b.get(ps, s - 1);
                    b.delete(ps, e);
                    b.insert(ps, &format!("{block}\n{prev}"));
                }
                let d: isize = if down { 1 } else { -1 };
                let put = |b: &Buf, l: usize, c: usize| { let p = b.line_pos(l); (p + c).min(b.line_end(p)) };
                b.cur = put(b, (cl as isize + d) as usize, col);
                b.anchor = put(b, (al as isize + d) as usize, acol);
            }
            ">_v" | "<_v" => {
                let (s, e, _) = self.visual_range(b);
                let op = &c[..1];
                self.operate(b, op, s, e, true, out);
                // LazyVim garde la sélection après > et <
                let (l1, l2) = (b.line_of(s.min(b.len())), b.line_of(e.saturating_sub(1).max(s).min(b.len())));
                b.anchor = b.line_pos(l1);
                b.cur = b.line_end(b.line_pos(l2)).saturating_sub(1).max(b.line_pos(l2));
                if self.mode == Mode::Visual { self.mode = Mode::VisualLine; }
            }
            "^d" | "^u" => {
                let half = (b.page / 2).max(1);
                let l = b.line_of(b.cur);
                let col = self.want_col.unwrap_or(b.col(b.cur));
                let nl = if c == "^d" { (l + half).min(b.lines() - 1) } else { l.saturating_sub(half) };
                let p = b.line_pos(nl);
                b.cur = (p + col).min(b.line_end(p));
                out.push(Out::Scroll('z'));
            }
            o if o.starts_with("obj") => {
                let mut it = o.chars().skip(3);
                let (inner, oc) = (it.next() == Some('i'), it.next().unwrap_or('w'));
                if let Some((s, e, lw)) = text_object(b, inner, oc, n) {
                    b.anchor = s;
                    b.cur = e.saturating_sub(1).max(s);
                    if lw { self.mode = Mode::VisualLine; }
                }
            }
            _ => {}
        }
    }

    fn op_visual(&mut self, b: &mut Buf, op: &str, out: &mut Vec<Out>) {
        let (s, e, lw) = self.visual_range(b);
        self.leave_visual(b);
        self.operate(b, op, s, e, lw, out);
    }

    // ------------------------------------------------------------------ mouvements
    fn motion(&mut self, b: &mut Buf, m: &M, n: usize, op: Option<&str>) -> Option<(usize, Kind)> {
        let cur = b.cur;
        let ls = b.line_start(cur);
        let le = b.line_end(cur);
        let r = match m {
            M::Left => (cur.saturating_sub(n).max(ls), Kind::Excl),
            M::Right => {
                let lim = if op.is_some() || self.mode != Mode::Normal { le } else { le.saturating_sub(1).max(ls) };
                ((cur + n).min(lim), Kind::Excl)
            }
            M::Up | M::Down => {
                let l = b.line_of(cur);
                let nl = if *m == M::Down { l + n } else { l.checked_sub(n)? };
                if nl >= b.lines() { if op.is_some() { return None; } return Some((cur, Kind::Line)); }
                let col = *self.want_col.get_or_insert(cur - ls);
                let p = b.line_pos(nl);
                let e = b.line_end(p);
                let maxc = if self.mode == Mode::Normal && e > p { e - 1 } else { e };
                ((p + col).min(maxc), Kind::Line)
            }
            M::Word(big) => {
                let mut p = cur;
                for _ in 0..n {
                    if p >= b.len() { break; }
                    let c0 = cls(b.t[p], *big);
                    while p < b.len() && cls(b.t[p], *big) == c0 && c0 != 0 { p += 1; }
                    while p < b.len() && cls(b.t[p], *big) == 0 {
                        if b.t[p] == '\n' && p + 1 < b.len() && b.t[p + 1] == '\n' { p += 1; break; }
                        p += 1;
                    }
                }
                (p.min(b.len()), Kind::Excl)
            }
            M::Back(big) => {
                let mut p = cur;
                for _ in 0..n {
                    while p > 0 && cls(b.t[p - 1], *big) == 0 { p -= 1; }
                    if p == 0 { break; }
                    let c0 = cls(b.t[p - 1], *big);
                    while p > 0 && cls(b.t[p - 1], *big) == c0 { p -= 1; }
                }
                (p, Kind::Excl)
            }
            M::End(big) => {
                let mut p = cur;
                for _ in 0..n {
                    p += 1;
                    while p < b.len() && cls(b.t[p], *big) == 0 { p += 1; }
                    if p >= b.len() { p = b.len().saturating_sub(1); break; }
                    let c0 = cls(b.t[p], *big);
                    while p + 1 < b.len() && cls(b.t[p + 1], *big) == c0 { p += 1; }
                }
                (p, Kind::Incl)
            }
            M::BackEnd(big) => {
                let mut p = cur;
                for _ in 0..n {
                    if p == 0 { break; }
                    let c0 = cls(b.t[p], *big);
                    while p > 0 && cls(b.t[p], *big) == c0 && c0 != 0 { p -= 1; }
                    while p > 0 && cls(b.t[p], *big) == 0 { p -= 1; }
                }
                (p, Kind::Incl)
            }
            M::LineStart => (ls, Kind::Excl),
            M::FirstNonBlank => (b.first_non_blank(cur), Kind::Excl),
            M::LineEnd => {
                let l = b.line_of(cur) + n - 1;
                let p = b.line_pos(l.min(b.lines() - 1));
                let e = b.line_end(p);
                self.want_col = Some(usize::MAX);
                (if op.is_none() && self.mode == Mode::Normal && e > p { e - 1 } else if op.is_some() { e.saturating_sub(1).max(p) } else { e }, Kind::Incl)
            }
            M::Goto(line) => {
                let l = line.map(|x| x.saturating_sub(1)).unwrap_or(b.lines() - 1).min(b.lines() - 1);
                (b.first_non_blank(b.line_pos(l)), Kind::Line)
            }
            M::FileStart => (b.first_non_blank(b.line_pos(n.saturating_sub(1).min(b.lines() - 1))), Kind::Line),
            M::Find(kind, ch) => {
                self.last_find = Some((*kind, *ch));
                find_char(b, cur, *kind, *ch, n, false)?
            }
            M::RepeatFind(rev) => {
                let (kind, ch) = self.last_find?;
                let kind = if *rev { match kind { 'f' => 'F', 'F' => 'f', 't' => 'T', _ => 't' } } else { kind };
                find_char(b, cur, kind, ch, n, true)?
            }
            M::Match => (match_bracket(b, cur)?, Kind::Incl),
            M::ParaF | M::ParaB => {
                let mut l = b.line_of(cur);
                let fwd = *m == M::ParaF;
                for _ in 0..n {
                    let blank = |b: &Buf, l: usize| { let p = b.line_pos(l); b.line_end(p) == p };
                    if fwd { while l + 1 < b.lines() && blank(b, l) { l += 1; } while l + 1 < b.lines() && !blank(b, l) { l += 1; } }
                    else { while l > 0 && blank(b, l) { l -= 1; } while l > 0 && !blank(b, l) { l -= 1; } }
                }
                let p = b.line_pos(l);
                (if fwd && l + 1 >= b.lines() && b.line_end(p) != p { b.line_end(p).saturating_sub(1).max(p) } else { p }, Kind::Excl)
            }
            M::Search(rev) => {
                let s = self.last_search.clone()?;
                let s = if *rev { (s.0, !s.1) } else { s };
                let mut p = cur;
                for _ in 0..n {
                    match search(b, p, &s, false) { Some(x) => p = x, None => { self.not_found(); return None; } }
                }
                (p, Kind::Excl)
            }
            M::Star(fwd) => {
                let (s, e) = word_at(b, cur)?;
                let w = b.get(s, e);
                self.last_search = Some((w, *fwd));
                let mut p = s;
                for _ in 0..n {
                    match search(b, p, self.last_search.as_ref()?, true) { Some(x) => p = x, None => return None }
                }
                (p, Kind::Excl)
            }
            M::HalfDown | M::HalfUp => return None,
            M::Mark(ch, lw) => {
                let p = *self.marks.get(ch)?;
                let p = p.min(b.len());
                if *lw { (b.first_non_blank(p), Kind::Line) } else { (p, Kind::Excl) }
            }
            M::CurLine => {
                let l = (b.line_of(cur) + n - 1).min(b.lines() - 1);
                (b.first_non_blank(b.line_pos(l)), Kind::Line)
            }
        };
        Some(r)
    }
}

// ---------------------------------------------------------------------- analyse des touches
fn parse(keys: &[K], visual: bool) -> P {
    let mut i = 0;
    let ch = |i: usize| -> Option<char> { match keys.get(i) { Some(K::Ch(c)) => Some(*c), _ => None } };
    // registre « "x » : accepté et ignoré (un seul registre, partagé avec le presse-papiers)
    if ch(0) == Some('"') {
        if keys.len() < 2 { return P::More; }
        i = 2;
    }
    let mut count = 0usize;
    let mut has_count = false;
    while let Some(c) = ch(i) {
        if c.is_ascii_digit() && (c != '0' || has_count) { count = count * 10 + c.to_digit(10).unwrap_or(0) as usize; has_count = true; i += 1; } else { break; }
    }
    let n = if has_count { count.max(1) } else { 1 };
    let Some(k) = keys.get(i) else { return P::More };
    // touches spéciales
    match k {
        K::Esc => return P::Cmd(1, "\x1b".into(), None),
        K::Ctrl(c) => return match c {
            'r' => P::Cmd(n, "^r".into(), None),
            'd' => P::Cmd(n, "^d".into(), None),
            'u' => P::Cmd(n, "^u".into(), None),
            'h' | 'j' | 'k' | 'l' => P::Cmd(1, format!("^{c}"), None),
            '/' | '_' => P::Cmd(1, "^/".into(), None),
            _ => P::Bad,
        },
        K::Alt(c) if *c == 'j' || *c == 'k' => return P::Cmd(1, format!("a-{c}"), None),
        K::Up => return P::Motion(n, M::Up),
        K::Down => return P::Motion(n, M::Down),
        K::Left | K::Back => return P::Motion(n, M::Left),
        K::Right => return P::Motion(n, M::Right),
        K::Home => return P::Motion(n, M::LineStart),
        K::End => return P::Motion(n, M::LineEnd),
        K::Enter => return P::Motion(n, M::Down),
        K::Del => return P::Cmd(n, "Del".into(), None),
        K::Tab => return P::Bad,
        _ => {}
    }
    let c = ch(i).unwrap_or('\0');
    // un mouvement seul (aussi la cible d'un opérateur)
    let motion = |j: usize, n: usize| -> Result<Option<M>, ()> {
        let Some(c) = ch(j) else { return if keys.get(j).is_none() { Err(()) } else { Ok(None) } };
        Ok(Some(match c {
            'h' => M::Left, 'l' | ' ' => M::Right, 'j' => M::Down, 'k' => M::Up,
            'w' => M::Word(false), 'W' => M::Word(true), 'b' => M::Back(false), 'B' => M::Back(true),
            'e' => M::End(false), 'E' => M::End(true), '0' => M::LineStart, '^' => M::FirstNonBlank, '_' => M::CurLine,
            '$' => M::LineEnd, 'G' => M::Goto(if has_count || n > 1 { Some(n) } else { None }),
            '%' => M::Match, '}' => M::ParaF, '{' => M::ParaB, 'n' => M::Search(false), 'N' => M::Search(true),
            '*' => M::Star(true), '#' => M::Star(false), ';' => M::RepeatFind(false), ',' => M::RepeatFind(true),
            'f' | 'F' | 't' | 'T' => match ch(j + 1) { Some(x) => M::Find(c, x), None => return if keys.get(j + 1).is_none() { Err(()) } else { Ok(None) } },
            '\'' | '`' => match ch(j + 1) { Some(x) => M::Mark(x, c == '\''), None => return if keys.get(j + 1).is_none() { Err(()) } else { Ok(None) } },
            'g' => match ch(j + 1) { Some('g') => M::FileStart, Some('e') => M::BackEnd(false), Some('E') => M::BackEnd(true), Some(_) => return Ok(None), None => return if keys.get(j + 1).is_none() { Err(()) } else { Ok(None) } },
            _ => return Ok(None),
        }))
    };
    // Espace : touche leader de LazyVim
    if c == ' ' && !has_count && !visual { return P::Cmd(1, "leader".into(), None); }
    if c == ' ' && visual { return P::Cmd(1, "leader".into(), None); }
    // opérateurs
    let op: Option<&str> = match c {
        'd' => Some("d"), 'c' => Some("c"), 'y' => Some("y"), '>' => Some(">"), '<' => Some("<"),
        'g' => match ch(i + 1) { Some('c') => Some("gc"), Some('u') => Some("gu"), Some('U') => Some("gU"), Some('~') => Some("g~"), None if keys.get(i + 1).is_none() => return P::More, _ => None },
        _ => None,
    };
    if let Some(op) = op {
        if visual {
            return match op { ">" | "<" => P::Cmd(1, format!("{op}_v"), None), "gc" => P::Cmd(1, "gc_v".into(), None),
                              "d" | "c" | "y" => P::Op(1, op.into(), Target::Line(1)), _ => P::Op(1, op.into(), Target::Line(1)) };
        }
        let j = i + op.chars().count();
        // second compteur (« d2w »)
        let mut j2 = j;
        let mut n2 = 0usize;
        let mut has2 = false;
        while let Some(d) = ch(j2) { if d.is_ascii_digit() && (d != '0' || has2) { n2 = n2 * 10 + d.to_digit(10).unwrap_or(0) as usize; has2 = true; j2 += 1; } else { break; } }
        let total = n * if has2 { n2.max(1) } else { 1 };
        let Some(t) = ch(j2) else { return if keys.get(j2).is_none() { P::More } else { P::Bad } };
        // doublé : dd, cc, yy, >>, <<, gcc, guu…
        let last = op.chars().last().unwrap_or(' ');
        if t == last || (op.len() == 2 && (t == 'g' && ch(j2 + 1) == Some(last))) {
            if op == "gc" { return P::Cmd(total, "gcc".into(), None); }
            return P::Op(total, op.into(), Target::Line(total));
        }
        if t == 'i' || t == 'a' {
            return match ch(j2 + 1) { Some(o) => P::Op(total, op.into(), Target::Obj(t == 'i', o)), None => if keys.get(j2 + 1).is_none() { P::More } else { P::Bad } };
        }
        return match motion(j2, total) {
            Err(()) => P::More,
            Ok(Some(m)) => P::Op(1, op.into(), Target::M(total, m)),
            Ok(None) => P::Bad,
        };
    }
    // objets de texte en mode visuel (« viw », « va( »)
    if visual && (c == 'i' || c == 'a') {
        return match ch(i + 1) { Some(o) => P::Cmd(n, format!("obj{c}{o}"), None), None => P::More };
    }
    match motion(i, n) {
        Err(()) => return P::More,
        Ok(Some(m)) => return P::Motion(n, m),
        Ok(None) => {}
    }
    // commandes
    let two = |a: char| ch(i + 1).map(|x| format!("{a}{x}"));
    let cmd: String = match c {
        'g' => match two('g') { Some(s) if ["gv", "gJ", "gt", "gT"].contains(&s.as_str()) => s, Some(_) => return P::Bad, None => return P::More },
        'z' => match two('z') { Some(s) if ["zz", "zt", "zb"].contains(&s.as_str()) => s, Some(_) => return P::Bad, None => return P::More },
        'Z' => match two('Z') { Some(s) if s == "ZZ" || s == "ZQ" => s, Some(_) => return P::Bad, None => return P::More },
        '[' | ']' => match ch(i + 1) { Some('b') => format!("{c}b"), Some(_) => return P::Bad, None => return P::More },
        'r' | 'm' => return match keys.get(i + 1) {
            Some(K::Ch(x)) => P::Cmd(n, c.to_string(), Some(*x)),
            Some(K::Enter) if c == 'r' => P::Cmd(n, "r".into(), Some('\n')),
            Some(_) => P::Bad,
            None => P::More,
        },
        'o' if visual => "o_v".into(),
        'u' if visual => "u_v".into(),
        'U' if visual => "U_v".into(),
        'i' | 'a' | 'I' | 'A' | 'o' | 'O' | 'v' | 'V' | 'x' | 'X' | 's' | 'S' | 'D' | 'C' | 'Y' | 'J' | '~' | 'p' | 'P' | 'u' | '.' | ':' | '/' | '?' | 'H' | 'L' => c.to_string(),
        _ => return P::Bad,
    };
    P::Cmd(n, cmd, None)
}


// ---------------------------------------------------------------------- outils
fn find_char(b: &Buf, cur: usize, kind: char, ch: char, n: usize, repeat: bool) -> Option<(usize, Kind)> {
    let (ls, le) = (b.line_start(cur), b.line_end(cur));
    let mut p = cur;
    for k in 0..n {
        match kind {
            'f' | 't' => {
                let start = if kind == 't' && (repeat || k > 0) { p + 2 } else { p + 1 };
                let q = (start..le).find(|&x| b.t[x] == ch)?;
                p = if kind == 't' { q - 1 } else { q };
            }
            _ => {
                let end = if kind == 'T' && (repeat || k > 0) { p.saturating_sub(1) } else { p };
                let q = (ls..end).rev().find(|&x| b.t[x] == ch)?;
                p = if kind == 'T' { q + 1 } else { q };
            }
        }
    }
    Some((p, if kind == 'f' || kind == 't' { Kind::Incl } else { Kind::Excl }))
}

fn match_bracket(b: &Buf, cur: usize) -> Option<usize> {
    let le = b.line_end(cur);
    let start = (cur..le).find(|&k| "()[]{}".contains(b.t[k]))?;
    let c = b.t[start];
    let (open, close, fwd) = match c { '(' => ('(', ')', true), ')' => ('(', ')', false), '[' => ('[', ']', true), ']' => ('[', ']', false), '{' => ('{', '}', true), _ => ('{', '}', false) };
    let mut depth = 0i32;
    if fwd {
        for k in start..b.len() { if b.t[k] == open { depth += 1; } else if b.t[k] == close { depth -= 1; if depth == 0 { return Some(k); } } }
    } else {
        for k in (0..=start).rev() { if b.t[k] == close { depth += 1; } else if b.t[k] == open { depth -= 1; if depth == 0 { return Some(k); } } }
    }
    None
}

fn word_at(b: &Buf, cur: usize) -> Option<(usize, usize)> {
    let le = b.line_end(cur);
    let mut s = (cur..le).find(|&k| cls(b.t[k], false) == 1)?;
    while s > 0 && cls(b.t[s - 1], false) == 1 { s -= 1; }
    let mut e = s;
    while e < b.len() && cls(b.t[e], false) == 1 { e += 1; }
    Some((s, e))
}

fn eq_ci(a: char, b: char, ci: bool) -> bool { if ci { a.to_lowercase().eq(b.to_lowercase()) } else { a == b } }

/// Occurrence suivante (ou précédente) de la recherche, en reprenant au début (ignorecase + smartcase).
fn search(b: &Buf, from: usize, s: &(String, bool), whole: bool) -> Option<usize> {
    let q: Vec<char> = s.0.chars().collect();
    if q.is_empty() || q.len() > b.len() { return None; }
    let ci = !s.0.chars().any(|c| c.is_uppercase());
    let at = |p: usize| -> bool {
        p + q.len() <= b.len() && q.iter().enumerate().all(|(k, c)| eq_ci(b.t[p + k], *c, ci))
            && (!whole || ((p == 0 || cls(b.t[p - 1], false) != 1) && (p + q.len() >= b.len() || cls(b.t[p + q.len()], false) != 1)))
    };
    let n = b.len() - q.len() + 1;
    if s.1 {
        (from + 1..n).chain(0..(from + 1).min(n)).find(|&p| at(p))
    } else {
        (0..from.min(n)).rev().chain((from.min(n)..n).rev()).find(|&p| at(p))
    }
}

fn replace_in(line: &str, pat: &str, rep: &str, all: bool, ci: bool) -> (String, usize) {
    let hits = if ci { crate::editor::find_all_ci(line, pat, usize::MAX) } else { line.match_indices(pat).map(|(i, m)| (i, i + m.len())).collect() };
    let mut out = String::new();
    let mut last = 0;
    let mut n = 0;
    for (s, e) in hits {
        if s < last { continue; }
        out.push_str(&line[last..s]);
        out.push_str(rep);
        last = e;
        n += 1;
        if !all { break; }
    }
    out.push_str(&line[last..]);
    (out, n)
}

/// Objets de texte : iw aw iW aW, i( a( ib, i{ a{ iB, i[ a[, i< a<, i" a" i' a' i` a`, ip ap.
fn text_object(b: &Buf, inner: bool, c: char, n: usize) -> Option<(usize, usize, bool)> {
    let cur = b.cur.min(b.len().saturating_sub(1));
    match c {
        'w' | 'W' => {
            if b.len() == 0 { return None; }
            let big = c == 'W';
            let k = cls(b.t[cur], big);
            let mut s = cur;
            while s > 0 && b.t[s - 1] != '\n' && cls(b.t[s - 1], big) == k { s -= 1; }
            let mut e = cur;
            while e < b.len() && b.t[e] != '\n' && cls(b.t[e], big) == k { e += 1; }
            for _ in 1..n { while e < b.len() && b.t[e] != '\n' && cls(b.t[e], big) == 0 { e += 1; } let k2 = b.t.get(e).map(|c| cls(*c, big)).unwrap_or(0); while e < b.len() && b.t[e] != '\n' && cls(b.t[e], big) == k2 && k2 != 0 { e += 1; } }
            if !inner {
                let e2 = { let mut x = e; while x < b.len() && matches!(b.t[x], ' ' | '\t') { x += 1; } x };
                if e2 > e { e = e2; } else { while s > 0 && matches!(b.t[s - 1], ' ' | '\t') { s -= 1; } }
            }
            Some((s, e, false))
        }
        '(' | ')' | 'b' | '{' | '}' | 'B' | '[' | ']' | '<' | '>' => {
            let (o, cl) = match c { '(' | ')' | 'b' => ('(', ')'), '{' | '}' | 'B' => ('{', '}'), '[' | ']' => ('[', ']'), _ => ('<', '>') };
            // ouvrante qui contient le curseur
            let mut depth = 0i32;
            let mut s = None;
            let mut k = cur as isize;
            if b.t.get(cur) == Some(&o) { s = Some(cur); } else {
                if b.t.get(cur) == Some(&cl) { k -= 1; depth = 1; }
                while k >= 0 {
                    let ch = b.t[k as usize];
                    if ch == cl { depth += 1; } else if ch == o { if depth == 0 { s = Some(k as usize); break; } depth -= 1; }
                    k -= 1;
                }
                if b.t.get(cur) == Some(&cl) && s.is_none() { return None; }
            }
            let s = s?;
            let mut depth = 0i32;
            let mut e = None;
            for k in s..b.len() { if b.t[k] == o { depth += 1; } else if b.t[k] == cl { depth -= 1; if depth == 0 { e = Some(k); break; } } }
            let e = e?;
            if inner {
                let (mut a, mut z) = (s + 1, e);
                // bloc sur plusieurs lignes : les lignes du milieu, accolades gardées sur leur ligne
                if b.t.get(a) == Some(&'\n') && (b.line_start(e)..e).all(|k| matches!(b.t[k], ' ' | '\t')) {
                    a += 1;
                    z = b.line_start(e);
                }
                Some((a, z.max(a), false))
            } else { Some((s, e + 1, false)) }
        }
        '"' | '\'' | '`' => {
            let (ls, le) = (b.line_start(cur), b.line_end(cur));
            let qs: Vec<usize> = (ls..le).filter(|&k| b.t[k] == c && (k == 0 || b.t[k - 1] != '\\')).collect();
            let pair = qs.chunks(2).find(|p| p.len() == 2 && p[0] <= cur && cur <= p[1])
                .or_else(|| qs.chunks(2).find(|p| p.len() == 2 && p[0] > cur))?;
            let (s, e) = (pair[0], pair[1]);
            if inner { Some((s + 1, e, false)) } else {
                let mut z = e + 1;
                while z < le && matches!(b.t[z], ' ' | '\t') { z += 1; }
                Some((s, z, false))
            }
        }
        'p' => {
            let blank = |l: usize| { let p = b.line_pos(l); b.line_end(p) == p };
            let mut l1 = b.line_of(cur);
            let kind = blank(l1);
            while l1 > 0 && blank(l1 - 1) == kind { l1 -= 1; }
            let mut l2 = b.line_of(cur);
            while l2 + 1 < b.lines() && blank(l2 + 1) == kind { l2 += 1; }
            if !inner { while l2 + 1 < b.lines() && blank(l2 + 1) != kind { l2 += 1; } }
            let s = b.line_pos(l1);
            let e = b.line_end(b.line_pos(l2));
            Some((s, (e + 1).min(b.len()).max(e), true))
        }
        _ => None,
    }
}

/// Commente / décommente les lignes l1..=l2 (comme « gc » de LazyVim).
fn toggle_comment(b: &mut Buf, l1: usize, l2: usize) {
    let (lc, bo, bc) = b.comment.clone();
    let (mark, close) = if lc.is_empty() { (bo, bc) } else { (lc, String::new()) };
    if mark.is_empty() { return; }
    let lines: Vec<(usize, usize)> = (l1..=l2).map(|l| { let p = b.line_pos(l); (p, b.line_end(p)) }).collect();
    let texts: Vec<String> = lines.iter().map(|(s, e)| b.get(*s, *e)).collect();
    let all = texts.iter().filter(|t| !t.trim().is_empty()).all(|t| t.trim_start().starts_with(mark.as_str()));
    let min_ind = texts.iter().filter(|t| !t.trim().is_empty()).map(|t| t.len() - t.trim_start().len()).min().unwrap_or(0);
    for (k, (s, e)) in lines.iter().enumerate().rev() {
        let t = &texts[k];
        if t.trim().is_empty() { continue; }
        let new = if all {
            let ind = t.len() - t.trim_start().len();
            let rest = &t[ind + mark.len()..];
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            let rest = if close.is_empty() { rest.to_string() } else { rest.trim_end().strip_suffix(close.as_str()).unwrap_or(rest).trim_end().to_string() };
            format!("{}{}", &t[..ind], rest)
        } else if close.is_empty() {
            format!("{}{} {}", &t[..min_ind], mark, &t[min_ind..])
        } else {
            format!("{}{} {} {}", &t[..min_ind], mark, &t[min_ind..], close)
        };
        if new != *t { b.delete(*s, *e); b.insert(*s, &new); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(s: &str) -> Vec<K> {
        let mut v = Vec::new();
        let mut it = s.chars().peekable();
        while let Some(c) = it.next() {
            let rest: String = it.clone().collect();
            let special = ["esc>", "cr>", "bs>", "c-", "a-"].iter().any(|p| rest.starts_with(p));
            if c == '<' && special {
                let name: String = it.by_ref().take_while(|c| *c != '>').collect();
                v.push(match name.as_str() { "esc" => K::Esc, "cr" => K::Enter, "bs" => K::Back,
                    n if n.starts_with("c-") => K::Ctrl(n.chars().nth(2).unwrap()), n if n.starts_with("a-") => K::Alt(n.chars().nth(2).unwrap()), _ => K::Esc });
            } else { v.push(K::Ch(c)); }
        }
        v
    }

    /// Tape les touches ; en mode Insertion le texte est écrit comme le ferait l'éditeur.
    fn run(text: &str, cur: usize, ks: &str) -> (String, usize, Vim) {
        let mut v = Vim::new();
        let mut b = Buf::new(text, cur);
        for k in keys(ks) {
            if v.mode == Mode::Insert && k != K::Esc {
                v.record_insert(k.clone());
                v.replaying = true;
                let mut o = Vec::new();
                v.insert_key(&mut b, k, &mut o);
                v.replaying = false;
                continue;
            }
            let _ = v.key(&mut b, k);
        }
        (b.text(), b.cur, v)
    }

    #[test]
    fn motions() {
        let t = "def f(a, b):\n    return a + b\n";
        assert_eq!(run(t, 0, "w").1, 4);
        assert_eq!(run(t, 0, "3w").1, 6);
        assert_eq!(run(t, 0, "e").1, 2);
        assert_eq!(run(t, 0, "$").1, 11);
        assert_eq!(run(t, 0, "j").1, 13);
        assert_eq!(run(t, 0, "j^").1, 17);
        assert_eq!(run(t, 5, "%").1, 10);
        assert_eq!(run(t, 0, "fb").1, 9);
        assert_eq!(run(t, 0, "f,;").1, 7);
        assert_eq!(run(t, 20, "gg").1, 0);
        assert_eq!(run(t, 0, "G").1, 30);
        assert_eq!(run(t, 0, "/return<cr>").1, 17);
        assert_eq!(run(t, 0, "*").1, 0);
    }

    #[test]
    fn operators() {
        assert_eq!(run("un deux trois", 0, "dw").0, "deux trois");
        assert_eq!(run("un deux trois", 3, "cwZ<esc>").0, "un Z trois");
        assert_eq!(run("a\nb\nc\n", 2, "dd").0, "a\nc\n");
        assert_eq!(run("a\nb\nc", 4, "dd").0, "a\nb");
        assert_eq!(run("a\nb\nc\n", 0, "2dd").0, "c\n");
        assert_eq!(run("a\nb\nc\n", 0, "yyp").0, "a\na\nb\nc\n");
        assert_eq!(run("a\nb\nc\n", 2, "ddP").0, "a\nb\nc\n");
        assert_eq!(run("f(un, deux)", 3, "ci(x<esc>").0, "f(x)");
        assert_eq!(run("s = \"bonjour\"", 6, "di\"").0, "s = \"\"");
        assert_eq!(run("un deux trois", 4, "daw").0, "un trois");
        assert_eq!(run("abc def", 0, "D").0, "");
        assert_eq!(run("abc def", 4, "x").0, "abc ef");
        assert_eq!(run("abc", 0, "~~").0, "ABc");
        assert_eq!(run("    x = 1", 4, "<<").0, "x = 1");
        assert_eq!(run("x = 1", 0, ">>").0, "    x = 1");
        assert_eq!(run("a\nb", 0, "J").0, "a b");
        assert_eq!(run("abc", 1, "rZ").0, "aZc");
        assert_eq!(run("x = 1\ny = 2", 0, "gcc").0, "# x = 1\ny = 2");
        assert_eq!(run("# x = 1\ny = 2", 0, "gcc").0, "x = 1\ny = 2");
        assert_eq!(run("x\ny\nz", 0, "gcj").0, "# x\n# y\nz");
        assert_eq!(run("abc def", 0, "gUiw").0, "ABC def");
    }

    #[test]
    fn insert_and_repeat() {
        assert_eq!(run("a", 0, "ob<esc>").0, "a\nb");
        assert_eq!(run("a", 0, "Ox<esc>").0, "x\na");
        assert_eq!(run("if x:", 0, "oy<esc>").0, "if x:\n    y");
        assert_eq!(run("abc", 0, "Az<esc>").0, "abcz");
        assert_eq!(run("un un un", 0, "cwX<esc>w.w.").0, "X X X");
        assert_eq!(run("a\nb\nc\nd", 0, "dd..").0, "d");
        assert_eq!(run("x1\nx2", 0, "Ay<esc>j.").0, "x1y\nx2y");
    }

    #[test]
    fn visual() {
        assert_eq!(run("abcdef", 1, "vld").0, "adef");
        assert_eq!(run("a\nb\nc\n", 0, "Vjd").0, "c\n");
        assert_eq!(run("a\nb\nc", 0, "Vj>").0, "    a\n    b\nc");
        assert_eq!(run("un deux", 3, "viwc2<esc>").0, "un 2");
        assert_eq!(run("a\nb", 0, "Vjgc").0, "# a\n# b");
        let (t, _, v) = run("abc", 0, "vly");
        assert_eq!(t, "abc");
        assert_eq!(v.reg.0, "ab");
    }

    #[test]
    fn ex_commands() {
        assert_eq!(run("a b a", 0, ":s/a/x/g<cr>").0, "x b x");
        assert_eq!(run("a\na", 0, ":%s/a/y/<cr>").0, "y\ny");
        assert_eq!(run("1\n2\n3", 0, ":3<cr>").1, 4);
        let mut v = Vim::new();
        let mut b = Buf::new("x", 0);
        for k in keys(":wq<cr>") { let o = v.key(&mut b, k); if let Some(Out::Cmd(c)) = o.first() { assert_eq!(c, "wq"); } }
        // Espace : leader, « u » : annuler, Ctrl+r : rétablir
        assert_eq!(Vim::new().key(&mut Buf::new("x", 0), K::Ch(' ')), vec![Out::Leader]);
        assert_eq!(Vim::new().key(&mut Buf::new("x", 0), K::Ch('u')), vec![Out::Undo(1)]);
        assert_eq!(Vim::new().key(&mut Buf::new("x", 0), K::Ctrl('r')), vec![Out::Redo(1)]);
    }

    #[test]
    fn alt_move_lines() {
        assert_eq!(run("a\nb\nc", 0, "<a-j>").0, "b\na\nc");
        assert_eq!(run("a\nb\nc", 4, "<a-k>").0, "a\nc\nb");
    }
}
