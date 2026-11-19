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
            info: String::new(), last_width: 0.0,
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
        // barre d'outils
        egui::Frame::new().fill(p.bg_dark).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
            egui::ScrollArea::horizontal().id_salt(("pdfbar", &self.path))
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{} / {}", self.page + 1, n)).color(p.comment));
                if icons::small(ui, Icon::ChevronLeft, p.fg, p.bg_hl, "Page précédente (Page préc.)").clicked() && self.page > 0 { self.goto = Some(self.page - 1); }
                if icons::small(ui, Icon::ChevronRight, p.fg, p.bg_hl, "Page suivante (Page suiv.)").clicked() && self.page + 1 < n { self.goto = Some(self.page + 1); }
                ui.separator();
                if ui.small_button("−").on_hover_text("Réduire (Ctrl+molette)").clicked() { self.fit = Fit::Free; self.zoom = (self.zoom / 1.2).max(0.2); }
                ui.label(egui::RichText::new(format!("{:.0} %", self.zoom * 100.0)).color(p.comment));
                if ui.small_button("+").on_hover_text("Agrandir (Ctrl+molette)").clicked() { self.fit = Fit::Free; self.zoom = (self.zoom * 1.2).min(6.0); }
                if ui.selectable_label(self.fit == Fit::Width, "Largeur").clicked() { self.fit = Fit::Width; }
                if ui.selectable_label(self.fit == Fit::Page, "Page entière").clicked() { self.fit = Fit::Page; }
                if ui.selectable_label(self.dark, "Sombre").clicked() { self.dark = !self.dark; }
                ui.separator();
                let r = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Rechercher dans le PDF").desired_width(170.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if self.hits.is_empty() { self.search(); } else { self.next_hit(1); }
                    r.request_focus();
                }
                if !self.hits.is_empty() {
                    if icons::small(ui, Icon::ChevronUp, p.fg, p.bg_hl, "Résultat précédent").clicked() { self.next_hit(-1); }
                    if icons::small(ui, Icon::ChevronDown, p.fg, p.bg_hl, "Résultat suivant").clicked() { self.next_hit(1); }
                }
                if r.changed() { self.hits.clear(); self.info.clear(); }
                ui.label(egui::RichText::new(&self.info).color(p.comment).size(12.0));
                ui.separator();
                if ui.small_button("Copier le texte").on_hover_text("Copie le texte de la page affichée").clicked() {
                    ctx.copy_text(self.page_text(self.page));
                }
                if icons::small(ui, Icon::Folder, p.comment, p.bg_hl, "Ouvrir avec l'application par défaut").clicked() {
                    let _ = open::that_detached(&self.path);
                }
            });
            });
        });
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
        // clavier : pages
        if ui.ui_contains_pointer() {
            if ui.input(|i| i.key_pressed(egui::Key::PageDown)) && self.page + 1 < n { self.goto = Some(self.page + 1); }
            if ui.input(|i| i.key_pressed(egui::Key::PageUp)) && self.page > 0 { self.goto = Some(self.page - 1); }
        }
    }

    fn next_hit(&mut self, d: i32) {
        if self.hits.is_empty() { return; }
        let n = self.hits.len() as i32;
        self.hit = ((self.hit as i32 + d).rem_euclid(n)) as usize;
        self.goto = Some(self.hits[self.hit].0);
        self.info = format!("{} / {}", self.hit + 1, n);
    }
}
