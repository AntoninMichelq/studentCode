//! Un fichier ouvert et sa zone d'édition : numéros de ligne, coloration, indentation automatique,
//! commentaire en un raccourci, recherche, points d'arrêt et ligne du débogueur.
use crate::highlight::{self, Lang};
use crate::theme::Palette;
use eframe::egui::{self, text::{CCursor, CharIndex}, text_selection::CCursorRange, Color32, FontId, Galley, Key,
    Modifiers, Rect, Sense, Shape, Stroke, TextEdit, Vec2};
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct Doc {
    pub path: Option<PathBuf>,
    pub text: String,
    saved_hash: u64,
    pub lang: Lang,
    pub id: egui::Id,
    /// coloration gardée d'une image à l'autre (texte, thème, taille). On ne garde pas la mise en
    /// page elle-même : egui la jette quand il redessine les lettres (thème clair ↔ sombre, image des
    /// lettres pleine…), et une mise en page gardée afficherait alors des morceaux d'autres lettres.
    cache: Option<(u64, String, f32, Arc<egui::text::LayoutJob>)>,
    /// dernière mise en page (seulement pour le défilement vers une ligne)
    last_galley: Option<Arc<Galley>>,
    pub breakpoints: BTreeSet<usize>, // lignes (à partir de 1)
    pub goto: Option<usize>,          // ligne à afficher au prochain dessin
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub find: Option<String>,
    pub find_next: bool,
    /// fichier d'une machine distante : (clé du lab, chemin sur la machine, nom du lab)
    pub remote: Option<(String, String, String)>,
    /// numéro du fichier sans nom (« sans_titre_1.py »), 0 pour un vrai fichier
    pub untitled: usize,
    /// extension du fichier sans nom (« py », « c »…)
    pub untitled_ext: String,
    /// blocs repliés (ligne d'en-tête, à partir de 1)
    pub folded: BTreeSet<usize>,
    /// blocs repliables : (hash du texte, [(en-tête, dernière ligne)])
    folds: Option<(u64, Vec<(usize, usize)>)>,
    lines_before: usize,
    /// « Commenter / décommenter » demandé depuis le menu ou le raccourci
    pub pending_comment: bool,
    /// « Dupliquer la ligne » demandé par le raccourci
    pub pending_duplicate: bool,
    /// Annuler, Copier, Coller… demandé par un menu
    pub pending_edit: Option<EditCmd>,
    /// donner le clavier à l'éditeur (onglet ouvert ou choisi)
    pub want_focus: bool,
    /// dernière sélection connue quand l'éditeur avait le clavier (ouvrir un menu la fait perdre)
    last_sel: Option<CCursorRange>,
    /// recherche vers le haut (bouton ↑ de la barre de recherche)
    pub find_back: bool,
    /// « Remplacer » : remplace l'occurrence sélectionnée par ce texte puis passe à la suivante
    pub replace_with: Option<String>,
    /// mode Vim (comme LazyVim)
    pub vim: crate::vim::Vim,
    /// texte surligné par la recherche Vim (/, *, n)
    pub vim_hl: Option<String>,
    /// curseur posé par Vim à l'image précédente (pour reconnaître un clic de souris)
    vim_set: Option<CCursorRange>,
    /// faire défiler pour garder le curseur Vim visible (4 lignes de marge, comme LazyVim)
    vim_scroll: Option<char>,
    /// position du curseur Vim (caractères)
    pub vim_cur: usize,
    vim_anchor: usize,
}

