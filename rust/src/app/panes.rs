//! Volets (écran partagé) : chaque volet a ses onglets ; on sépare en glissant un onglet sur le
//! bord droit ou bas d'un volet, ou par le clic droit sur l'onglet.
use super::*;
use crate::explorer;
use egui::{Align2, Rect, Sense, Stroke};

#[derive(Clone, Copy, PartialEq)]
enum Zone { Right, Bottom, Center }

impl App {
    pub(super) fn layout_ui(&mut self, ui: &mut egui::Ui, node: &Layout, rect: Rect) -> Layout {
        match node {
            Layout::Leaf(p) => {
                self.pane_ui(ui, *p, rect);
                node.clone()
            }
            Layout::Split { vertical, ratio, a, b } => {
                let gap = 5.0;
                let (ra, rb, sep) = if *vertical {
                    let y = rect.min.y + rect.height() * ratio;
                    (Rect::from_min_max(rect.min, egui::pos2(rect.max.x, y - gap / 2.0)),
                     Rect::from_min_max(egui::pos2(rect.min.x, y + gap / 2.0), rect.max),
                     Rect::from_min_max(egui::pos2(rect.min.x, y - gap / 2.0), egui::pos2(rect.max.x, y + gap / 2.0)))
                } else {
                    let x = rect.min.x + rect.width() * ratio;
                    (Rect::from_min_max(rect.min, egui::pos2(x - gap / 2.0, rect.max.y)),
                     Rect::from_min_max(egui::pos2(x + gap / 2.0, rect.min.y), rect.max),
                     Rect::from_min_max(egui::pos2(x - gap / 2.0, rect.min.y), egui::pos2(x + gap / 2.0, rect.max.y)))
                };
                let mut first = Vec::new();
                a.panes(&mut first);
                let resp = ui.interact(sep, egui::Id::new(("sep", first.first().copied().unwrap_or(0), *vertical)), Sense::drag());
                let mut r = *ratio;
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(if *vertical { egui::CursorIcon::ResizeVertical } else { egui::CursorIcon::ResizeHorizontal });
                }
                if resp.dragged() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        r = if *vertical { (pos.y - rect.min.y) / rect.height() } else { (pos.x - rect.min.x) / rect.width() };
                        r = r.clamp(0.12, 0.88);
                    }
                }
                ui.painter().rect_filled(sep, 0.0, if resp.hovered() || resp.dragged() { self.p.blue.gamma_multiply(0.6) } else { self.p.bg_dark });
                let na = self.layout_ui(ui, a, ra);
                let nb = self.layout_ui(ui, b, rb);
                Layout::Split { vertical: *vertical, ratio: r, a: Box::new(na), b: Box::new(nb) }
            }
        }
    }

    pub(super) fn apply_pane_ops(&mut self) {
        for op in std::mem::take(&mut self.pane_ops) {
            match op {
                PaneOp::Close(uid) => {
                    if let Some(i) = self.tabs.iter().position(|t| t.uid == uid) { self.close_tab(i, false); }
                }
                PaneOp::CloseOthers(uid) => {
                    let Some(pane) = self.tabs.iter().find(|t| t.uid == uid).map(|t| t.pane) else { continue };
                    let others: Vec<u64> = self.tabs.iter().filter(|t| t.pane == pane && t.uid != uid && !t.dirty()).map(|t| t.uid).collect();
                    for o in others {
                        if let Some(i) = self.tabs.iter().position(|t| t.uid == o) { self.close_tab(i, true); }
                    }
                }
                PaneOp::Split(uid, pane, vertical) => self.split_tab(uid, pane, vertical),
                PaneOp::Move(uid, pane) => self.move_tab(uid, pane),
            }
        }
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, pane: usize, rect: Rect) {
        let p = self.p;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(("pane", pane)));
        let ui = &mut child;
        ui.set_clip_rect(rect);
        if ui.input(|i| i.pointer.any_pressed()) && ui.rect_contains_pointer(rect) && self.focus_pane != pane {
            self.focus_pane = pane;
        }
        let focused = self.focus_pane == pane;
        let active = self.active_in(pane);
        if let Some(a) = active { self.pane_active.insert(pane, self.tabs[a].uid); }
        // ---- barre d'onglets
        let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 32.0));
        ui.painter().rect_filled(bar, 0.0, p.bg_dark);
        let ids: Vec<usize> = (0..self.tabs.len()).filter(|i| self.tabs[*i].pane == pane).collect();
        let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar));
        egui::ScrollArea::horizontal().id_salt(("tabs", pane))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut bar_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for &i in &ids {
                        let t = &self.tabs[i];
                        let is_active = Some(i) == active;
                        let mut title = t.title();
                        if let Some((_, _, lab)) = t.doc().and_then(|d| d.remote.as_ref()) { title = format!("{title} · {lab}"); }
                        let w = ui.fonts_mut(|f| f.layout_no_wrap(title.clone(), FontId::proportional(13.0), p.fg)).size().x + 62.0;
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::click_and_drag());
                        let bg = if is_active { p.bg } else if resp.hovered() { p.bg_hl.gamma_multiply(0.6) } else { p.bg_dark };
                        ui.painter().rect_filled(r, 0.0, bg);
                        if is_active {
                            ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(2.0, if focused { p.blue } else { p.comment }));
                        }
                        let badge_name = match &t.c { Content::Pdf(_) => "x.pdf".to_string(), Content::Image(_) => "x.png".to_string(), _ => t.title() };
                        explorer::file_badge(ui.painter(), egui::pos2(r.min.x + 16.0, r.center().y), &badge_name, &p);
                        ui.painter().text(egui::pos2(r.min.x + 30.0, r.center().y), Align2::LEFT_CENTER, &title,
                                          FontId::proportional(13.0), if is_active { p.fg } else { p.comment });
                        let cb = Rect::from_center_size(egui::pos2(r.max.x - 15.0, r.center().y), Vec2::splat(16.0));
                        let hover_close = ui.rect_contains_pointer(cb);
                        if t.dirty() && !hover_close {
                            ui.painter().circle_filled(cb.center(), 4.0, p.fg.gamma_multiply(0.8));
                        } else if resp.hovered() || is_active {
                            if hover_close { ui.painter().rect_filled(cb, 3.0, p.bg_hl); }
                            crate::icons::paint(ui.painter(), crate::icons::Icon::Close, cb.center(), 10.0, if hover_close { p.red } else { p.comment });
                        }
                        let uid = t.uid;
                        let path_txt = t.path().map(|x| x.to_string_lossy().to_string())
                            .or_else(|| t.doc().and_then(|d| d.remote.as_ref()).map(|(_, rp, lab)| format!("{lab}:{rp}")));
                        if resp.clicked() {
                            if hover_close { self.pane_ops.push(PaneOp::Close(uid)); }
                            else { self.pane_active.insert(pane, uid); self.focus_pane = pane; if let Some(d) = self.tabs[i].doc_mut() { d.reload_if_changed(); } }
                        }
                        if resp.middle_clicked() { self.pane_ops.push(PaneOp::Close(uid)); }
                        if resp.drag_started() { self.drag_tab = Some(uid); }
                        let others: Vec<usize> = self.panes().into_iter().filter(|x| *x != pane).collect();
                        resp.context_menu(|ui| {
                            if ui.button("Séparer à droite").clicked() { self.pane_ops.push(PaneOp::Split(uid, pane, false)); ui.close(); }
                            if ui.button("Séparer en bas").clicked() { self.pane_ops.push(PaneOp::Split(uid, pane, true)); ui.close(); }
                            for (k, o) in others.iter().enumerate() {
                                if ui.button(format!("Déplacer vers le volet {}", k + 1)).clicked() { self.pane_ops.push(PaneOp::Move(uid, *o)); ui.close(); }
                            }
                            ui.separator();
                            if let Some(pt) = &path_txt {
                                if ui.button("Copier le chemin").clicked() { ui.ctx().copy_text(pt.clone()); ui.close(); }
                            }
                            if ui.button("Fermer les autres onglets").clicked() { self.pane_ops.push(PaneOp::CloseOthers(uid)); ui.close(); }
                            if ui.button("Fermer").clicked() { self.pane_ops.push(PaneOp::Close(uid)); ui.close(); }
                        });
                        if let Some(pt) = path_txt { resp.on_hover_text(pt); }
                    }
                });
            });
        // ---- contenu
        let body = Rect::from_min_max(egui::pos2(rect.min.x, bar.max.y), rect.max);
        let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body).id_salt(("body", pane)));
        if let Some(i) = active {
            let font = FontId::monospace(self.font_size);
            let dbg = self.dbg_line.clone();
            let err = self.error_line.clone();
            let find_open = self.find_open && focused;
            let find_text = self.find_text.clone();
            let can_bp = self.module.is_some();
            match &mut self.tabs[i].c {
                Content::Doc(d) => {
                    let path = d.path.clone();
                    let line_of = |x: &Option<(PathBuf, usize)>| -> Option<usize> {
                        match (x, &path) { (Some((f, l)), Some(pp)) if same_path(f, pp) => Some(*l), _ => None }
                    };
                    d.find = if find_open { Some(find_text) } else { None };
                    let bp_ok = can_bp && d.lang == crate::highlight::Lang::Python && d.remote.is_none();
                    let ev = d.ui(&mut body_ui, &p, &font, line_of(&dbg), line_of(&err), bp_ok);
                    if ev.changed { self.error_line = None; }
                    if let Some(line) = ev.breakpoint { self.sync_breakpoint(i, line); }
                    if find_open {
                        self.find_info = if ev.not_found { "aucun résultat".into() } else if ev.matches > 0 { format!("{} résultat(s)", ev.matches) } else { String::new() };
                    }
                }
                Content::Pdf(v) => v.ui(&mut body_ui, &p),
                Content::Image(v) => v.ui(&mut body_ui, &p),
            }
        }
        // ---- dépôt d'un onglet glissé : bord droit / bas = séparer, milieu = déplacer ici
        if let Some(uid) = self.drag_tab {
            if let Some(pos) = ui.ctx().pointer_hover_pos().filter(|pos| rect.contains(*pos)) {
                let zone = if pos.x > body.max.x - body.width() * 0.3 { Zone::Right }
                           else if pos.y > body.max.y - body.height() * 0.3 { Zone::Bottom } else { Zone::Center };
                let hint = match zone {
                    Zone::Right => Rect::from_min_max(egui::pos2(body.center().x, body.min.y), body.max),
                    Zone::Bottom => Rect::from_min_max(egui::pos2(body.min.x, body.center().y), body.max),
                    Zone::Center => body,
                };
                let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
                painter.rect_filled(hint.shrink(4.0), 6.0, p.blue.gamma_multiply(0.18));
                painter.rect_stroke(hint.shrink(4.0), 6.0, Stroke::new(2.0, p.blue), egui::StrokeKind::Inside);
                if ui.input(|i| i.pointer.any_released()) {
                    let from_pane = self.tabs.iter().find(|t| t.uid == uid).map(|t| t.pane);
                    match zone {
                        Zone::Right => self.pane_ops.push(PaneOp::Split(uid, pane, false)),
                        Zone::Bottom => self.pane_ops.push(PaneOp::Split(uid, pane, true)),
                        Zone::Center => if from_pane != Some(pane) { self.pane_ops.push(PaneOp::Move(uid, pane)); },
                    }
                    self.drag_tab = None;
                }
            }
        }
        if ui.input(|i| i.pointer.any_released()) && !ui.ctx().pointer_hover_pos().map(|p| rect.contains(p)).unwrap_or(true) {
            // relâché ailleurs : rien
        }
    }
}
