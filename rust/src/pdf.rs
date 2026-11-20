//! Module Lecteur PDF (facultatif) : pdfium, chargé seulement à l'ouverture du premier PDF.
//! Pages rendues à la demande (seules les pages visibles), zoom, ajustement à la largeur ou
//! à la page, mode sombre, recherche surlignée, liens, page retenue pour chaque fichier.
use crate::icons::{self, Icon};
use crate::theme::Palette;
use eframe::egui::{self, Color32, Rect, Sense, Vec2};
use pdfium_render::prelude::*;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

/// Dossier du module PDF (bibliothèque pdfium) s'il est installé.
pub fn module_lib() -> Option<PathBuf> {
    let name = Pdfium::pdfium_platform_library_name();
    let mut dirs = Vec::new();
    if let Ok(d) = std::env::var("STUDENTCODE_MODULES") {
        dirs.push(PathBuf::from(d).join("pdf"));
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..4 {
            let Some(d) = dir else { break };
            dirs.push(d.join("modules").join("pdf"));
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    dirs.into_iter().map(|d| d.join(&name)).find(|p| p.is_file())
}

/// Vérification (autotest) : la bibliothèque se charge.
pub fn check() -> Result<(), String> {
    pdfium().map(|_| ())
}

fn pdfium() -> Result<&'static Pdfium, String> {
    PDFIUM.get_or_init(|| {
        let lib = module_lib().ok_or("le module Lecteur PDF n'est pas installé")?;
        let b = Pdfium::bind_to_library(&lib).map_err(|e| format!("pdfium : {e}"))?;
        Ok(Pdfium::new(b))
    }).as_ref().map_err(|e| e.clone())
}

#[derive(PartialEq, Clone, Copy)]
enum Fit { Width, Page, Free }

pub struct PdfView {
    pub path: PathBuf,
    doc: PdfDocument<'static>,
    sizes: Vec<(f32, f32)>,
    zoom: f32,
    fit: Fit,
    pub dark: bool,
    cache: HashMap<(usize, u32, bool), egui::TextureHandle>,
    order: VecDeque<(usize, u32, bool)>,
    page: usize,
    goto: Option<usize>,
    query: String,
    hits: Vec<(usize, Vec<Rect>)>,
    hit: usize,
    info: String,
    last_width: f32,
    /// barre flottante : visible jusqu'à cet instant (elle se cache après 2,2 s, comme la version Python)
    bar_until: f64,
    page_in: String,
    pub search_open: bool,
}

fn positions_file() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "StudentCode").map(|d| d.config_dir().join("pdf_positions.json"))
}