fn hash(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

impl Doc {
    pub fn new_empty(n: usize) -> Doc {
        Doc {
            path: None, text: String::new(), saved_hash: hash(""), lang: Lang::Plain,
            id: egui::Id::new(("doc", n, std::time::SystemTime::now())), cache: None, last_galley: None,
            breakpoints: BTreeSet::new(), goto: None, cursor_line: 1, cursor_col: 1, find: None, find_next: false,
            remote: None, untitled: n, untitled_ext: "py".into(), folded: BTreeSet::new(), folds: None, lines_before: 0, pending_comment: false, pending_duplicate: false, pending_edit: None, want_focus: false, last_sel: None, find_back: false, replace_with: None,
            vim: crate::vim::Vim::new(), vim_hl: None, vim_set: None, vim_scroll: None, vim_cur: 0, vim_anchor: 0,
        }
    }

    /// Nouveau fichier Python sans nom (comme la version Python : « sans_titre_N.py »).
    pub fn untitled(n: usize, text: &str) -> Doc {
        Doc::untitled_as(n, text, "py")
    }

    /// Fichier sans nom d'un type donné (« c », « java »… ; « Makefile » pour un Makefile).
    pub fn untitled_as(n: usize, text: &str, ext: &str) -> Doc {
        let mut d = Doc::new_empty(n);
        d.untitled_ext = ext.to_string();
        d.lang = Lang::detect(&d.title());
        d.text = text.to_string();
        d.saved_hash = hash(text);
        d
    }

    pub fn open(path: &Path) -> Result<Doc, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() > 20_000_000 {
            return Err("fichier trop gros pour l'éditeur (plus de 20 Mo)".into());
        }
        if bytes.iter().take(8000).any(|&b| b == 0) {
            return Err("fichier binaire".into());
        }
        let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
        let mut d = Doc::new_empty(0);
        d.untitled = 0;
        d.id = egui::Id::new(("doc", path));
        d.saved_hash = hash(&text);
        d.text = text;
        d.lang = Lang::detect(&path.to_string_lossy());
        d.path = Some(path.to_path_buf());
        Ok(d)
    }

    pub fn from_remote(key: &str, path: &str, label: &str, text: String) -> Doc {
        let mut d = Doc::new_empty(0);
        d.id = egui::Id::new(("remote", key, path));
        d.saved_hash = hash(&text);
        d.text = text;
        d.lang = Lang::detect(path);
        d.remote = Some((key.to_string(), path.to_string(), label.to_string()));
        d
    }

    pub fn mark_saved(&mut self) {
        self.saved_hash = hash(&self.text);
    }

    pub fn dirty(&self) -> bool {
        hash(&self.text) != self.saved_hash
    }

    pub fn title(&self) -> String {
        if let Some((_, p, _)) = &self.remote {
            return p.rsplit('/').next().unwrap_or(p).to_string();
        }
        self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| if self.untitled > 0 {
                if self.untitled_ext.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) { self.untitled_ext.clone() }
                else if self.untitled_ext.is_empty() { format!("sans_titre_{}", self.untitled) }
                else { format!("sans_titre_{}.{}", self.untitled, self.untitled_ext) }
            } else { "sans titre".into() })
    }

    /// Enlève les espaces en fin de ligne (option des Paramètres, en enregistrant).
    pub fn trim_trailing(&mut self) {
        let mut out: String = self.text.split('\n').map(|l| l.trim_end_matches([' ', '\t'])).collect::<Vec<_>>().join("\n");
        if out.len() != self.text.len() {
            std::mem::swap(&mut out, &mut self.text);
            self.cache = None;
        }
    }

    pub fn save(&mut self) -> Result<(), String> {
        let path = self.path.clone().ok_or("pas de nom de fichier")?;
        std::fs::write(&path, self.text.as_bytes()).map_err(|e| e.to_string())?;
        self.saved_hash = hash(&self.text);
        Ok(())
    }

    pub fn save_as(&mut self, path: PathBuf) -> Result<(), String> {
        self.lang = Lang::detect(&path.to_string_lossy());
        self.path = Some(path);
        self.cache = None;
        self.save()
    }

    /// Recharge le fichier s'il a changé sur le disque et n'a pas été modifié ici.
    pub fn reload_if_changed(&mut self) {
        if self.dirty() {
            return;
        }
        if let Some(p) = &self.path {
            if let Ok(t) = std::fs::read_to_string(p) {
                let t = t.replace("\r\n", "\n");
                if hash(&t) != self.saved_hash {
                    self.saved_hash = hash(&t);
                    self.text = t;
                }
            }
        }
    }

    /// Largeur d'un niveau d'indentation du fichier (2 ou 4 colonnes, devinée sur ses premières lignes).
    fn indent_unit(&self) -> usize {
        let mut two = 0;
        let mut four = 0;
        for l in self.text.lines().take(400) {
            if let Some(n) = indent_of(l) {
                if l.starts_with('\t') { return 4; }
                if n > 0 && n % 4 == 0 { four += 1 } else if n % 2 == 0 && n > 0 { two += 1 }
            }
        }
        if two > four / 3 && two > 0 { 2 } else { 4 }
    }

    /// Blocs repliables : accolades pour les langages à accolades, indentation pour Python et YAML
    /// (comme les lexers QScintilla de la version Python). Calculé une fois par version du texte.
    fn fold_ranges(&mut self) -> Vec<(usize, usize)> {
        let h = hash(&self.text);
        if let Some((k, v)) = &self.folds { if *k == h { return v.clone(); } }
        let mut out = Vec::new();
        let lines: Vec<&str> = self.text.split('\n').collect();
        match self.lang {
            Lang::Plain | Lang::Markdown => {}
            Lang::Python | Lang::Yaml => {
                for i in 0..lines.len() {
                    let Some(ind) = indent_of(lines[i]) else { continue };
                    let mut last = i;
                    let mut j = i + 1;
                    while j < lines.len() {
                        match indent_of(lines[j]) {
                            None => {}
                            Some(n) if n > ind => last = j,
                            Some(_) => break,
                        }
                        j += 1;
                    }
                    if last > i { out.push((i + 1, last + 1)); }
                }
            }
            _ => {
                // accolades (hors chaînes et commentaires de fin de ligne, approximatif comme Scintilla)
                let mut stack: Vec<usize> = Vec::new();
                for (i, l) in lines.iter().enumerate() {
                    let code = l.split("//").next().unwrap_or("");
                    let mut in_str: Option<char> = None;
                    let mut prev = ' ';
                    for c in code.chars() {
                        match in_str {
                            Some(q) => { if c == q && prev != '\\' { in_str = None; } }
                            None => match c {
                                '"' | '\'' => in_str = Some(c),
                                '{' => stack.push(i),
                                '}' => { if let Some(o) = stack.pop() { if i > o { out.push((o + 1, i + 1)); } } }
                                _ => {}
                            },
                        }
                        prev = c;
                    }
                }
                out.sort();
                out.dedup_by_key(|r| r.0); // une seule flèche par ligne (le bloc le plus large)
            }
        }
        self.folds = Some((h, out.clone()));
        out
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, p: &Palette, font: &FontId, debug_line: Option<usize>,
              error_line: Option<usize>, show_breakpoints: bool, o: &EditorOpts) -> EditorEvents {
        let mut events = EditorEvents::default();
        let (cw, row_h) = ui.fonts_mut(|f| (f.glyph_width(font, '0'), f.row_height(font)));
        let lines = self.text.matches('\n').count() + 1;
        // un changement du nombre de lignes déplace les blocs : on déplie tout
        if lines != self.lines_before { self.folded.clear(); self.lines_before = lines; }
        // marges de QScintilla dans la version Python : numéros (« 00000 »), points d'arrêt (20 px), pliage (18 px)
        let num_w = if o.line_numbers { 5.0 * cw + 4.0 } else { 4.0 };
        let sym_w = 20.0;
        let fold_w = if o.folding { 18.0 } else { 0.0 };
        let gutter_w = num_w + sym_w + fold_w;
        if !o.folding { self.folded.clear(); }
        let folds = if o.folding { self.fold_ranges() } else { Vec::new() };
        let folded_ranges: Vec<(usize, usize)> = folds.iter().filter(|(h, _)| self.folded.contains(h)).copied().collect();
        let hidden = |line: usize| folded_ranges.iter().any(|(h, e)| line > *h && line <= *e);
        let te_id = self.id.with("te");
        if self.want_focus && !ui.input(|i| i.pointer.any_down() || i.pointer.any_released()) {
            self.want_focus = false;
            ui.memory_mut(|m| m.request_focus(te_id));
        } else if self.want_focus {
            ui.ctx().request_repaint();
        }
                // commande d'un menu : attendre la fin du clic qui a fermé le menu, sinon l'éditeur perd
        // aussitôt le clavier et la sélection
        if self.pending_edit.is_some() {
            if ui.input(|i| i.pointer.any_down() || i.pointer.any_released()) {
                ui.ctx().request_repaint();
            } else if let Some(cmd) = self.pending_edit.take() {
                self.apply_edit(ui.ctx(), te_id, cmd);
            }
        }
        let focused = ui.memory(|m| m.has_focus(te_id));
        if !o.vim && self.vim.mode != crate::vim::Mode::Insert { self.vim = crate::vim::Vim::new(); self.vim.mode = crate::vim::Mode::Insert; }
        if o.vim && self.vim.mode == crate::vim::Mode::Insert && self.vim_set.is_none() && !focused { self.vim.mode = crate::vim::Mode::Normal; }
        if o.vim && focused {
            let page = (ui.available_height() / row_h).max(4.0) as usize;
            self.vim_input(ui, te_id, o, page, &mut events);
        }
        let vim_normal = o.vim && self.vim.mode != crate::vim::Mode::Insert;
        if focused || self.pending_comment || self.pending_duplicate {
            if !vim_normal || self.pending_comment || self.pending_duplicate { self.handle_keys(ui, te_id, o); }
        }
        // recherche : sélectionner l'occurrence suivante
        let mut scroll_to: Option<usize> = self.goto.take();
        if self.find_next || self.find_back || self.replace_with.is_some() {
            let back = std::mem::take(&mut self.find_back) && !self.find_next;
            self.find_next = false;
            let repl = self.replace_with.take();
            if let Some(q) = self.find.clone().filter(|q| !q.is_empty()) {
                let mut state = egui::TextEdit::load_state(ui.ctx(), te_id);
                let ql = q.to_lowercase();
                // « Remplacer » : l'occurrence sélectionnée est remplacée
                if let Some(r) = &repl {
                    if let Some(range) = state.as_ref().and_then(|s| s.cursor.char_range()) {
                        let sr = range.as_sorted_char_range();
                        let (bs, be) = (char_to_byte(&self.text, sr.start.0), char_to_byte(&self.text, sr.end.0));
                        if be > bs && match_ci_at(&self.text, bs, &ql.chars().collect::<Vec<_>>()) == Some(be) {
                            self.text.replace_range(bs..be, r);
                            let c = sr.start.0 + r.chars().count();
                            let mut st = state.clone().unwrap_or_default();
                            st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(CharIndex(c)))));
                            st.clone().store(ui.ctx(), te_id);
                            state = Some(st);
                            events.changed = true;
                        }
                    }
                }
                let sel = state.as_ref().and_then(|s| s.cursor.char_range()).map(|r| r.as_sorted_char_range());
                let hit = if back {
                    let to = sel.map(|r| char_to_byte(&self.text, r.start.0)).unwrap_or(self.text.len());
                    find_ci(&self.text, &q, to, true)
                } else {
                    let from = sel.map(|r| char_to_byte(&self.text, r.end.0)).unwrap_or(0);
                    find_ci(&self.text, &q, from, false)
                };
                if let Some((b, be)) = hit {
                    let s = self.text[..b].chars().count();
                    let e = s + self.text[b..be].chars().count();
                    let mut st = state.unwrap_or_default();
                    st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(CharIndex(s)), CCursor::new(CharIndex(e)))));
                    st.store(ui.ctx(), te_id);
                    scroll_to = Some(self.text[..b].matches('\n').count() + 1);
                    events.found = true;
                } else {
                    events.not_found = true;
                }
            }
        }
        if let Some(l) = scroll_to {
            // une ligne à montrer dans un bloc replié : on le déplie
            let open: Vec<usize> = folded_ranges.iter().filter(|(h, e)| l > *h && l <= *e).map(|r| r.0).collect();
            for h in open { self.folded.remove(&h); }
        }
        let full = ui.available_rect_before_wrap();
        let gutter_rect = Rect::from_min_size(full.min, Vec2::new(gutter_w, full.height()));
        let text_rect = Rect::from_min_max(egui::pos2(full.min.x + gutter_w, full.min.y), full.max);
        ui.painter().rect_filled(full, 0.0, p.bg);
        let gutter_resp = ui.interact(gutter_rect, self.id.with("gutter"), Sense::click());
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        let mut sa = if o.word_wrap { egui::ScrollArea::vertical() } else { egui::ScrollArea::both() }
            .id_salt(self.id.with("scroll")).auto_shrink(false);
        if let Some(line) = scroll_to {
            let y = self.last_galley.as_ref().and_then(|g| { let st = line_starts(g); st.get(line.saturating_sub(1)).and_then(|r| g.rows.get(*r)).map(|r| r.pos.y) })
                .unwrap_or((line.saturating_sub(1)) as f32 * row_h);
            sa = sa.vertical_scroll_offset((y - text_rect.height() / 3.0).max(0.0));
        }
        let lang = self.lang;
        let pal = *p;
        let font_c = font.clone();
        let cache = &mut self.cache;
        let last = &mut self.last_galley;
        let fold_key: Vec<usize> = self.folded.iter().copied().collect();
        let wrap_w = if o.word_wrap { (text_rect.width() - 14.0).max(80.0) } else { f32::INFINITY };
        let theme_key = format!("{:?}{:?}{:?}{}", p.fg, p.bg, fold_key, wrap_w.round());
        let hidden_bytes: Vec<(usize, usize)> = {
            // octets des lignes cachées : du début de la ligne après l'en-tête à la fin du bloc
            let starts: Vec<usize> = std::iter::once(0).chain(self.text.match_indices('\n').map(|(i, _)| i + 1)).collect();
            folded_ranges.iter().map(|(h, e)| {
                let a = starts.get(*h).copied().unwrap_or(self.text.len());
                let b = starts.get(*e).copied().unwrap_or(self.text.len());
                (a, b)
            }).collect()
        };
        let bg_slot_holder: std::cell::Cell<Option<egui::layers::ShapeIdx>> = std::cell::Cell::new(None);
        let vim_scroll = self.vim_scroll.take();
        let vim_on = o.vim;
        let out = sa.show(&mut child, |ui| {
            bg_slot_holder.set(Some(ui.painter().add(Shape::Noop)));
            let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap: f32| -> Arc<Galley> {
                let text = buf.as_str();
                let h = hash(text);
                let job = match cache.as_ref() {
                    Some((ch, key, size, j)) if *ch == h && *key == theme_key && *size == font_c.size => j.clone(),
                    _ => {
                        let mut job = highlight::highlight(text, lang, &pal, &font_c);
                        if !hidden_bytes.is_empty() && hidden_bytes.iter().all(|(_, b)| *b <= text.len()) {
                            job = fold_job(job, &hidden_bytes);
                        }
                        job.wrap.max_width = wrap_w;
                        let j = Arc::new(job);
                        *cache = Some((h, theme_key.clone(), font_c.size, j.clone()));
                        j
                    }
                };
                // mise en page refaite par egui (qu'il garde lui-même tant que les lettres sont valables)
                let g = ui.fonts_mut(|f| f.layout_job((*job).clone()));
                *last = Some(g.clone());
                g
            };
            if vim_normal { ui.visuals_mut().text_cursor.stroke.width = 0.0; }
            let r = TextEdit::multiline(&mut self.text)
                .id(te_id)
                .font(font.clone())
                .code_editor()
                .frame(egui::Frame::NONE)
                .margin(egui::Margin { left: 0, right: 6, top: 2, bottom: 2 })
                .desired_width(f32::INFINITY)
                .desired_rows((text_rect.height() / row_h) as usize)
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(ui);
            // mode Vim : Échap est pour Vim (egui retirerait sinon le clavier à l'éditeur)
            if vim_on && r.response.response.has_focus() {
                ui.memory_mut(|m| m.set_focus_lock_filter(te_id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true }));
            }
            // Vim a bougé le curseur : le garder visible avec 4 lignes de marge (scrolloff de LazyVim)
            if let (Some(how), Some(cr)) = (vim_scroll, r.cursor_range) {
                let c = r.galley.pos_from_cursor(cr.primary);
                let rect = c.translate(r.galley_pos.to_vec2());
                let rect = match how {
                    'z' => rect.expand2(Vec2::new(0.0, ui.clip_rect().height() / 2.0 - row_h)),
                    't' => Rect::from_min_size(rect.min, Vec2::new(rect.width(), ui.clip_rect().height() - row_h)),
                    'b' => Rect::from_min_max(rect.max - Vec2::new(rect.width(), ui.clip_rect().height() - row_h), rect.max),
                    _ => rect.expand2(Vec2::new(0.0, row_h * 4.0)),
                };
                ui.scroll_to_rect(rect, None);
            }
            r
        });
        let te = out.inner;
        let offset = out.state.clamped_offset();
        let galley_pos = te.galley_pos;
        let mut bg_shapes = Vec::new();
        let mut fg_shapes = Vec::new();
        let galley = te.galley.clone();
        // lignes → rangées de la mise en page (une ligne peut en avoir plusieurs avec le retour à la
        // ligne automatique, une ligne repliée n'a presque pas de hauteur)
        let starts = line_starts(&galley);
        let row_y = |line: usize| -> (f32, f32) {
            let l = line.saturating_sub(1);
            match (starts.get(l), galley.rows.get(starts.get(l).copied().unwrap_or(usize::MAX))) {
                (Some(&r0), Some(r)) => {
                    let r1 = starts.get(l + 1).copied().unwrap_or(galley.rows.len()).max(r0 + 1);
                    let bottom = galley.rows.get(r1 - 1).map(|x| x.pos.y + x.rect().height()).unwrap_or(r.pos.y + r.rect().height());
                    (galley_pos.y + r.pos.y, bottom - r.pos.y)
                }
                _ => (galley_pos.y + l as f32 * row_h, row_h),
            }
        };
        let line_of_row = |row: usize| -> usize { starts.partition_point(|&s| s <= row).max(1) };
        // première ligne visible
        let first_vis = line_of_row(galley.rows.partition_point(|r| r.pos.y + r.rect().height() < offset.y)) - 1;
        let line_rect = |line: usize| -> Rect {
            let (y, h) = row_y(line);
            Rect::from_min_size(egui::pos2(text_rect.min.x, y), Vec2::new(text_rect.width().max(4000.0), h))
        };
        if ui.memory(|m| m.has_focus(te_id)) { if let Some(r) = te.cursor_range { self.last_sel = Some(r); } }
        if let Some(r) = te.cursor_range {
            let c = r.primary.index.0;
            let byte = char_to_byte(&self.text, c);
            self.cursor_line = self.text[..byte].matches('\n').count() + 1;
            self.cursor_col = self.text[..byte].rsplit('\n').next().map(|s| s.chars().count()).unwrap_or(0) + 1;
        }
        // le curseur entre dans un bloc replié (flèches, clic) : le bloc se déplie
        if hidden(self.cursor_line) {
            let open: Vec<usize> = folded_ranges.iter().filter(|(h, e)| self.cursor_line > *h && self.cursor_line <= *e).map(|r| r.0).collect();
            for h in open { self.folded.remove(&h); }
            ui.ctx().request_repaint();
        }
        // ligne du curseur : bleu très léger, toujours visible (comme QScintilla, alpha 28/255)
        if o.caret_line {
            bg_shapes.push(Shape::rect_filled(line_rect(self.cursor_line), 0.0,
                                              p.blue.gamma_multiply(if p.light { 45.0 / 255.0 } else { 28.0 / 255.0 })));
        }
        // guides d'indentation en pointillés, à partir du 2e niveau (comme QScintilla)
        if o.indent_guides {
            let unit = self.indent_unit();
            let count = (text_rect.height() / row_h) as usize + 3;
            for (k, l) in self.text.split('\n').enumerate().skip(first_vis).take(count) {
                let line = k + 1;
                let Some(ind) = indent_of(l) else { continue };
                let (y, h) = row_y(line);
                if h < 2.0 { continue; }
                let mut lvl = 1;
                while lvl * unit < ind && lvl < 40 {
                    let x = (galley_pos.x + (lvl * unit) as f32 * cw).round();
                    let mut yy = y.round();
                    while yy < y + h - 0.5 {
                        bg_shapes.push(Shape::rect_filled(Rect::from_min_size(egui::pos2(x, yy), Vec2::splat(1.0)), 0.0, p.bg_hl));
                        yy += 2.0;
                    }
                    lvl += 1;
                }
            }
        }
        // accolade correspondante : fond gris, caractère orange (rouge si elle n'a pas de paire)
        if let Some(r) = te.cursor_range.filter(|r| r.is_empty() && o.bracket_match) {
            let cur = char_to_byte(&self.text, r.primary.index.0);
            let found = matching_bracket(&self.text, cur);
            let cells: Vec<(usize, Color32)> = match found {
                Some((a, b)) => vec![(a, p.orange), (b, p.orange)],
                None => {
                    let bb = self.text.as_bytes();
                    let at = [Some(cur), cur.checked_sub(1)].into_iter().flatten()
                        .find(|&i| bb.get(i).map(|c| b"()[]{}".contains(c)).unwrap_or(false));
                    at.map(|i| vec![(i, p.red)]).unwrap_or_default()
                }
            };
            for (byte, colr) in cells {
                let ci = self.text[..byte].chars().count();
                let rr = te.galley.pos_from_cursor(CCursor::new(CharIndex(ci)));
                let cell = Rect::from_min_size(galley_pos + rr.min.to_vec2(), Vec2::new(cw, rr.height()));
                if found.is_some() { bg_shapes.push(Shape::rect_filled(cell, 0.0, p.gutter)); } else { bg_shapes.push(Shape::rect_filled(cell, 0.0, p.bg)); }
                let ch = self.text[byte..].chars().next().unwrap_or(' ').to_string();
                fg_shapes.push((cell.min, ch, colr));
            }
        }
        if let Some(l) = error_line {
            bg_shapes.push(Shape::rect_filled(line_rect(l), 0.0, p.err_bg));
        }
        if let Some(l) = debug_line {
            bg_shapes.push(Shape::rect_filled(line_rect(l), 0.0, p.curline));
        }
        // curseur bloc de Vim (modes Normal et Visuel)
        if vim_normal && ui.memory(|m| m.has_focus(te_id)) {
            let ci = self.vim_cur.min(self.text.chars().count());
            let r0 = te.galley.pos_from_cursor(CCursor::new(CharIndex(ci)));
            let next = self.text.chars().nth(ci);
            let w = match next {
                Some(c) if c != '\n' => {
                    let r1 = te.galley.pos_from_cursor(CCursor::new(CharIndex(ci + 1)));
                    if (r1.min.y - r0.min.y).abs() < 1.0 { (r1.min.x - r0.min.x).max(cw * 0.5) } else { cw }
                }
                _ => cw,
            };
            let rr = Rect::from_min_size(galley_pos + r0.min.to_vec2(), Vec2::new(w, r0.height()));
            bg_shapes.push(Shape::rect_filled(rr, 1.0, p.blue.gamma_multiply(if p.light { 0.45 } else { 0.6 })));
        }
        let hl = self.find.clone().filter(|q| q.chars().count() >= 2).or_else(|| if o.vim { self.vim_hl.clone().filter(|q| !q.is_empty()) } else { None });
        if let Some(q) = hl.as_ref() {
            let hits = find_all_ci(&self.text, q, 2001);
            let count = hits.len();
            for (b, be) in hits.into_iter().take(2000) {
                let ci = self.text[..b].chars().count();
                let r0 = te.galley.pos_from_cursor(CCursor::new(CharIndex(ci)));
                let r1 = te.galley.pos_from_cursor(CCursor::new(CharIndex(ci + self.text[b..be].chars().count())));
                let rr = Rect::from_min_max(galley_pos + r0.min.to_vec2(), galley_pos + egui::vec2(r1.max.x, r0.max.y));
                bg_shapes.push(Shape::rect_filled(rr, 2.0, p.orange.gamma_multiply(0.30)));
            }
            events.matches = count;
        }
        if let Some(slot) = bg_slot_holder.get() {
            child.painter().set(slot, Shape::Vec(bg_shapes));
        }
        for (pos, ch, colr) in fg_shapes {
            child.painter().text(pos, egui::Align2::LEFT_TOP, ch, font.clone(), colr);
        }
        // marges : numéros, points d'arrêt / ligne du débogueur, flèches de pliage
        let painter = ui.painter_at(gutter_rect);
        painter.rect_filled(gutter_rect, 0.0, p.bg);
        let visible = (gutter_rect.height() / row_h).ceil() as usize + 2;
        let headers: std::collections::HashMap<usize, usize> = folds.iter().copied().collect();
        let mut shown = 0;
        let mut i = first_vis;
        while i < lines && shown < visible + 200 {
            let line = i + 1;
            i += 1;
            let (y, rh) = row_y(line);
            if rh < 2.0 { continue; }
            shown += 1;
            if y > gutter_rect.max.y { break; }
            let cy = y + row_h.min(rh) / 2.0;
            if o.line_numbers {
                // numéros relatifs (LazyVim) : distance au curseur, numéro réel sur la ligne du curseur
                let here = line == self.cursor_line;
                let txt = if o.relative_numbers && !here { line.abs_diff(self.cursor_line).to_string() } else { line.to_string() };
                painter.text(egui::pos2(gutter_rect.min.x + num_w - 4.0, y + row_h.min(rh) / 2.0), egui::Align2::RIGHT_CENTER,
                             txt, font.clone(), if here && o.relative_numbers { p.orange } else { p.gutter });
            }
            let sym = egui::pos2(gutter_rect.min.x + num_w + sym_w / 2.0, cy);
            if self.breakpoints.contains(&line) {
                painter.circle_filled(sym, (rh * 0.36).min(7.0), p.red);
            }
            if Some(line) == debug_line {
                let s = (rh * 0.36).min(7.0);
                painter.add(Shape::convex_polygon(vec![sym + Vec2::new(-s, -s), sym + Vec2::new(s, 0.0), sym + Vec2::new(-s, s)], p.yellow, Stroke::NONE));
            }
            if headers.contains_key(&line) {
                let fc = egui::pos2(gutter_rect.min.x + num_w + sym_w + fold_w / 2.0 - 1.0, cy);
                if self.folded.contains(&line) {
                    crate::icons::paint(&painter, crate::icons::Icon::ChevRight, fc, 14.0, p.blue);
                } else {
                    crate::icons::paint(&painter, crate::icons::Icon::ChevDown, fc, 14.0, p.comment);
                }
            }
        }
        if gutter_resp.clicked() {
            if let Some(pos) = gutter_resp.interact_pointer_pos() {
                // ligne sous le clic, d'après la mise en page (les blocs repliés n'ont pas de hauteur)
                let gy = pos.y - galley_pos.y;
                let idx = galley.rows.partition_point(|r| r.pos.y + r.rect().height() <= gy);
                let line = line_of_row(idx).min(lines);
                if o.folding && pos.x >= gutter_rect.min.x + num_w + sym_w {
                    if headers.contains_key(&line) && !self.folded.remove(&line) {
                        self.folded.insert(line);
                    }
                } else if show_breakpoints && line <= lines {
                    if !self.breakpoints.remove(&line) {
                        self.breakpoints.insert(line);
                    }
                    events.breakpoint = Some(line);
                }
            }
        }
        if te.response.response.changed() {
            events.changed = true;
        }
        // clic droit : les actions de base, comme dans tout éditeur
        let sel = te.cursor_range.map(|r| !r.is_empty()).unwrap_or(false);
        let (can_undo, can_redo) = egui::TextEdit::load_state(ui.ctx(), te_id).and_then(|st| {
            let r = st.cursor.char_range()?;
            let cur = (r, self.text.clone());
            let u = st.undoer();
            Some((u.has_undo(&cur), u.has_redo(&cur)))
        }).unwrap_or((false, false));
        te.response.response.context_menu(|ui| {
            ui.set_min_width(250.0);
            let item = |ui: &mut egui::Ui, t: &str, k: &str, on: bool| crate::icons::menu_action(ui, None, t, k, on).clicked();
            if item(ui, "Annuler", "Ctrl+Z", can_undo) { self.pending_edit = Some(EditCmd::Undo); ui.close(); }
            if item(ui, "Rétablir", "Ctrl+Y", can_redo) { self.pending_edit = Some(EditCmd::Redo); ui.close(); }
            ui.separator();
            if item(ui, "Couper", "Ctrl+X", sel) { self.pending_edit = Some(EditCmd::Cut); ui.close(); }
            if item(ui, "Copier", "Ctrl+C", sel) { self.pending_edit = Some(EditCmd::Copy); ui.close(); }
            if item(ui, "Coller", "Ctrl+V", true) {
                if let Some(t) = crate::clip::get() { self.pending_edit = Some(EditCmd::Paste(t)); }
                ui.close();
            }
            if item(ui, "Supprimer", "Suppr", sel) { self.pending_edit = Some(EditCmd::Delete); ui.close(); }
            ui.separator();
            if item(ui, "Tout sélectionner", "Ctrl+A", true) { self.pending_edit = Some(EditCmd::SelectAll); ui.close(); }
            if item(ui, "Commenter / décommenter", "", true) { self.pending_comment = true; ui.close(); }
            if item(ui, "Dupliquer la ligne", "", true) { self.pending_duplicate = true; ui.close(); }
        });
        if self.pending_edit.is_some() || self.pending_comment || self.pending_duplicate { ui.ctx().request_repaint(); }
        // ligne de commande Vim (« : », « / ») et messages, en bas de l'éditeur
        if o.vim {
            let line = self.vim.cmdline.clone();
            let msg = self.vim.msg.clone();
            let show = self.vim.showcmd();
            if line.is_some() || msg.is_some() || !show.is_empty() {
                let h = row_h + 8.0;
                let bar = Rect::from_min_max(egui::pos2(full.min.x, full.max.y - h), full.max);
                let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, self.id.with("vimbar")))
                    .with_clip_rect(full);
                if line.is_some() || msg.is_some() {
                    painter.rect_filled(bar, 0.0, p.bg_dark);
                    painter.line_segment([bar.left_top(), bar.right_top()], Stroke::new(1.0, p.bg_hl));
                }
                let pos = egui::pos2(bar.min.x + 8.0, bar.center().y);
                if let Some(l) = &line {
                    let g = painter.layout_no_wrap(l.clone(), font.clone(), p.fg);
                    let w = g.size().x;
                    painter.galley(pos - Vec2::new(0.0, g.size().y / 2.0), g, p.fg);
                    painter.rect_filled(Rect::from_min_size(egui::pos2(pos.x + w + 1.0, bar.min.y + 4.0), Vec2::new(cw, row_h)), 1.0, p.fg.gamma_multiply(0.7));
                } else if let Some(m) = &msg {
                    let col = if m.starts_with('E') { p.red } else { p.fg };
                    painter.text(pos, egui::Align2::LEFT_CENTER, m, font.clone(), col);
                }
                if !show.is_empty() {
                    painter.text(egui::pos2(bar.max.x - 12.0, bar.center().y), egui::Align2::RIGHT_CENTER, show, font.clone(), p.comment);
                }
            }
        }
        ui.allocate_rect(full, Sense::hover());
        events
    }

    /// Touches du clavier → Vim. En mode Insertion, l'éditeur écrit lui-même (Vim garde seulement
    /// la trace pour « . ») ; dans les autres modes, les touches vont à Vim et pas au texte.
    fn vim_input(&mut self, ui: &mut egui::Ui, te_id: egui::Id, o: &EditorOpts, page: usize, events: &mut EditorEvents) {
        use crate::vim::{Buf, Mode, Out, K};
        let ctx = ui.ctx().clone();
        let mut state = egui::TextEdit::load_state(&ctx, te_id).unwrap_or_default();
        let n = self.text.chars().count();
        // clic ou glisser de la souris : le curseur (ou la sélection) vient d'elle
        let now = state.cursor.char_range();
        // fichier qui vient d'être ouvert : egui met le curseur à la fin, Vim le veut au début
        let clicked = ui.input(|i| i.pointer.any_released() || i.pointer.any_down());
        if self.vim_set.is_none() && !clicked && self.vim.mode != Mode::Insert {
            let r = CCursorRange::one(CCursor::new(CharIndex(self.vim_cur.min(n))));
            state.cursor.set_char_range(Some(r));
            state.clone().store(&ctx, te_id);
            self.vim_set = Some(r);
        }
        if let Some(r) = now.filter(|_| self.vim_set.is_some() || clicked) {
            if Some(r) != self.vim_set {
                if self.vim.mode == Mode::Insert {
                    self.vim_cur = r.primary.index.0;
                } else if r.is_empty() {
                    if matches!(self.vim.mode, Mode::Visual | Mode::VisualLine) && self.vim_set.is_some() { self.vim.mode = Mode::Normal; }
                    self.vim_cur = r.primary.index.0;
                } else {
                    // sélection à la souris : mode Visuel
                    self.vim.mode = Mode::Visual;
                    self.vim_anchor = r.secondary.index.0;
                    self.vim_cur = r.primary.index.0.saturating_sub(if r.primary.index.0 > r.secondary.index.0 { 1 } else { 0 });
                }
            }
        }
        self.vim_cur = self.vim_cur.min(n);
        // touches
        let insert = self.vim.mode == Mode::Insert && self.vim.cmdline.is_none();
        let mut keys: Vec<K> = Vec::new();
        ctx.input_mut(|i| {
            let mut keep = Vec::new();
            for e in std::mem::take(&mut i.events) {
                let mut eat = false;
                match &e {
                    egui::Event::Key { key, pressed: true, modifiers: m, .. } => {
                        let ctrl = m.ctrl || m.command;
                        let k = vim_key(*key, ctrl, m.alt, m.shift);
                        if insert {
                            match k {
                                Some(K::Esc) => { keys.push(K::Esc); eat = true; }
                                Some(K::Enter) if !ctrl && !m.alt => self.vim.record_insert(K::Enter),
                                Some(K::Back) if !ctrl && !m.alt => self.vim.record_insert(K::Back),
                                Some(K::Tab) if !ctrl && !m.alt && !m.shift => self.vim.record_insert(K::Tab),
                                Some(K::Del) if !ctrl && !m.alt => self.vim.record_insert(K::Del),
                                Some(K::Alt(c)) => { keys.push(K::Esc); keys.push(K::Alt(c)); keys.push(K::Ch('a')); eat = true; }
                                _ => {}
                            }
                        } else if let Some(k) = k {
                            keys.push(k);
                            eat = true;
                        } else if !ctrl {
                            // lettres, chiffres, signes : le texte arrive par l'événement Text
                            eat = true;
                        }
                    }
                    egui::Event::Key { .. } => { eat = !insert; }
                    egui::Event::Text(t) => {
                        if insert {
                            for c in t.chars() { self.vim.record_insert(K::Ch(c)); }
                        } else {
                            keys.extend(t.chars().map(K::Ch));
                            eat = true;
                        }
                    }
                    egui::Event::Paste(_) | egui::Event::Cut if !insert => eat = true,
                    _ => {}
                }
                if !eat { keep.push(e); }
            }
            i.events = keep;
        });
        if keys.is_empty() {
            self.vim_set = state.cursor.char_range();
            return;
        }
        let (lc, bo, bc) = self.lang.comments();
        let mut b = Buf::new(&self.text, self.vim_cur);
        b.anchor = self.vim_anchor.min(n);
        b.page = page;
        b.indent = o.indent_text();
        b.comment = (lc.to_string(), bo.to_string(), bc.to_string());
        if keys.iter().any(|k| matches!(k, K::Ch('p') | K::Ch('P'))) { b.clip = crate::clip::get(); }
        let before = (state.cursor.char_range().unwrap_or(CCursorRange::one(CCursor::new(CharIndex(self.vim_cur)))), self.text.clone());
        let mut outs = Vec::new();
        let mut rest = keys.into_iter();
        for k in rest.by_ref() {
            self.vim.msg = None;
            let o2 = self.vim.key(&mut b, k);
            let leader = o2.iter().any(|x| *x == Out::Leader);
            outs.extend(o2);
            if leader { break; }
        }
        events.leader_keys.extend(rest);
        let mut undoer = state.undoer();
        if b.changed {
            undoer.add_undo(&before);
            self.text = b.text();
            self.cache = None;
            events.changed = true;
        }
        self.vim_cur = b.cur;
        self.vim_anchor = b.anchor;
        for out in outs {
            match out {
                Out::Undo(k) | Out::Redo(k) => {
                    let redo = matches!(out, Out::Redo(_));
                    for _ in 0..k.max(1) {
                        let cur = (CCursorRange::one(CCursor::new(CharIndex(self.vim_cur))), self.text.clone());
                        let r = if redo { undoer.redo(&cur).cloned() } else { undoer.undo(&cur).cloned() };
                        match r {
                            Some((c, t)) => { self.text = t; self.vim_cur = c.primary.index.0.min(c.secondary.index.0); self.cache = None; events.changed = true; }
                            None => { self.vim.msg = Some(if redo { "Déjà à la modification la plus récente".into() } else { "Déjà à la modification la plus ancienne".into() }); break; }
                        }
                    }
                }
                Out::Copy(t) => ctx.copy_text(t),
                Out::Highlight(q) => self.vim_hl = q,
                Out::Scroll(c) => self.vim_scroll = Some(c),
                other => events.vim.push(other),
            }
        }
        // curseur et sélection pour l'éditeur
        let n = self.text.chars().count();
        let cur = self.vim_cur.min(n);
        let a = self.vim_anchor.min(n);
        let range = match self.vim.mode {
            Mode::Visual => if cur >= a { CCursorRange::two(CCursor::new(CharIndex(a)), CCursor::new(CharIndex((cur + 1).min(n)))) }
                            else { CCursorRange::two(CCursor::new(CharIndex((a + 1).min(n))), CCursor::new(CharIndex(cur))) },
            Mode::VisualLine => {
                let (lo, hi) = (a.min(cur), a.max(cur));
                let chars: Vec<char> = self.text.chars().collect();
                let ls = chars[..lo].iter().rposition(|c| *c == '\n').map(|p| p + 1).unwrap_or(0);
                let le = chars[hi.min(n)..].iter().position(|c| *c == '\n').map(|p| p + hi + 1).unwrap_or(n);
                if cur >= a { CCursorRange::two(CCursor::new(CharIndex(ls)), CCursor::new(CharIndex(le))) }
                else { CCursorRange::two(CCursor::new(CharIndex(le)), CCursor::new(CharIndex(ls))) }
            }
            _ => CCursorRange::one(CCursor::new(CharIndex(cur))),
        };
        state.set_undoer(undoer);
        state.cursor.set_char_range(Some(range));
        state.store(&ctx, te_id);
        self.vim_set = Some(range);
        if self.vim_scroll.is_none() { self.vim_scroll = Some('m'); }
    }

    /// Mot sous le curseur (␣sw).
    pub fn word_at_cursor(&self) -> Option<String> {
        let chars: Vec<char> = self.text.chars().collect();
        let mut s = self.vim_cur.min(chars.len());
        let w = |c: char| c.is_alphanumeric() || c == '_';
        if s >= chars.len() || !w(chars[s]) { return None; }
        while s > 0 && w(chars[s - 1]) { s -= 1; }
        let mut e = s;
        while e < chars.len() && w(chars[e]) { e += 1; }
        Some(chars[s..e].iter().collect())
    }

    /// Remplace tout le texte (:e!) en gardant l'annulation possible.
    pub fn replace_all(&mut self, text: &str, ctx: &egui::Context) {
        let id = self.id.with("te");
        let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let mut u = st.undoer();
        u.add_undo(&(CCursorRange::one(CCursor::new(CharIndex(self.vim_cur))), self.text.clone()));
        st.set_undoer(u);
        st.store(ctx, id);
        self.text = text.to_string();
        self.cache = None;
    }

    /// Texte sélectionné (sur une seule ligne), pour pré-remplir la recherche.
    pub fn selected_text(&self, ctx: &egui::Context) -> Option<String> {
        let st = egui::TextEdit::load_state(ctx, self.id.with("te"))?;
        let r = st.cursor.char_range()?.as_sorted_char_range();
        let (s, e) = (char_to_byte(&self.text, r.start.0), char_to_byte(&self.text, r.end.0));
        let t = self.text.get(s..e)?;
        if t.is_empty() || t.contains('\n') { None } else { Some(t.to_string()) }
    }

    /// Exécute une commande du menu Édition / clic droit, en gardant l'historique d'annulation.
    pub fn apply_edit(&mut self, ctx: &egui::Context, te_id: egui::Id, cmd: EditCmd) {
        let mut state = egui::TextEdit::load_state(ctx, te_id).unwrap_or_default();
        let n = self.text.chars().count();
        let focused = ctx.memory(|m| m.has_focus(te_id));
        let range = if focused { state.cursor.char_range() } else { self.last_sel.or(state.cursor.char_range()) }
            .unwrap_or(CCursorRange::one(CCursor::new(CharIndex(n))));
        let sorted = range.as_sorted_char_range();
        let (a, b) = (sorted.start.0.min(n), sorted.end.0.min(n));
        let (s, e) = (char_to_byte(&self.text, a), char_to_byte(&self.text, b));
        let mut undoer = state.undoer();
        let before = (range, self.text.clone());
        let mut set = |text: &mut String, new: &str, at: usize| -> CCursorRange {
            undoer.add_undo(&before);
            text.replace_range(s..e, new);
            let c = at + new.chars().count();
            CCursorRange::one(CCursor::new(CharIndex(c)))
        };
        let new_range = match cmd {
            EditCmd::Undo | EditCmd::Redo => {
                let r = if cmd == EditCmd::Undo { undoer.undo(&before).cloned() } else { undoer.redo(&before).cloned() };
                match r {
                    Some((c, t)) => { self.text = t; c }
                    None => range,
                }
            }
            EditCmd::Copy => { if s < e { ctx.copy_text(self.text[s..e].to_string()); } range }
            EditCmd::Cut => {
                if s < e { ctx.copy_text(self.text[s..e].to_string()); set(&mut self.text, "", a) } else { range }
            }
            EditCmd::Delete => if s < e { set(&mut self.text, "", a) } else { range },
            EditCmd::Paste(t) => set(&mut self.text, &t, a),
            EditCmd::SelectAll => CCursorRange::two(CCursor::new(CharIndex(0)), CCursor::new(CharIndex(n))),
            EditCmd::DeleteLine | EditCmd::MoveLineUp | EditCmd::MoveLineDown => {
                // lignes touchées par la sélection
                let ls = self.text[..s].rfind('\n').map(|x| x + 1).unwrap_or(0);
                let e2 = if e > s && self.text[..e].ends_with('\n') { e - 1 } else { e };
                let le = self.text[e2..].find('\n').map(|x| x + e2).unwrap_or(self.text.len());
                let block = self.text[ls..le].to_string();
                let cpos = |t: &str, b: usize| t[..b].chars().count();
                match cmd {
                    EditCmd::DeleteLine => {
                        undoer.add_undo(&before);
                        let (from, to) = if le < self.text.len() { (ls, le + 1) } else if ls > 0 { (ls - 1, le) } else { (ls, le) };
                        self.text.replace_range(from..to, "");
                        let c = cpos(&self.text, from.min(self.text.len()));
                        let c = if from < ls { c + 1 } else { c }.min(self.text.chars().count());
                        CCursorRange::one(CCursor::new(CharIndex(c)))
                    }
                    EditCmd::MoveLineUp if ls > 0 => {
                        undoer.add_undo(&before);
                        let ps = self.text[..ls - 1].rfind('\n').map(|x| x + 1).unwrap_or(0);
                        let prev = self.text[ps..ls - 1].to_string();
                        self.text.replace_range(ps..le, &format!("{block}\n{prev}"));
                        let d = prev.chars().count() + 1;
                        CCursorRange::two(CCursor::new(CharIndex(a - d)), CCursor::new(CharIndex(b - d)))
                    }
                    EditCmd::MoveLineDown if le < self.text.len() => {
                        undoer.add_undo(&before);
                        let ne = self.text[le + 1..].find('\n').map(|x| x + le + 1).unwrap_or(self.text.len());
                        let next = self.text[le + 1..ne].to_string();
                        self.text.replace_range(ls..ne, &format!("{next}\n{block}"));
                        let d = next.chars().count() + 1;
                        CCursorRange::two(CCursor::new(CharIndex(a + d)), CCursor::new(CharIndex(b + d)))
                    }
                    _ => range,
                }
            }
        };
        state.set_undoer(undoer);
        self.last_sel = Some(new_range);
        state.cursor.set_char_range(Some(new_range));
        state.store(ctx, te_id);
        self.cache = None;
        ctx.memory_mut(|m| m.request_focus(te_id));
    }

    fn handle_keys(&mut self, ui: &mut egui::Ui, te_id: egui::Id, o: &EditorOpts) {
        let ctx = ui.ctx().clone();
        let Some(mut state) = egui::TextEdit::load_state(&ctx, te_id) else { return };
        let Some(range) = state.cursor.char_range() else { return };
        // egui fait correspondre « Tab » aussi à Maj+Tab : on teste donc les touches exactement
        // (sinon Maj+Tab indentait au lieu de désindenter)
        let exact = |i: &mut egui::InputState, m: Modifiers, k: Key| {
            let pos = i.events.iter().position(|e| matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. }
                if *key == k && modifiers.shift == m.shift && modifiers.alt == m.alt && modifiers.command == m.command && modifiers.ctrl == m.ctrl));
            match pos { Some(p) => { i.events.remove(p); true } None => false }
        };
        let untab = ctx.input_mut(|i| exact(i, Modifiers::SHIFT, Key::Tab));
        let tab = ctx.input_mut(|i| exact(i, Modifiers::NONE, Key::Tab));
        let enter = o.auto_indent && ctx.input_mut(|i| exact(i, Modifiers::NONE, Key::Enter) || exact(i, Modifiers::SHIFT, Key::Enter));
        let comment = std::mem::take(&mut self.pending_comment);
        let dup = std::mem::take(&mut self.pending_duplicate);
        // fermeture automatique : ( [ { " ' tapés → la paire, le curseur au milieu ;
        // taper le caractère fermant juste devant lui le saute
        if o.close_brackets && !(enter || tab || untab || comment || dup) {
            let sorted = range.as_sorted_char_range();
            let (s, e) = (char_to_byte(&self.text, sorted.start.0), char_to_byte(&self.text, sorted.end.0));
            let next = self.text[e..].chars().next();
            let prev = self.text[..s].chars().next_back();
            let typed = ctx.input(|i| i.events.iter().find_map(|ev| match ev { egui::Event::Text(t) if t.chars().count() == 1 => Some(t.clone()), _ => None }));
            if let Some(t) = typed {
                let c = t.chars().next().unwrap_or(' ');
                let pair = match c { '(' => Some(')'), '[' => Some(']'), '{' => Some('}'), '"' | '\'' => Some(c), _ => None };
                let word_before = prev.map(|x| x.is_alphanumeric()).unwrap_or(false);
                let mut done = None;
                if s == e && matches!(c, ')' | ']' | '}' | '"' | '\'') && next == Some(c) {
                    done = Some((sorted.start.0 + 1, sorted.start.0 + 1));
                } else if let Some(close) = pair.filter(|_| !(matches!(c, '"' | '\'') && word_before)) {
                    let inner = self.text[s..e].to_string();
                    self.text.replace_range(s..e, &format!("{c}{inner}{close}"));
                    let a = sorted.start.0 + 1;
                    done = Some((a, a + inner.chars().count()));
                }
                if let Some((a, b)) = done {
                    ctx.input_mut(|i| { if let Some(k) = i.events.iter().position(|ev| matches!(ev, egui::Event::Text(_))) { i.events.remove(k); } });
                    state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(CharIndex(a)), CCursor::new(CharIndex(b)))));
                    state.store(&ctx, te_id);
                    self.cache = None;
                    return;
                }
            }
        }
        if !(enter || tab || untab || comment || dup) {
            return;
        }
        let unit = o.indent();
        let sorted = range.as_sorted_char_range();
        let (s, e) = (char_to_byte(&self.text, sorted.start.0), char_to_byte(&self.text, sorted.end.0));
        let line_start = self.text[..s].rfind('\n').map(|x| x + 1).unwrap_or(0);
        let new_cursor: (usize, usize);
        if enter {
            let line = &self.text[line_start..s];
            let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            let trimmed = line.trim_end();
            let opens = trimmed.ends_with('{') || trimmed.ends_with('(') || trimmed.ends_with('[')
                || (trimmed.ends_with(':') && matches!(self.lang, Lang::Python | Lang::Yaml));
            let next = self.text[e..].chars().next();
            let mut ins = format!("\n{indent}");
            let mut after = String::new();
            if opens {
                ins.push_str(&unit);
                if matches!(next, Some('}') | Some(')') | Some(']')) {
                    after = format!("\n{indent}");
                }
            }
            self.text.replace_range(s..e, &format!("{ins}{after}"));
            let c = self.text[..s].chars().count() + ins.chars().count();
            new_cursor = (c, c);
        } else if (tab && s == e) || (tab && !self.text[s..e].contains('\n')) {
            self.text.replace_range(s..e, &unit);
            let c = self.text[..s].chars().count() + unit.chars().count();
            new_cursor = (c, c);
        } else if dup {
            let le = self.text[e..].find('\n').map(|x| x + e).unwrap_or(self.text.len());
            let block = self.text[line_start..le].to_string();
            self.text.insert_str(le, &format!("\n{block}"));
            let shift = block.chars().count() + 1;
            new_cursor = (sorted.start.0 + shift, sorted.end.0 + shift);
        } else {
            // plusieurs lignes : indenter, désindenter ou commenter
            let le = self.text[e..].find('\n').map(|x| x + e).unwrap_or(self.text.len());
            let block = self.text[line_start..le].to_string();
            let (lc, bo, bc) = self.lang.comments();
            let new_block: String = if comment {
                let mark = if lc.is_empty() { bo } else { lc };
                let close = if lc.is_empty() { bc } else { "" };
                let all = block.lines().filter(|l| !l.trim().is_empty())
                    .all(|l| l.trim_start().starts_with(mark));
                block.lines().map(|l| {
                    if l.trim().is_empty() { return l.to_string(); }
                    let ind = l.len() - l.trim_start().len();
                    if all {
                        let rest = &l[ind + mark.len()..];
                        let rest = rest.strip_prefix(' ').unwrap_or(rest);
                        let rest = if close.is_empty() { rest } else { rest.trim_end().strip_suffix(close).unwrap_or(rest).trim_end() };
                        format!("{}{}", &l[..ind], rest)
                    } else if close.is_empty() {
                        format!("{}{} {}", &l[..ind], mark, &l[ind..])
                    } else {
                        format!("{}{} {} {}", &l[..ind], mark, &l[ind..], close)
                    }
                }).collect::<Vec<_>>().join("\n")
            } else if untab {
                block.lines().map(|l| {
                    let n = l.chars().take(o.tab_width.max(1)).take_while(|c| *c == ' ').count();
                    if n == 0 && l.starts_with('\t') { l[1..].to_string() } else { l[n..].to_string() }
                }).collect::<Vec<_>>().join("\n")
            } else {
                block.lines().map(|l| if l.is_empty() { String::new() } else { format!("{unit}{l}") }).collect::<Vec<_>>().join("\n")
            };
            self.text.replace_range(line_start..le, &new_block);
            let a = self.text[..line_start].chars().count();
            new_cursor = (a, a + new_block.chars().count());
        }
        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(CharIndex(new_cursor.0)), CCursor::new(CharIndex(new_cursor.1)))));
        state.store(&ctx, te_id);
        self.cache = None;
    }

    /// Place le curseur au début d'une ligne et l'affiche.
    pub fn goto_line(&mut self, ctx: &egui::Context, line: usize) {
        let te_id = self.id.with("te");
        let byte = self.text.split_inclusive('\n').take(line.saturating_sub(1)).map(|l| l.len()).sum::<usize>();
        let c = self.text[..byte.min(self.text.len())].chars().count();
        let mut st = egui::TextEdit::load_state(ctx, te_id).unwrap_or_default();
        st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(CharIndex(c)))));
        st.store(ctx, te_id);
        self.goto = Some(line);
        ctx.memory_mut(|m| m.request_focus(te_id));
    }

    pub fn has_focus(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.has_focus(self.id.with("te")))
    }

    pub fn focus(&self, ctx: &egui::Context) {
        ctx.memory_mut(|m| m.request_focus(self.id.with("te")));
    }
}

