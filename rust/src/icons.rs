//! Icônes : exactement celles de la version Python (dessins SVG de studentcode/icons.py rendus en
//! masques blancs par scripts/gen_rust_icons.py), recolorées à l'affichage.
use eframe::egui::{self, pos2, Color32, Painter, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Menu, New, Open, Save, Run, Debug, Continue, Over, Into, Outof, Pause, Stop, Bp, Libs, Search,
    Terminal, Output, Folder, File, Text, Image, Min, Max, Move, Trash, SplitR, SplitD, Globe, Back,
    Forward, Reload, Star, External, Pdf, Server, Plug, More, Edit, Key, Up, Upload, Download, Shield,
    Palette, ChevRight, ChevDown, NewPlus, FitWidth, FitPage, Moon, Close, Python, Settings,
    /// « ⇄ » des onglets distants (dessiné, pas d'image)
    Swap,
    /// flèche vers le haut fine (résultat précédent)
    ChevUp,
}

impl Icon {
    fn name(self) -> &'static str {
        use Icon::*;
        match self {
            Menu => "menu", New => "new", Open => "open", Save => "save", Run => "run", Debug => "debug",
            Continue => "continue", Over => "over", Into => "into", Outof => "outof", Pause => "pause",
            Stop => "stop", Bp => "bp", Libs => "libs", Search => "search", Terminal => "terminal",
            Output => "output", Folder => "folder", File => "file", Text => "text", Image => "image",
            Min => "min", Max => "max", Move => "move", Trash => "trash", SplitR => "split_r",
            SplitD => "split_d", Globe => "globe", Back => "back", Forward => "forward", Reload => "reload",
            Star => "star", External => "external", Pdf => "pdf", Server => "server", Plug => "plug",
            More => "more", Edit => "edit", Key => "key", Up => "up", Upload => "upload",
            Download => "download", Shield => "shield", Palette => "palette", ChevRight => "chev_right",
            ChevDown => "chev_down", NewPlus => "new_plus", FitWidth => "fit_width", FitPage => "fit_page",
            Moon => "moon", Close => "close", Python => "python", Settings => "settings", Swap | ChevUp => "",
        }
    }
}

/// Icône d'un fichier : pastille (fond + lettres) ou dessin, avec sa couleur Tokyo Night d'origine.
#[derive(Clone, Copy)]
pub enum FileIcon {
    Badge(&'static str, &'static str),
    Icon(&'static str, &'static str),
}

thread_local! {
    static TEX: RefCell<HashMap<&'static str, egui::TextureHandle>> = RefCell::new(HashMap::new());
}

fn texture(ctx: &egui::Context, name: &'static str) -> Option<egui::TextureId> {
    TEX.with(|t| {
        let mut t = t.borrow_mut();
        if let Some(h) = t.get(name) { return Some(h.id()); }
        let bytes = crate::fileicons_gen::IMAGES.iter().find(|(n, _)| *n == name)?.1;
        let img = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
        let opts = egui::TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..egui::TextureOptions::LINEAR };
        let hdl = ctx.load_texture(format!("icon:{name}"), ci, opts);
        let id = hdl.id();
        t.insert(name, hdl);
        Some(id)
    })
}

/// Dessine une image d'icône (`name`) dans un carré de `size` px centré sur `c`.
pub fn paint_named(p: &Painter, name: &'static str, c: Pos2, size: f32, col: Color32) {
    if let Some(id) = texture(p.ctx(), name) {
        let r = Rect::from_center_size(snap(p, c), Vec2::splat(size));
        p.image(id, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), col);
    }
}

fn snap(p: &Painter, c: Pos2) -> Pos2 {
    let ppp = p.ctx().pixels_per_point();
    pos2((c.x * ppp).round() / ppp, (c.y * ppp).round() / ppp)
}

