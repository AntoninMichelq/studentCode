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
    cache: Option<(u64, String, f32, Arc<Galley>)>,
    pub breakpoints: BTreeSet<usize>, // lignes (à partir de 1)
    pub goto: Option<usize>,          // ligne à afficher au prochain dessin
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub find: Option<String>,
    pub find_next: bool,
    /// fichier d'une machine distante : (clé du lab, chemin sur la machine, nom du lab)
    pub remote: Option<(String, String, String)>,
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
            id: egui::Id::new(("doc", n, std::time::SystemTime::now())), cache: None,
            breakpoints: BTreeSet::new(), goto: None, cursor_line: 1, cursor_col: 1, find: None, find_next: false,
            remote: None,
        }
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
            .unwrap_or_else(|| "sans titre".into())
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

    pub fn ui(&mut self, ui: &mut egui::Ui, p: &Palette, font: &FontId, debug_line: Option<usize>,
              error_line: Option<usize>, show_breakpoints: bool) -> EditorEvents {
        let mut events = EditorEvents::default();
        let (cw, row_h) = ui.fonts_mut(|f| (f.glyph_width(font, '0'), f.row_height(font)));
        let lines = self.text.matches('\n').count() + 1;
        let digits = lines.to_string().len().max(3) as f32;
        let gutter_w = digits * cw + 26.0;
        let te_id = self.id.with("te");
        let focused = ui.memory(|m| m.has_focus(te_id));
        if focused {
            self.handle_keys(ui, te_id);
        }
        // recherche : sélectionner l'occurrence suivante
        let mut scroll_to: Option<usize> = self.goto.take();
        if self.find_next {
            self.find_next = false;
            if let Some(q) = self.find.clone().filter(|q| !q.is_empty()) {
                let state = egui::TextEdit::load_state(ui.ctx(), te_id);
                let from = state.as_ref().and_then(|s| s.cursor.char_range()).map(|r| r.as_sorted_char_range().end.0).unwrap_or(0);
                let byte_from = char_to_byte(&self.text, from);
                let lower = self.text.to_lowercase();
                let ql = q.to_lowercase();
                let hit = lower.get(byte_from..).and_then(|s| s.find(&ql)).map(|x| x + byte_from)
                    .or_else(|| lower.find(&ql));
                if let Some(b) = hit {
                    let s = self.text[..b].chars().count();
                    let e = s + q.chars().count();
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
        let full = ui.available_rect_before_wrap();
        let gutter_rect = Rect::from_min_size(full.min, Vec2::new(gutter_w, full.height()));
        let text_rect = Rect::from_min_max(egui::pos2(full.min.x + gutter_w, full.min.y), full.max);
        ui.painter().rect_filled(full, 0.0, p.bg);
        let gutter_resp = ui.interact(gutter_rect, self.id.with("gutter"), Sense::click());
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        let mut sa = egui::ScrollArea::both().id_salt(self.id.with("scroll")).auto_shrink(false);
        if let Some(line) = scroll_to {
            let y = self.cache.as_ref().and_then(|c| c.3.rows.get(line.saturating_sub(1))).map(|r| r.pos.y)
                .unwrap_or((line.saturating_sub(1)) as f32 * row_h);
            sa = sa.vertical_scroll_offset((y - text_rect.height() / 3.0).max(0.0));
        }
        let lang = self.lang;
        let pal = *p;
        let font_c = font.clone();
        let cache = &mut self.cache;
        let theme_key = format!("{:?}{:?}", p.fg, p.bg);
        let bg_slot_holder: std::cell::Cell<Option<egui::layers::ShapeIdx>> = std::cell::Cell::new(None);
        let out = sa.show(&mut child, |ui| {
            bg_slot_holder.set(Some(ui.painter().add(Shape::Noop)));
            let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap: f32| -> Arc<Galley> {
                let text = buf.as_str();
                let h = hash(text);
                if let Some((ch, key, size, g)) = cache.as_ref() {
                    if *ch == h && *key == theme_key && *size == font_c.size {
                        return g.clone();
                    }
                }
                let mut job = highlight::highlight(text, lang, &pal, &font_c);
                job.wrap.max_width = f32::INFINITY;
                let g = ui.fonts_mut(|f| f.layout_job(job));
                *cache = Some((h, theme_key.clone(), font_c.size, g.clone()));
                g
            };
            TextEdit::multiline(&mut self.text)
                .id(te_id)
                .font(font.clone())
                .code_editor()
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::symmetric(6, 2))
                .desired_width(f32::INFINITY)
                .desired_rows((text_rect.height() / row_h) as usize)
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(ui)
        });
        let te = out.inner;
        let offset = out.state.clamped_offset();
        let galley_pos = te.galley_pos;
        // ligne courante, ligne du débogueur, ligne en erreur, occurrences de la recherche
        let mut bg_shapes = Vec::new();
        // position réelle de chaque ligne dans le texte mis en page
        let galley = te.galley.clone();
        let row_y = |line: usize| -> (f32, f32) {
            match galley.rows.get(line.saturating_sub(1)) {
                Some(r) => (galley_pos.y + r.pos.y, r.rect().height().max(1.0)),
                None => (galley_pos.y + (line.saturating_sub(1)) as f32 * row_h, row_h),
            }
        };
        let line_rect = |line: usize| -> Rect {
            let (y, h) = row_y(line);
            Rect::from_min_size(egui::pos2(text_rect.min.x, y), Vec2::new(text_rect.width().max(4000.0), h))
        };
        if let Some(r) = te.cursor_range {
            let c = r.primary.index.0;
            let byte = char_to_byte(&self.text, c);
            self.cursor_line = self.text[..byte].matches('\n').count() + 1;
            self.cursor_col = self.text[..byte].rsplit('\n').next().map(|s| s.chars().count()).unwrap_or(0) + 1;
            if te.response.response.has_focus() && r.is_empty() {
                bg_shapes.push(Shape::rect_filled(line_rect(self.cursor_line), 0.0, p.curline));
            }
        }
        if let Some(l) = error_line {
            bg_shapes.push(Shape::rect_filled(line_rect(l), 0.0, p.err_bg));
        }
        if let Some(l) = debug_line {
            bg_shapes.push(Shape::rect_filled(line_rect(l), 0.0, p.yellow.gamma_multiply(0.22)));
        }
        if let Some(q) = self.find.as_ref().filter(|q| q.chars().count() >= 2) {
            let lower = self.text.to_lowercase();
            let ql = q.to_lowercase();
            let mut count = 0;
            for (b, _) in lower.match_indices(&ql) {
                count += 1;
                if count > 2000 { break; }
                let ci = self.text[..b.min(self.text.len())].chars().count();
                let r0 = te.galley.pos_from_cursor(CCursor::new(CharIndex(ci)));
                let r1 = te.galley.pos_from_cursor(CCursor::new(CharIndex(ci + q.chars().count())));
                let rr = Rect::from_min_max(galley_pos + r0.min.to_vec2(), galley_pos + egui::vec2(r1.max.x, r0.max.y));
                bg_shapes.push(Shape::rect_filled(rr, 2.0, p.orange.gamma_multiply(0.30)));
            }
            events.matches = count;
        }
        if let Some(slot) = bg_slot_holder.get() {
            child.painter().set(slot, Shape::Vec(bg_shapes));
        }
        // gouttière : numéros de ligne et points d'arrêt
        let painter = ui.painter_at(gutter_rect);
        painter.rect_filled(gutter_rect, 0.0, p.bg);
        let real_h = if galley.rows.len() > 1 { galley.rect.height() / galley.rows.len() as f32 } else { row_h };
        let first = (offset.y / real_h).floor().max(0.0) as usize;
        let visible = (gutter_rect.height() / real_h).ceil() as usize + 2;
        for i in first..(first + visible).min(lines) {
            let line = i + 1;
            let (y, row_h) = row_y(line);
            if y > gutter_rect.max.y { break; }
            let current = line == self.cursor_line;
            let col = if Some(line) == debug_line { p.yellow } else if current { p.fg } else { p.gutter };
            painter.text(egui::pos2(gutter_rect.max.x - 12.0, y + row_h / 2.0), egui::Align2::RIGHT_CENTER,
                         line.to_string(), font.clone(), col);
            if self.breakpoints.contains(&line) {
                painter.circle_filled(egui::pos2(gutter_rect.min.x + 9.0, y + row_h / 2.0), row_h * 0.28, p.red);
            }
            if Some(line) == debug_line {
                crate::icons::paint(&painter, crate::icons::Icon::Run, egui::pos2(gutter_rect.min.x + 9.0, y + row_h / 2.0),
                                    row_h * 0.7, p.yellow);
            }
        }
        if show_breakpoints && gutter_resp.clicked() {
            if let Some(pos) = gutter_resp.interact_pointer_pos() {
                let line = ((pos.y - galley_pos.y) / real_h).floor() as usize + 1;
                if line <= lines {
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
        ui.allocate_rect(full, Sense::hover());
        events
    }

    fn handle_keys(&mut self, ui: &mut egui::Ui, te_id: egui::Id) {
        let ctx = ui.ctx().clone();
        let Some(mut state) = egui::TextEdit::load_state(&ctx, te_id) else { return };
        let Some(range) = state.cursor.char_range() else { return };
        let enter = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        let tab = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
        let untab = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab));
        let comment = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Slash)
            || i.consume_key(Modifiers::COMMAND, Key::Colon));
        let dup = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::D));
        if !(enter || tab || untab || comment || dup) {
            return;
        }
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
                ins.push_str("    ");
                if matches!(next, Some('}') | Some(')') | Some(']')) {
                    after = format!("\n{indent}");
                }
            }
            self.text.replace_range(s..e, &format!("{ins}{after}"));
            let c = self.text[..s].chars().count() + ins.chars().count();
            new_cursor = (c, c);
        } else if (tab && s == e) || (tab && !self.text[s..e].contains('\n')) {
            self.text.replace_range(s..e, "    ");
            let c = self.text[..s].chars().count() + 4;
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
                    let n = l.chars().take(4).take_while(|c| *c == ' ').count();
                    if n == 0 && l.starts_with('\t') { l[1..].to_string() } else { l[n..].to_string() }
                }).collect::<Vec<_>>().join("\n")
            } else {
                block.lines().map(|l| if l.is_empty() { String::new() } else { format!("    {l}") }).collect::<Vec<_>>().join("\n")
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

    pub fn focus(&self, ctx: &egui::Context) {
        ctx.memory_mut(|m| m.request_focus(self.id.with("te")));
    }
}

#[derive(Default)]
pub struct EditorEvents {
    pub changed: bool,
    pub breakpoint: Option<usize>,
    pub found: bool,
    pub not_found: bool,
    pub matches: usize,
}

pub fn char_to_byte(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map(|(b, _)| b).unwrap_or(s.len())
}

#[allow(dead_code)]
pub fn stroke(c: Color32) -> Stroke {
    Stroke::new(1.0, c)
}