/// Commandes du menu Édition et du clic droit (les raccourcis Ctrl+Z, Ctrl+C… sont gérés par egui).
#[derive(Clone, Debug, PartialEq)]
pub enum EditCmd {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste(String),
    Delete,
    SelectAll,
    DeleteLine,
    MoveLineUp,
    MoveLineDown,
}

/// Réglages de l'éditeur (fenêtre Paramètres).
#[derive(Clone, Copy, PartialEq)]
pub struct EditorOpts {
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
    /// mode Vim
    pub vim: bool,
    /// numéros de ligne relatifs (comme LazyVim)
    pub relative_numbers: bool,
}

impl Default for EditorOpts {
    fn default() -> Self {
        EditorOpts { tab_width: 4, use_spaces: true, line_numbers: true, indent_guides: true, caret_line: true,
                     bracket_match: true, folding: true, word_wrap: false, auto_indent: true, close_brackets: false, vim: false, relative_numbers: false }
    }
}

impl EditorOpts {
    pub fn indent_text(&self) -> String { self.indent() }
    /// Ce qu'insère la touche Tab (espaces ou tabulation).
    fn indent(&self) -> String {
        if self.use_spaces { " ".repeat(self.tab_width.max(1)) } else { "\t".into() }
    }
}

#[derive(Default)]
pub struct EditorEvents {
    /// ce que Vim demande à l'application (leader, :w, changer d'onglet…) et touches tapées après Espace
    pub vim: Vec<crate::vim::Out>,
    pub leader_keys: Vec<crate::vim::K>,
    pub changed: bool,
    pub breakpoint: Option<usize>,
    pub found: bool,
    pub not_found: bool,
    pub matches: usize,
}