/// Dessine `icon` dans un carré de `size` px (le dessin d'origine fait 24 unités) centré sur `c`.
pub fn paint(p: &Painter, icon: Icon, c: Pos2, size: f32, col: Color32) {
    match icon {
        Icon::Swap => {
            let s = size / 24.0;
            let r = |x: f32, y: f32| pos2(c.x + (x - 12.0) * s, c.y + (y - 12.0) * s);
            let st = Stroke::new(1.3, col);
            p.line_segment([r(3.0, 9.0), r(21.0, 9.0)], st);
            p.add(Shape::line(vec![r(17.0, 5.0), r(21.0, 9.0), r(17.0, 13.0)], st));
            p.line_segment([r(3.0, 15.0), r(21.0, 15.0)], st);
            p.add(Shape::line(vec![r(7.0, 11.0), r(3.0, 15.0), r(7.0, 19.0)], st));
        }
        Icon::ChevUp => paint_named(p, "up", c, size, col),
        Icon::Python => paint_named(p, "python", c, size, Color32::WHITE),
        _ => paint_named(p, icon.name(), c, size, col),
    }
}

/// Couleur Tokyo Night d'une icône → couleur du thème courant (comme REMAP de la version Python).
pub fn themed(hex: &str, pal: &crate::theme::Palette) -> Color32 {
    let tn = ["#1a1b26", "#16161e", "#292e42", "#c0caf5", "#565f89", "#3b4261", "#7aa2f7", "#7dcfff",
              "#9ece6a", "#bb9af7", "#ff9e64", "#f7768e", "#e0af68", "#73daca"];
    let cols = [pal.bg, pal.bg_dark, pal.bg_hl, pal.fg, pal.comment, pal.gutter, pal.blue, pal.cyan,
                pal.green, pal.magenta, pal.orange, pal.red, pal.yellow, pal.teal];
    match tn.iter().position(|t| t.eq_ignore_ascii_case(hex)) {
        Some(i) => cols[i],
        None => Color32::from_hex(hex).unwrap_or(pal.comment),
    }
}

/// Icône d'un fichier d'après son nom (même table que la version Python).
pub fn file_icon_of(name: &str) -> FileIcon {
    let lower = name.to_lowercase();
    if let Some((_, f)) = crate::fileicons_gen::NAMES.iter().find(|(n, _)| *n == lower) { return *f; }
    // comme os.path.splitext : « .gitignore » n'a pas d'extension, on cherche alors le nom entier
    let ext = match lower.rfind('.') { Some(i) if i > 0 => &lower[i..], _ => lower.as_str() };
    crate::fileicons_gen::EXT.iter().find(|(e, _)| *e == ext).map(|(_, f)| *f).unwrap_or(FileIcon::Icon("file", "#565f89"))
}

pub fn paint_file(p: &Painter, c: Pos2, name: &str, size: f32, pal: &crate::theme::Palette) {
    match file_icon_of(name) {
        FileIcon::Badge(txt, col) => {
            paint_named(p, "badge_bg", c, size, themed(col, pal));
            paint_named(p, txt, c, size, pal.bg);
        }
        FileIcon::Icon(n, col) => paint_named(p, n, c, size, themed(col, pal)),
    }
}

/// Bouton plat de la barre d'outils (icône 18 px, comme QToolButton de la version Python).
/// Comme un bouton du système : pressé puis relâché sur le bouton = clic, même si la souris a
/// bougé de quelques pixels entre les deux (egui, lui, annule le clic au-delà de 6 px).
pub fn tolerant(ui: &egui::Ui, mut resp: egui::Response) -> egui::Response {
    if !resp.clicked() && resp.enabled() && resp.contains_pointer() {
        let ok = ui.input(|i| i.pointer.primary_released()
            && i.pointer.press_origin().map(|o| resp.rect.contains(o)).unwrap_or(false));
        if ok { resp.flags |= egui::response::Flags::FAKE_PRIMARY_CLICKED; }
    }
    resp
}

pub fn button(ui: &mut egui::Ui, icon: Icon, col: Color32, hover_bg: Color32, tip: &str) -> egui::Response {
    sized_button(ui, icon, col, hover_bg, tip, Vec2::new(32.0, 30.0), 18.0)
}