fn positions() -> BTreeMap<String, (usize, f32)> {
    positions_file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

impl PdfView {
    pub fn open(path: &Path) -> Result<PdfView, String> {
        let p = pdfium()?;
        let doc = p.load_pdf_from_file(path, None).map_err(|e| match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => "PDF protégé par un mot de passe".to_string(),
            e => format!("PDF illisible : {e}"),
        })?;
        let sizes = doc.pages().iter().map(|pg| (pg.width().value, pg.height().value)).collect::<Vec<_>>();
        if sizes.is_empty() {
            return Err("PDF sans page".into());
        }
        let (page, zoom) = positions().get(&path.to_string_lossy().to_string()).copied().unwrap_or((0, 0.0));
        Ok(PdfView {
            path: path.to_path_buf(), doc, sizes, zoom: if zoom > 0.0 { zoom } else { 1.0 },
            fit: if zoom > 0.0 { Fit::Free } else { Fit::Width }, dark: false, cache: HashMap::new(),
            order: VecDeque::new(), page, goto: Some(page), query: String::new(), hits: Vec::new(), hit: 0,
            info: String::new(), bar_until: 0.0, page_in: String::new(), search_open: false, last_width: 0.0,
        })
    }

    pub fn save_position(&self) {
        let Some(f) = positions_file() else { return };
        let mut m = positions();
        m.insert(self.path.to_string_lossy().to_string(), (self.page, if self.fit == Fit::Free { self.zoom } else { 0.0 }));
        if m.len() > 300 {
            let k = m.keys().next().cloned().unwrap();
            m.remove(&k);
        }
        if let Some(d) = f.parent() { let _ = std::fs::create_dir_all(d); }
        if let Ok(j) = serde_json::to_string(&m) { let _ = std::fs::write(f, j); }
    }

    pub fn title(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    }

    fn texture(&mut self, ctx: &egui::Context, i: usize, ppp: f32) -> Option<egui::TextureHandle> {
        let scale = self.zoom * ppp;
        let key = (i, (scale * 100.0) as u32, self.dark);
        if let Some(t) = self.cache.get(&key) {
            return Some(t.clone());
        }
        let page = self.doc.pages().get(i as PdfPageIndex).ok()?;
        let w = (self.sizes[i].0 * scale).round().max(1.0) as i32;
        let cfg = PdfRenderConfig::new().set_target_width(w).render_form_data(true);
        let bmp = page.render_with_config(&cfg).ok()?;
        let (bw, bh) = (bmp.width() as usize, bmp.height() as usize);
        let mut rgba = bmp.as_rgba_bytes();
        if self.dark {
            // mode sombre : couleurs inversées (texte clair sur fond sombre)
            for px in rgba.chunks_exact_mut(4) {
                px[0] = 255 - px[0];
                px[1] = 255 - px[1];
                px[2] = 255 - px[2];
            }
        }
        let img = egui::ColorImage::from_rgba_unmultiplied([bw, bh], &rgba);
        let tex = ctx.load_texture(format!("pdf{}-{i}-{}", self.path.display(), key.1), img, egui::TextureOptions::LINEAR);
        self.cache.insert(key, tex.clone());
        self.order.push_back(key);
        while self.order.len() > 10 {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
            }
        }
        Some(tex)
    }

    fn search(&mut self) {
        self.hits.clear();
        self.hit = 0;
        let q = self.query.trim().to_string();
        if q.is_empty() { self.info.clear(); return; }
        let opts = PdfSearchOptions::new();
        for (i, page) in self.doc.pages().iter().enumerate() {
            let Ok(text) = page.text() else { continue };
            let Ok(s) = text.search(&q, &opts) else { continue };
            let h = page.height().value;
            while let Some(segs) = s.find_next() {
                let rects: Vec<Rect> = segs.iter().map(|seg| {
                    let b = seg.bounds();
                    Rect::from_min_max(egui::pos2(b.left().value, h - b.top().value), egui::pos2(b.right().value, h - b.bottom().value))
                }).collect();
                self.hits.push((i, rects));
                if self.hits.len() > 5000 { break; }
            }
        }
        self.info = if self.hits.is_empty() { "aucun résultat".into() } else { format!("1 / {}", self.hits.len()) };
        if let Some((p, _)) = self.hits.first() { self.goto = Some(*p); }
    }

    fn page_text(&self, i: usize) -> String {
        self.doc.pages().get(i as PdfPageIndex).ok().and_then(|p| p.text().ok().map(|t| t.all())).unwrap_or_default()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        let n = self.sizes.len();
        let area = ui.available_rect_before_wrap();
        let avail = ui.available_size();
        if (avail.x - self.last_width).abs() > 1.0 { self.last_width = avail.x; }
        let gap = 12.0;
        let maxw = self.sizes.iter().map(|s| s.0).fold(0.0, f32::max);
        match self.fit {
            Fit::Width => self.zoom = ((avail.x - 40.0) / maxw).clamp(0.2, 6.0),
            Fit::Page => {
                let (w, h) = self.sizes[self.page.min(n - 1)];
                self.zoom = ((avail.x - 40.0) / w).min((avail.y - 2.0 * gap) / h).clamp(0.1, 6.0);
            }
            Fit::Free => {}
        }
        // zoom à la molette avec Ctrl
        if ui.rect_contains_pointer(ui.max_rect()) {
            let z = ui.input(|i| i.zoom_delta());
            if (z - 1.0).abs() > 0.001 {
                self.fit = Fit::Free;
                self.zoom = (self.zoom * z).clamp(0.2, 6.0);
            }
        }
        let tops: Vec<f32> = self.sizes.iter().scan(gap, |y, s| { let t = *y; *y += s.1 * self.zoom + gap; Some(t) }).collect();
        let total_h = tops.last().copied().unwrap_or(0.0) + self.sizes.last().map(|s| s.1 * self.zoom).unwrap_or(0.0) + gap;
        let total_w = (maxw * self.zoom + 40.0).max(avail.x);
        let mut sa = egui::ScrollArea::both().id_salt(("pdf", &self.path)).auto_shrink(false);
        if let Some(g) = self.goto.take() {
            sa = sa.vertical_scroll_offset(tops[g.min(n - 1)] - gap);
            self.page = g.min(n - 1);
        }
        let ppp = ctx.pixels_per_point();
        let mut clicked_link: Option<(usize, f32, f32)> = None;
        let out = sa.show_viewport(ui, |ui, view| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(total_w, total_h), Sense::click());
            let origin = rect.min;
            let mut center_page = self.page;
            let mid = view.center().y;
            for i in 0..n {
                let (w, h) = (self.sizes[i].0 * self.zoom, self.sizes[i].1 * self.zoom);
                let top = tops[i];
                if top <= mid && mid <= top + h + gap { center_page = i; }
                if top + h < view.min.y - 200.0 || top > view.max.y + 200.0 { continue; }
                let x = origin.x + (total_w - w) / 2.0;
                let pr = Rect::from_min_size(egui::pos2(x, origin.y + top), Vec2::new(w, h));
                ui.painter().rect_filled(pr.translate(Vec2::new(2.0, 3.0)), 2.0, Color32::from_black_alpha(60));
                match self.texture(&ui.ctx().clone(), i, ppp) {
                    Some(t) => { ui.painter().image(t.id(), pr, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE); }
                    None => { ui.painter().rect_filled(pr, 0.0, if self.dark { p.bg_hl } else { Color32::WHITE }); }
                }
                for (k, (hp, rects)) in self.hits.iter().enumerate() {
                    if *hp != i { continue; }
                    let col = if k == self.hit { p.orange.gamma_multiply(0.55) } else { p.yellow.gamma_multiply(0.35) };
                    for r in rects {
                        let rr = Rect::from_min_max(pr.min + r.min.to_vec2() * self.zoom, pr.min + r.max.to_vec2() * self.zoom);
                        ui.painter().rect_filled(rr, 1.0, col);
                    }
                }
                if resp.clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if pr.contains(pos) {
                            let px = (pos.x - pr.min.x) / self.zoom;
                            let py = self.sizes[i].1 - (pos.y - pr.min.y) / self.zoom;
                            clicked_link = Some((i, px, py));
                        }
                    }
                }
            }
            if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::Default); }
            center_page
        });
        self.page = out.inner;
        // liens : adresse web ou page du document
        if let Some((i, x, y)) = clicked_link {
            if let Ok(page) = self.doc.pages().get(i as PdfPageIndex) {
                if let Some(link) = page.links().link_at_point(PdfPoints::new(x), PdfPoints::new(y)) {
                    match link.action() {
                        Some(PdfAction::Uri(u)) => { if let Ok(url) = u.uri() { let _ = open::that_detached(url); } }
                        Some(PdfAction::LocalDestination(d)) => {
                            if let Ok(pi) = d.destination().and_then(|d| d.page_index()) { self.goto = Some(pi as usize); }
                        }
                        _ => {
                            if let Some(pi) = link.destination().and_then(|d| d.page_index().ok()) { self.goto = Some(pi as usize); }
                        }
                    }
                }
            }
        }
        self.floating_bar(ui, p, area, n);
        // clavier : pages
        if ui.ui_contains_pointer() {
            if ui.input(|i| i.key_pressed(egui::Key::PageDown)) && self.page + 1 < n { self.goto = Some(self.page + 1); }
            if ui.input(|i| i.key_pressed(egui::Key::PageUp)) && self.page > 0 { self.goto = Some(self.page - 1); }
        }
    }

    /// Barre flottante en haut à droite, comme dans la version Python : page, zoom, largeur,
    /// page entière, mode sombre, recherche, ouvrir avec le système.
    fn floating_bar(&mut self, ui: &mut egui::Ui, p: &Palette, area: Rect, n: usize) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let moved = ctx.input(|i| i.pointer.delta() != Vec2::ZERO || i.smooth_scroll_delta != Vec2::ZERO);
        if (moved && ui.rect_contains_pointer(area)) || self.bar_until == 0.0 { self.bar_until = now + 2.2; }
        let busy = self.search_open && !self.query.is_empty();
        let bar_id = egui::Id::new(("pdf_bar", &self.path));
        let hovered = ctx.memory(|m| m.area_rect(bar_id)).map(|r| ui.rect_contains_pointer(r)).unwrap_or(false);
        let focus_in = ctx.memory(|m| m.focused()).map(|f| f == egui::Id::new(("pdf_page", &self.path)) || f == egui::Id::new(("pdf_find", &self.path))).unwrap_or(false);
        if hovered || busy || focus_in { self.bar_until = self.bar_until.max(now + 0.5); }
        if now > self.bar_until { return; }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((self.bar_until - now).max(0.05)));
        if !focus_in { self.page_in = (self.page + 1).to_string(); }
        egui::Area::new(bar_id).order(egui::Order::Foreground).pivot(egui::Align2::RIGHT_TOP)
            .fixed_pos(egui::pos2(area.max.x - 10.0, area.min.y + 8.0)).show(&ctx, |ui| {
            egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.bg_hl)).corner_radius(8)
                .inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let tool = |ui: &mut egui::Ui, ic: Icon, c: Color32, tip: &str, on: bool| {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(24.0, 24.0), egui::Sense::click());
                        if on || resp.hovered() { ui.painter().rect_filled(r, 5.0, p.bg_hl); }
                        icons::paint(ui.painter(), ic, r.center(), 16.0, c);
                        resp.on_hover_text(tip)
                    };
                    let field = |ui: &mut egui::Ui, text: &mut String, w: f32, id: egui::Id, hint: &str| -> egui::Response {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 22.0), egui::Sense::hover());
                        let foc = ui.memory(|m| m.has_focus(id));
                        ui.painter().rect_filled(r, 4.0, p.bg);
                        ui.painter().rect_stroke(r, 4.0, egui::Stroke::new(1.0, if foc { p.blue } else { p.gutter }), egui::StrokeKind::Inside);
                        let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(Vec2::new(5.0, 2.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                        cu.add(egui::TextEdit::singleline(text).id(id).frame(egui::Frame::NONE).hint_text(hint)
                            .horizontal_align(if w < 60.0 { egui::Align::Center } else { egui::Align::Min }).desired_width(r.width() - 10.0))
                    };
                    let pid = egui::Id::new(("pdf_page", &self.path));
                    let r = field(ui, &mut self.page_in, 38.0, pid, "");
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Ok(k) = self.page_in.trim().parse::<usize>() { self.goto = Some(k.clamp(1, n.max(1)) - 1); }
                    }
                    ui.label(egui::RichText::new(format!("/ {n}")).color(p.comment));
                    ui.add_space(6.0);
                    if tool(ui, Icon::Min, p.fg, "Zoom arrière (Ctrl+−)", false).clicked() { self.fit = Fit::Free; self.zoom = (self.zoom / 1.2).max(0.2); }
                    let (zr, _) = ui.allocate_exact_size(Vec2::new(46.0, 24.0), egui::Sense::hover());
                    ui.painter().text(zr.center(), egui::Align2::CENTER_CENTER, format!("{:.0} %", self.zoom * 100.0), egui::FontId::proportional(13.3), p.comment);
                    if tool(ui, Icon::NewPlus, p.fg, "Zoom avant (Ctrl++)", false).clicked() { self.fit = Fit::Free; self.zoom = (self.zoom * 1.2).min(6.0); }
                    if tool(ui, Icon::FitWidth, p.blue, "Ajuster à la largeur (Ctrl+0)", self.fit == Fit::Width).clicked() { self.fit = Fit::Width; }
                    if tool(ui, Icon::FitPage, p.blue, "Page entière (une page à la fois)", self.fit == Fit::Page).clicked() {
                        self.fit = if self.fit == Fit::Page { Fit::Width } else { Fit::Page };
                    }
                    if tool(ui, Icon::Moon, p.yellow, "Mode sombre pour le PDF", self.dark).clicked() { self.dark = !self.dark; self.cache.clear(); self.order.clear(); }
                    if self.search_open {
                        let fid = egui::Id::new(("pdf_find", &self.path));
                        let r = field(ui, &mut self.query, 150.0, fid, "Rechercher…");
                        if r.changed() { self.hits.clear(); self.info.clear(); }
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let back = ui.input(|i| i.modifiers.shift);
                            if self.hits.is_empty() { self.search(); } else { self.next_hit(if back { -1 } else { 1 }); }
                            r.request_focus();
                        }
                        if !self.info.is_empty() { ui.label(egui::RichText::new(&self.info).color(p.comment)); }
                    }
                    if tool(ui, Icon::Search, p.fg, "Rechercher (Ctrl+F)", self.search_open).clicked() {
                        self.search_open = !self.search_open;
                        if self.search_open { ctx.memory_mut(|m| m.request_focus(egui::Id::new(("pdf_find", &self.path)))); }
                    }
                    if tool(ui, Icon::External, p.comment, "Ouvrir dans l'application PDF du système", false).clicked() {
                        let _ = open::that_detached(&self.path);
                    }
                });
            });
        });
    }

    /// Ctrl+F sur un PDF : la recherche de la barre flottante.
    pub fn open_search(&mut self, ctx: &egui::Context) {
        self.search_open = true;
        self.bar_until = ctx.input(|i| i.time) + 2.2;
        ctx.memory_mut(|m| m.request_focus(egui::Id::new(("pdf_find", &self.path))));
    }

    fn next_hit(&mut self, d: i32) {
        if self.hits.is_empty() { return; }
        let n = self.hits.len() as i32;
        self.hit = ((self.hit as i32 + d).rem_euclid(n)) as usize;
        self.goto = Some(self.hits[self.hit].0);
        self.info = format!("{} / {}", self.hit + 1, n);
    }
}