/// Recherche sans tenir compte des majuscules, avec des positions (octets) dans le texte d'origine
/// (passer par `to_lowercase()` du texte entier décale les positions pour certains caractères).
pub fn find_all_ci(text: &str, q: &str, limit: usize) -> Vec<(usize, usize)> {
    let ql: Vec<char> = q.chars().flat_map(|c| c.to_lowercase()).collect();
    let mut out = Vec::new();
    if ql.is_empty() { return out; }
    let mut p = 0;
    while p < text.len() && out.len() < limit {
        match match_ci_at(text, p, &ql) {
            Some(e) => { out.push((p, e)); p = e.max(p + 1); }
            None => p += text[p..].chars().next().map(|c| c.len_utf8()).unwrap_or(1),
        }
        while p < text.len() && !text.is_char_boundary(p) { p += 1; }
    }
    out
}

fn match_ci_at(text: &str, p: usize, ql: &[char]) -> Option<usize> {
    let mut k = 0;
    for (i, c) in text[p..].char_indices() {
        for lc in c.to_lowercase() {
            if k >= ql.len() || ql[k] != lc { return None; }
            k += 1;
        }
        if k == ql.len() { return Some(p + i + c.len_utf8()); }
    }
    None
}

/// Occurrence suivante (ou précédente) de `q` à partir de l'octet `from`, en reprenant au début.
pub fn find_ci(text: &str, q: &str, from: usize, back: bool) -> Option<(usize, usize)> {
    let all = find_all_ci(text, q, usize::MAX);
    if back {
        all.iter().rev().find(|(s, _)| *s < from).or(all.last()).copied()
    } else {
        all.iter().find(|(s, _)| *s >= from).or(all.first()).copied()
    }
}