/// Petit bouton (icône 16 px).
pub fn small(ui: &mut egui::Ui, icon: Icon, col: Color32, hover_bg: Color32, tip: &str) -> egui::Response {
    sized_button(ui, icon, col, hover_bg, tip, Vec2::new(26.0, 26.0), 16.0)
}

pub fn sized_button(ui: &mut egui::Ui, icon: Icon, col: Color32, hover_bg: Color32, tip: &str, size: Vec2, s: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = tolerant(ui, resp);
    if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, hover_bg);
    }
    paint(ui.painter(), icon, rect.center(), s, col);
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

/// Ligne de menu avec une icône (comme les QAction avec icône de la version Python).
pub fn menu_item(ui: &mut egui::Ui, icon: Icon, col: Color32, text: &str) -> egui::Response {
    let font = egui::FontId::proportional(13.3);
    let tw = ui.fonts_mut(|f| f.layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE)).size().x;
    let w = (tw + 54.0).max(ui.available_width());
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 27.0), Sense::click());
    let resp = tolerant(ui, resp);
    if resp.hovered() { ui.painter().rect_filled(r, 4.0, ui.visuals().widgets.open.weak_bg_fill); }
    paint(ui.painter(), icon, pos2(r.min.x + 16.0, r.center().y), 16.0, col);
    ui.painter().text(pos2(r.min.x + 32.0, r.center().y), egui::Align2::LEFT_CENTER, text, font, ui.visuals().text_color());
    resp
}

/// Bouton Qt de la version Python (QPushButton : fond gris clair, coins arrondis 5 px ;
/// « primary » : fond bleu, texte sombre en gras). `w` : largeur (la place restante si 0).
pub fn push_button(ui: &mut egui::Ui, pal: &crate::theme::Palette, icon: Option<(Icon, Color32)>, text: &str,
                   primary: bool, enabled: bool, w: f32, h: f32) -> egui::Response {
    let w = if w <= 0.0 { ui.available_width() } else { w };
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, h), if enabled { Sense::click() } else { Sense::hover() });
    let resp = tolerant(ui, resp);
    let bg = if primary {
        if enabled { if resp.hovered() { pal.blue.gamma_multiply(0.85) } else { pal.blue } } else { pal.bg_hl }
    } else if resp.hovered() && enabled { pal.gutter } else { pal.bg_hl };
    ui.painter().rect_filled(r, 5.0, bg);
    let font = if primary { egui::FontId::new(13.3, egui::FontFamily::Name("bold".into())) } else { egui::FontId::proportional(13.3) };
    let col = if !enabled { pal.comment } else if primary { pal.bg_dark } else { pal.fg };
    let tw = ui.fonts_mut(|f| f.layout_no_wrap(text.to_string(), font.clone(), col)).size().x;
    let iw = if icon.is_some() { 18.0 + 8.0 } else { 0.0 };
    let x0 = r.center().x - (tw + iw) / 2.0;
    if let Some((ic, c)) = icon { paint(ui.painter(), ic, pos2(x0 + 9.0, r.center().y), 18.0, if enabled { c } else { pal.comment }); }
    ui.painter().text(pos2(x0 + iw, r.center().y), egui::Align2::LEFT_CENTER, text, font, col);
    resp
}

/// Champ de saisie Qt (QLineEdit : fond, bord gris arrondi, bleu quand il a le focus).
pub fn line_edit(ui: &mut egui::Ui, pal: &crate::theme::Palette, text: &mut String, hint: &str, id: egui::Id,
                 enabled: bool, w: f32) -> egui::Response {
    let w = if w <= 0.0 { ui.available_width() } else { w };
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::hover());
    let foc = ui.memory(|m| m.has_focus(id));
    ui.painter().rect_filled(r, 5.0, pal.bg);
    ui.painter().rect_stroke(r, 5.0, Stroke::new(1.0, if foc { pal.blue } else { pal.gutter }), egui::StrokeKind::Inside);
    let tr = r.shrink2(Vec2::new(8.0, 3.0));
    let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(tr).layout(egui::Layout::left_to_right(egui::Align::Center)));
    cu.add_enabled(enabled, egui::TextEdit::singleline(text).id(id).hint_text(hint).frame(egui::Frame::NONE)
        .desired_width(tr.width()).font(egui::FontId::proportional(13.3)))
}

