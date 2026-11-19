//! Affichage d'une image (png, jpg, gif, bmp, webp, ico, svg simple non géré).
use crate::theme::Palette;
use eframe::egui::{self, Rect, Sense, Vec2};
use std::path::{Path, PathBuf};

pub struct ImageView {
    pub path: PathBuf,
    tex: egui::TextureHandle,
    size: [usize; 2],
    zoom: Option<f32>,
}

impl ImageView {
    pub fn open(path: &Path, ctx: &egui::Context) -> Result<ImageView, String> {
        let img = image::open(path).map_err(|e| format!("image illisible : {e}"))?.to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        let tex = ctx.load_texture(format!("img{}", path.display()), ci, egui::TextureOptions::LINEAR);
        Ok(ImageView { path: path.to_path_buf(), tex, size, zoom: None })
    }

    pub fn title(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{} × {} px", self.size[0], self.size[1])).color(p.comment));
            if ui.small_button("Ajuster").clicked() { self.zoom = None; }
            if ui.small_button("100 %").clicked() { self.zoom = Some(1.0); }
        });
        let avail = ui.available_size();
        let fit = (avail.x / self.size[0] as f32).min(avail.y / self.size[1] as f32).min(1.0);
        if ui.rect_contains_pointer(ui.max_rect()) {
            let z = ui.input(|i| i.zoom_delta());
            if (z - 1.0).abs() > 0.001 { self.zoom = Some((self.zoom.unwrap_or(fit) * z).clamp(0.05, 20.0)); }
        }
        let z = self.zoom.unwrap_or(fit);
        let sz = Vec2::new(self.size[0] as f32 * z, self.size[1] as f32 * z);
        egui::ScrollArea::both().id_salt(("img", &self.path)).auto_shrink(false).show(ui, |ui| {
            let full = Vec2::new(sz.x.max(avail.x), sz.y.max(avail.y - 30.0));
            let (rect, _) = ui.allocate_exact_size(full, Sense::hover());
            let r = Rect::from_center_size(rect.center(), sz);
            // damier pour les zones transparentes
            ui.painter().rect_filled(r, 0.0, p.bg_hl);
            ui.painter().image(self.tex.id(), r, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        });
    }
}

pub fn is_image(path: &Path) -> bool {
    matches!(path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(),
             Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico"))
}