pub fn char_to_byte(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map(|(b, _)| b).unwrap_or(s.len())
}

#[allow(dead_code)]
pub fn stroke(c: Color32) -> Stroke {
    Stroke::new(1.0, c)
}


/// Indentation d'une ligne en colonnes (tabulation = 4), `None` pour une ligne vide.
fn indent_of(line: &str) -> Option<usize> {
    if line.trim().is_empty() { return None; }
    let mut n = 0;
    for c in line.chars() {
        match c { ' ' => n += 1, '\t' => n += 4 - n % 4, _ => break }
    }
    Some(n)
}

/// Parenthèse, crochet ou accolade sous le curseur (ou juste avant) et celle qui lui correspond
/// (positions en octets). La recherche s'arrête à 200 000 caractères.
fn matching_bracket(text: &str, cursor: usize) -> Option<(usize, usize)> {
    let b = text.as_bytes();
    let pick = |i: usize| b.get(i).copied().filter(|c| b"()[]{}".contains(c)).map(|c| (i, c));
    let (at, c) = pick(cursor).or_else(|| cursor.checked_sub(1).and_then(pick))?;
    let (open, close, fwd) = match c {
        b'(' => (b'(', b')', true), b'[' => (b'[', b']', true), b'{' => (b'{', b'}', true),
        b')' => (b'(', b')', false), b']' => (b'[', b']', false), _ => (b'{', b'}', false),
    };
    let mut depth = 0i32;
    if fwd {
        for (k, &x) in b.iter().enumerate().skip(at).take(200_000) {
            if x == open { depth += 1 } else if x == close { depth -= 1; if depth == 0 { return Some((at, k)); } }
        }
    } else {
        let lo = at.saturating_sub(200_000);
        for k in (lo..=at).rev() {
            let x = b[k];
            if x == close { depth += 1 } else if x == open { depth -= 1; if depth == 0 { return Some((k, at)); } }
        }
    }
    None
}