/// Case à cocher dans le style de la version Python (une coche quand elle est cochée).
pub fn check_box(ui: &mut egui::Ui, pal: &crate::theme::Palette, on: &mut bool, text: &str, enabled: bool) -> egui::Response {
    let font = egui::FontId::proportional(13.3);
    let w = ui.available_width();
    // texte sur plusieurs lignes si le panneau est étroit
    let galley = ui.fonts_mut(|f| f.layout(text.to_string(), font.clone(), pal.fg, (w - 20.0).max(40.0)));
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, (galley.size().y + 3.0).max(20.0)), if enabled { Sense::click() } else { Sense::hover() });
    let resp = tolerant(ui, resp);
    let bx = Rect::from_center_size(pos2(r.min.x + 7.0, r.min.y + 10.0), Vec2::splat(13.0));
    let col = if enabled { pal.fg } else { pal.comment };
    if *on {
        ui.painter().add(Shape::line(vec![pos2(bx.min.x + 2.5, bx.center().y), pos2(bx.min.x + 5.5, bx.max.y - 3.0), pos2(bx.max.x - 2.0, bx.min.y + 3.0)], Stroke::new(1.6, col)));
    } else {
        ui.painter().rect_stroke(bx, 2.0, Stroke::new(1.0, pal.gutter), egui::StrokeKind::Inside);
    }
    let ty = if galley.rows.len() > 1 { r.min.y + 2.0 } else { r.center().y - galley.size().y / 2.0 };
    ui.painter().with_clip_rect(r.intersect(ui.clip_rect())).galley(pos2(r.min.x + 18.0, ty), galley, col);
    if resp.clicked() && enabled { *on = !*on; }
    resp
}

/// Action de menu comme une QAction de la version Python : icône, texte, raccourci à droite.
pub fn menu_action(ui: &mut egui::Ui, icon: Option<(Icon, Color32)>, text: &str, shortcut: &str, enabled: bool) -> egui::Response {
    let font = egui::FontId::proportional(13.3);
    let tw = ui.fonts_mut(|f| f.layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE)).size().x;
    let sw = ui.fonts_mut(|f| f.layout_no_wrap(shortcut.to_string(), font.clone(), Color32::WHITE)).size().x;
    let w = (14.0 + 26.0 + tw + 36.0 + sw + 24.0).max(ui.available_width());
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 27.0), if enabled { Sense::click() } else { Sense::hover() });
    let resp = tolerant(ui, resp);
    if resp.hovered() && enabled { ui.painter().rect_filled(r, 4.0, ui.visuals().widgets.open.weak_bg_fill); }
    let tc = if enabled { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
    if let Some((ic, c)) = icon { paint(ui.painter(), ic, pos2(r.min.x + 22.0, r.center().y), 16.0, if enabled { c } else { tc }); }
    ui.painter().text(pos2(r.min.x + 40.0, r.center().y), egui::Align2::LEFT_CENTER, text, font.clone(), tc);
    if !shortcut.is_empty() {
        ui.painter().text(pos2(r.max.x - 24.0, r.center().y), egui::Align2::RIGHT_CENTER, shortcut, font, tc);
    }
    resp
}

/// Action cochable (coche à la place de l'icône).
pub fn menu_check(ui: &mut egui::Ui, on: bool, text: &str, shortcut: &str) -> egui::Response {
    let r = menu_action(ui, None, text, shortcut, true);
    if on {
        let c = pos2(r.rect.min.x + 22.0, r.rect.center().y);
        let col = ui.visuals().text_color();
        ui.painter().add(Shape::line(vec![c + Vec2::new(-5.0, 0.0), c + Vec2::new(-1.5, 3.5), c + Vec2::new(5.0, -4.0)], Stroke::new(1.6, col)));
    }
    r
}