/// Première rangée de chaque ligne du texte dans la mise en page.
fn line_starts(g: &Galley) -> Vec<usize> {
    let mut v = Vec::with_capacity(g.rows.len() + 1);
    v.push(0);
    for (i, r) in g.rows.iter().enumerate() {
        if r.ends_with_newline { v.push(i + 1); }
    }
    v
}

/// Rend invisibles et presque sans hauteur les lignes des blocs repliés.
fn fold_job(job: egui::text::LayoutJob, hidden: &[(usize, usize)]) -> egui::text::LayoutJob {
    let mut out = job.clone();
    out.sections.clear();
    for sec in job.sections {
        let (a, b) = (sec.byte_range.start.0, sec.byte_range.end.0);
        // points de coupe : bornes des zones cachées à l'intérieur de la section
        let mut cuts = vec![a, b];
        for (x, y) in hidden {
            if *x > a && *x < b { cuts.push(*x); }
            if *y > a && *y < b { cuts.push(*y); }
        }
        cuts.sort();
        cuts.dedup();
        for w in cuts.windows(2) {
            let (s, e) = (w[0], w[1]);
            let mut f = sec.format.clone();
            if hidden.iter().any(|(x, y)| s >= *x && e <= *y) {
                f.color = Color32::TRANSPARENT;
                f.line_height = Some(0.01);
                f.font_id.size = 1.0;
            }
            let mut ns = sec.clone();
            ns.leading_space = if s == a { sec.leading_space } else { 0.0 };
            ns.byte_range = egui::text::ByteIndex(s)..egui::text::ByteIndex(e);
            ns.format = f;
            out.sections.push(ns);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_ci_keeps_offsets() {
        // « İ » devient plus long en minuscules : les positions doivent rester celles du texte
        let t = "İstanbul STRASSE straße Straße";
        let hits = find_all_ci(t, "straße", 10);
        assert_eq!(hits.len(), 2);
        for (s, e) in &hits { assert_eq!(t[*s..*e].to_lowercase(), "straße"); }
        assert_eq!(find_all_ci("aaa", "aa", 10), vec![(0, 2)]);
        assert_eq!(find_ci("Ab ab AB", "ab", 1, false), Some((3, 5)));
        assert_eq!(find_ci("Ab ab AB", "ab", 3, true), Some((0, 2)));
        assert_eq!(find_ci("Ab ab AB", "ab", 0, true), Some((6, 8)));
    }

    fn run(text: &str, cur: usize, cmd: EditCmd) -> String {
        let ctx = egui::Context::default();
        let mut d = Doc::untitled(1, text);
        let id = d.id.with("te");
        let mut st = egui::text_edit::TextEditState::default();
        st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(CharIndex(cur)))));
        st.store(&ctx, id);
        d.apply_edit(&ctx, id, cmd);
        d.text
    }

    #[test]
    fn line_commands() {
        assert_eq!(run("a\nb\nc", 2, EditCmd::MoveLineUp), "b\na\nc");
        assert_eq!(run("a\nb\nc", 2, EditCmd::MoveLineDown), "a\nc\nb");
        assert_eq!(run("a\nb\nc", 0, EditCmd::MoveLineUp), "a\nb\nc");
        assert_eq!(run("a\nb\nc", 2, EditCmd::DeleteLine), "a\nc");
        assert_eq!(run("a\nb\nc", 4, EditCmd::DeleteLine), "a\nb");
        assert_eq!(run("été", 3, EditCmd::Paste("ô".into())), "étéô");
        let ctx = egui::Context::default();
        let mut d = Doc::untitled(1, "abc");
        let id = d.id.with("te");
        d.apply_edit(&ctx, id, EditCmd::SelectAll);
        d.apply_edit(&ctx, id, EditCmd::Delete);
        assert_eq!(d.text, "");
        d.apply_edit(&ctx, id, EditCmd::Undo);
        assert_eq!(d.text, "abc");
        d.apply_edit(&ctx, id, EditCmd::Redo);
        assert_eq!(d.text, "");
    }

    #[test]
    fn trim_trailing_spaces() {
        let mut d = Doc::untitled(1, "a  \nb\t\n  c ");
        d.trim_trailing();
        assert_eq!(d.text, "a\nb\n  c");
    }
}

/// Touche spéciale (ou avec Ctrl / Alt) → touche Vim. Les lettres simples arrivent par l'événement Text.
fn vim_key(key: Key, ctrl: bool, alt: bool, shift: bool) -> Option<crate::vim::K> {
    use crate::vim::K;
    let letter = |k: Key| -> Option<char> {
        let n = k.name();
        let c = n.chars().next()?;
        if n.chars().count() == 1 && c.is_ascii_alphabetic() { Some(c.to_ascii_lowercase()) } else { None }
    };
    if ctrl && !alt {
        if key == Key::Slash || key == Key::Colon && shift { return Some(K::Ctrl('/')); }
        if key == Key::OpenBracket { return Some(K::Esc); }
        return match letter(key) { Some(c @ ('r' | 'd' | 'u' | 'h' | 'j' | 'k' | 'l')) if !shift => Some(K::Ctrl(c)), _ => None };
    }
    if alt && !ctrl {
        return match letter(key) { Some(c @ ('j' | 'k')) => Some(K::Alt(c)), _ => None };
    }
    if ctrl || alt { return None; }
    Some(match key {
        Key::Escape => K::Esc,
        Key::Enter => K::Enter,
        Key::Backspace => K::Back,
        Key::Tab => K::Tab,
        Key::Delete => K::Del,
        Key::ArrowUp => K::Up,
        Key::ArrowDown => K::Down,
        Key::ArrowLeft => K::Left,
        Key::ArrowRight => K::Right,
        Key::Home => K::Home,
        Key::End => K::End,
        _ => return None,
    })
}
