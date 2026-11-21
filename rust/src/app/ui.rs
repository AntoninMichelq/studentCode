//! Barres du haut et du bas, panneau du bas (terminaux, sortie), débogueur, boîtes de dialogue.
use super::*;
use crate::icons::{self, Icon};
use egui::{Align2, Rect, Sense, Stroke};

/// Glisser un onglet sur un autre les échange (l'ordre est gardé dans les réglages).
/// Renvoie vrai si l'ordre a changé.
pub(super) fn drag_reorder(ui: &egui::Ui, order: &mut [String], tabs: &[(String, Rect, egui::Response)]) -> bool {
    let Some((d, _, _)) = tabs.iter().find(|(_, _, r)| r.dragged()) else { return false };
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    let Some(pos) = ui.ctx().pointer_interact_pos() else { return false };
    let Some((k, kr, _)) = tabs.iter().find(|(k, r, _)| k != d && r.contains(pos)) else { return false };
    // n'échanger qu'une fois le pointeur au-delà du milieu de l'onglet visé (sinon ça oscille)
    let (a, b) = match (order.iter().position(|x| x == d), order.iter().position(|x| x == k)) {
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    if (b > a && pos.x < kr.center().x) || (b < a && pos.x > kr.center().x) { return false; }
    if a < b { order[a..=b].rotate_left(1); } else { order[b..=a].rotate_right(1); }
    true
}

/// Bouton de la barre d'outils.
pub(super) struct TbItem {
    id: &'static str,
    icon: Icon,
    col: Color32,
    tip: String,
    keys: String,
    enabled: bool,
}

impl App {
    pub(super) fn window_edges(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)) {
            return;
        }
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else { return };
        let r = ctx.viewport_rect();
        let m = 5.0;
        let (l, rt, t, b) = (pos.x - r.min.x < m, r.max.x - pos.x < m, pos.y - r.min.y < m, r.max.y - pos.y < m);
        use egui::{CursorIcon as C, ResizeDirection as D};
        let dir = match (l, rt, t, b) {
            (true, _, true, _) => Some((D::NorthWest, C::ResizeNorthWest)),
            (_, true, true, _) => Some((D::NorthEast, C::ResizeNorthEast)),
            (true, _, _, true) => Some((D::SouthWest, C::ResizeSouthWest)),
            (_, true, _, true) => Some((D::SouthEast, C::ResizeSouthEast)),
            (true, ..) => Some((D::West, C::ResizeWest)),
            (_, true, ..) => Some((D::East, C::ResizeEast)),
            (_, _, true, _) => Some((D::North, C::ResizeNorth)),
            (_, _, _, true) => Some((D::South, C::ResizeSouth)),
            _ => None,
        };
        if let Some((d, c)) = dir {
            ctx.set_cursor_icon(c);
            if ctx.input(|i| i.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(d));
            }
        }
    }

    /// Barre d'outils identique à celle de la version Python (QToolBar : icônes 18 px, boutons de
    /// 34 px, séparateurs fins, titre au centre, boutons de fenêtre à droite).
    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = self.p;
        egui::Panel::top("top").exact_size(44.0)
            .frame(egui::Frame::new().fill(p.bg_dark).inner_margin(egui::Margin { left: 8, right: 9, top: 6, bottom: 7 }))
            .show(ui, |ui| {
                let full = ui.max_rect().expand2(Vec2::new(9.0, 7.0));
                ui.painter().line_segment([egui::pos2(full.min.x, full.max.y - 0.5), egui::pos2(full.max.x + 9.0, full.max.y - 0.5)], Stroke::new(1.0, p.bg));
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    let btn = |ui: &mut egui::Ui, ic: Icon, col: Color32, tip: &str| {
                        icons::sized_button(ui, ic, col, p.bg_hl, tip, Vec2::new(34.0, 31.0), 18.0)
                    };
                    let sep = |ui: &mut egui::Ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(13.0, 31.0), Sense::hover());
                        ui.painter().line_segment([egui::pos2(r.center().x, r.min.y + 3.0), egui::pos2(r.center().x, r.max.y - 3.0)], Stroke::new(1.0, p.bg_hl));
                    };
                    // boutons choisis dans les Paramètres (ou clic droit sur la barre) ; fenêtre étroite :
                    // comme QToolBar, les derniers passent dans un menu « » »
                    let items = self.toolbar_items();
                    let menu = btn(ui, Icon::Menu, p.fg, &format!("Menu  ({})", self.cfg.shortcut_text(crate::settings::Action::Menu)));
                    if std::mem::take(&mut self.menu_request) {
                        egui::Popup::open_id(ctx, egui::Popup::default_response_id(&menu));
                    }
                    egui::Popup::menu(&menu).show(|ui| self.main_menu(ui, ctx));
                    let mut menus = vec![menu];
                    if !items.is_empty() { sep(ui); }
                    let width = |it: &Option<TbItem>| if it.is_some() { 35.0 } else { 14.0 };
                    let avail = ui.available_width() - 3.0 * 35.0 - 60.0;
                    let total: f32 = items.iter().map(width).sum();
                    let mut shown = items.len();
                    if total > avail {
                        let mut w = 25.0;
                        shown = items.iter().take_while(|it| { w += width(it); w <= avail }).count();
                    }
                    let mut clicked: Option<&'static str> = None;
                    for it in &items[..shown] {
                        match it {
                            None => sep(ui),
                            Some(t) => {
                                let r = btn(ui, t.icon, if t.enabled { t.col } else { p.gutter }, &t.tip);
                                if t.id == "web" { egui::Popup::menu(&r).show(|ui| self.web_menu(ui)); }
                                else if r.clicked() && t.enabled { clicked = Some(t.id); }
                                menus.push(r);
                            }
                        }
                    }
                    if shown < items.len() {
                        let more = icons::sized_button(ui, Icon::ChevRight, p.fg, p.bg_hl, "Plus de boutons", Vec2::new(24.0, 31.0), 14.0);
                        egui::Popup::menu(&more).show(|ui| {
                            ui.set_min_width(300.0);
                            let mut last_sep = true;
                            for it in &items[shown..] {
                                match it {
                                    None => { if !last_sep { ui.separator(); } last_sep = true; }
                                    Some(t) if t.id == "web" => { ui.menu_button("Pages web", |ui| self.web_menu(ui)); last_sep = false; }
                                    Some(t) => {
                                        let name = t.tip.split("  (").next().unwrap_or(&t.tip).to_string();
                                        if icons::menu_action(ui, Some((t.icon, t.col)), &name, &t.keys, t.enabled).clicked() {
                                            clicked = Some(t.id); ui.close();
                                        }
                                        last_sep = false;
                                    }
                                }
                            }
                        });
                        menus.push(more);
                    }
                    if let Some(id) = clicked { self.toolbar_click(id, ctx); }
                    // zone libre : titre, déplacer la fenêtre, double-clic pour agrandir
                    let right_w = 3.0 * 35.0;
                    let free = (ui.available_width() - right_w).max(4.0);
                    {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(free, 31.0), Sense::click_and_drag());
                        let mid = rect.center();
                        ui.painter().with_clip_rect(rect).text(mid, Align2::CENTER_CENTER, &self.title, FontId::proportional(13.3), p.comment);
                        if resp.drag_started() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                        if resp.double_clicked() {
                            let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                        }
                        menus.push(resp);
                    }
                    // clic droit sur la barre : choisir les boutons
                    // (identifiant à part : sinon ce menu et celui du bouton — ☰, pages web, « » » — partagent le
                    // même état, et le clic gauche refermait aussitôt le menu qu'il venait d'ouvrir)
                    for r in menus {
                        egui::Popup::context_menu(&r).id(r.id.with("toolbar_ctx")).show(|ui| self.toolbar_menu(ui));
                    }
                    if btn(ui, Icon::Min, p.fg, "Réduire").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    if btn(ui, Icon::Max, p.fg, "Agrandir / restaurer").clicked() {
                        let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                    }
                    if btn(ui, Icon::Close, p.red, "Fermer").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
    }

    /// Boutons visibles de la barre d'outils, dans l'ordre choisi (`None` = séparateur). Certains
    /// n'apparaissent que lorsqu'ils servent, comme dans la version Python.
    fn toolbar_items(&self) -> Vec<Option<TbItem>> {
        use crate::settings::Action as A;
        let p = self.p;
        let python = (self.cur().is_none() || self.is_python()) && self.module.is_some();
        let stopped = self.debugging() && !self.frames.is_empty();
        let idle = !self.running() || self.repl;
        let k = |a: A| self.cfg.shortcut_text(a);
        let it = |id: &'static str, icon: Icon, col: Color32, name: &str, keys: String, enabled: bool| {
            let tip = if keys.is_empty() { name.to_string() } else { format!("{name}  ({keys})") };
            Some(TbItem { id, icon, col, tip, keys, enabled })
        };
        let mut v: Vec<Option<TbItem>> = Vec::new();
        for id in &self.cfg.toolbar_order {
            if id != "|" && self.cfg.toolbar_hidden.contains(id) { continue; }
            match id.as_str() {
                "|" => v.push(None),
                "new" => v.push(it("new", Icon::New, p.fg, "Nouveau", k(A::New), true)),
                "open" => v.push(it("open", Icon::Open, p.yellow, "Ouvrir un dossier", k(A::OpenFolder), true)),
                "open_file" => v.push(it("open_file", Icon::Open, p.blue, "Ouvrir un fichier", k(A::OpenFile), true)),
                "save" => v.push(it("save", Icon::Save, p.blue, "Enregistrer", k(A::Save), self.cur().is_some())),
                "save_all" => v.push(it("save_all", Icon::Save, p.cyan, "Tout enregistrer", k(A::SaveAll), self.tabs.iter().any(|t| t.dirty()))),
                "run" => v.push(it("run", Icon::Run, p.green, "Exécuter le fichier courant", k(A::Run), idle)),
                "debug" if python => {
                    let can = idle || stopped;
                    v.push(if stopped { it("debug", Icon::Continue, p.green, "Continuer", k(A::Debug), true) }
                           else { it("debug", Icon::Debug, p.orange, "Déboguer jusqu'au prochain point d'arrêt / Continuer", k(A::Debug), can) });
                }
                "steps" if self.debugging() => {
                    v.push(it("over", Icon::Over, p.blue, "Exécuter la ligne (par-dessus)", k(A::StepOver), stopped));
                    v.push(it("into", Icon::Into, p.blue, "Entrer dans la fonction", k(A::StepInto), stopped));
                    v.push(it("outof", Icon::Outof, p.blue, "Sortir de la fonction", k(A::StepOut), stopped));
                    v.push(it("pause", Icon::Pause, p.blue, "Mettre en pause", String::new(), !stopped));
                }
                "stop" if !idle => v.push(it("stop", Icon::Stop, p.red, "Arrêter le programme", k(A::Stop), true)),
                "bp" if python => v.push(it("bp", Icon::Bp, p.red, "Poser / enlever un point d'arrêt", k(A::Breakpoint), true)),
                "libs" if python => v.push(it("libs", Icon::Libs, p.magenta, "Installer / gérer les bibliothèques (pip)", k(A::Libs), true)),
                "search" => v.push(it("search", Icon::Search, p.cyan, "Rechercher", k(A::Find), self.cur().is_some())),
                "terminal" => v.push(it("terminal", Icon::Terminal, p.green, "Terminal", k(A::ToggleBottom), true)),
                "split_r" => v.push(it("split_r", Icon::SplitR, p.blue, "Nouveau volet à droite", k(A::SplitRight), self.cur().is_some())),
                "split_d" => v.push(it("split_d", Icon::SplitD, p.blue, "Nouveau volet en bas", String::new(), self.cur().is_some())),
                "web" => v.push(it("web", Icon::Globe, p.cyan, "Ouvrir une page web (cours, documentation…)", String::new(), true)),
                "ssh" => v.push(it("ssh", Icon::Server, p.cyan, "Se connecter à une machine en SSH", k(A::Ssh), true)),
                "vpn" => v.push(it("vpn", Icon::Shield, p.green, "Gérer les VPN (fichiers .ovpn)", k(A::Vpn), true)),
                "settings" => v.push(it("settings", Icon::Settings, p.fg, "Paramètres", k(A::Settings), true)),
                _ => {}
            }
        }
        // pas de séparateur au début, à la fin ou en double
        let mut out: Vec<Option<TbItem>> = Vec::new();
        for x in v {
            if x.is_none() && out.last().map(|l| l.is_none()).unwrap_or(true) { continue; }
            out.push(x);
        }
        while matches!(out.last(), Some(None)) { out.pop(); }
        out
    }

    fn toolbar_click(&mut self, id: &str, ctx: &egui::Context) {
        use crate::settings::Action as A;
        let a = match id {
            "new" => A::New, "open" => A::OpenFolder, "open_file" => A::OpenFile, "save" => A::Save, "save_all" => A::SaveAll,
            "run" => A::Run, "debug" => A::Debug, "over" => A::StepOver, "into" => A::StepInto, "outof" => A::StepOut,
            "stop" => A::Stop, "bp" => A::Breakpoint, "libs" => A::Libs, "search" => A::Find, "terminal" => A::ToggleBottom,
            "split_r" => A::SplitRight, "ssh" => A::Ssh, "vpn" => A::Vpn, "settings" => A::Settings,
            "pause" => { self.dbg_cmd("pause"); return; }
            "split_d" => {
                if let Some(i) = self.cur() { let (u, pa) = (self.tabs[i].uid, self.tabs[i].pane); self.split_tab(u, pa, true); }
                return;
            }
            _ => return,
        };
        self.run_action(a, ctx);
    }

    /// Clic droit sur la barre d'outils : cocher les boutons à afficher.
    fn toolbar_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(260.0);
        ui.label(egui::RichText::new("Boutons de la barre d'outils").color(self.p.comment));
        let mut changed = false;
        for (id, name) in crate::settings::TOOLBAR {
            let mut on = !self.cfg.toolbar_hidden.iter().any(|x| x == id);
            if ui.checkbox(&mut on, *name).changed() {
                if on { self.cfg.toolbar_hidden.retain(|x| x != id); } else { self.cfg.toolbar_hidden.push(id.to_string()); }
                changed = true;
            }
        }
        ui.separator();
        if ui.button("Personnaliser l'ordre…").clicked() { self.settings_open = true; self.settings_page = 1; ui.close(); }
        if ui.button("Rétablir la barre d'outils").clicked() {
            let d = crate::settings::Settings::default();
            self.cfg.toolbar_order = d.toolbar_order;
            self.cfg.toolbar_hidden = d.toolbar_hidden;
            changed = true;
        }
        if changed { self.cfg.save(); }
    }

    /// Menu ☰ : mêmes sous-menus et mêmes actions que la version Python.
    fn main_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = self.p;
        ui.set_min_width(170.0);
        let has_tab = self.cur().is_some();
        let keys: std::collections::HashMap<&'static str, String> = crate::settings::ACTIONS.iter().map(|a| (a.id(), self.cfg.shortcut_text(*a))).collect();
        ui.menu_button("Fichier", |ui| {
            ui.set_min_width(330.0);
            if icons::menu_action(ui, Some((Icon::New, p.fg)), "Nouveau fichier", &keys["new"], true).clicked() { self.new_doc(); ui.close(); }
            if icons::menu_action(ui, Some((Icon::Open, p.yellow)), "Ouvrir un fichier…", &keys["open_file"], true).clicked() {
                if let Some(f) = rfd::FileDialog::new().set_directory(&self.root).pick_file() { self.open_file(&f); }
                ui.close();
            }
            if icons::menu_action(ui, Some((Icon::Open, p.yellow)), "Ouvrir un dossier…", &keys["open_folder"], true).clicked() { self.open_folder_dialog(); ui.close(); }
            ui.separator();
            if icons::menu_action(ui, Some((Icon::Save, p.blue)), "Enregistrer", &keys["save"], has_tab).clicked() { if let Some(i) = self.cur() { self.save_tab(i); } ui.close(); }
            if icons::menu_action(ui, Some((Icon::Save, p.blue)), "Enregistrer sous…", &keys["save_as"], has_tab).clicked() {
                if let Some(i) = self.cur() {
                    if let Some(f) = rfd::FileDialog::new().set_directory(&self.root).save_file() {
                        if let Some(d) = self.tabs[i].doc_mut() {
                            d.remote = None;
                            let r = d.save_as(f);
                            self.report_save(r);
                        }
                    }
                }
                ui.close();
            }
            ui.separator();
            if icons::menu_action(ui, Some((Icon::Close, p.comment)), "Fermer l'onglet", &keys["close_tab"], has_tab).clicked() { if let Some(i) = self.cur() { self.close_tab(i, false); } ui.close(); }
            if icons::menu_action(ui, None, "Quitter", &keys["quit"], true).clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); ui.close(); }
        });
        ui.menu_button("Édition", |ui| {
            ui.set_min_width(330.0);
            use crate::editor::EditCmd;
            let mut cmd: Option<EditCmd> = None;
            if icons::menu_action(ui, None, "Annuler", "Ctrl+Z", has_tab).clicked() { cmd = Some(EditCmd::Undo); }
            if icons::menu_action(ui, None, "Rétablir", "Ctrl+Y", has_tab).clicked() { cmd = Some(EditCmd::Redo); }
            ui.separator();
            if icons::menu_action(ui, None, "Couper", "Ctrl+X", has_tab).clicked() { cmd = Some(EditCmd::Cut); }
            if icons::menu_action(ui, None, "Copier", "Ctrl+C", has_tab).clicked() { cmd = Some(EditCmd::Copy); }
            if icons::menu_action(ui, None, "Coller", "Ctrl+V", has_tab).clicked() { cmd = crate::clip::get().map(EditCmd::Paste); ui.close(); }
            if icons::menu_action(ui, None, "Tout sélectionner", "Ctrl+A", has_tab).clicked() { cmd = Some(EditCmd::SelectAll); }
            if let Some(c) = cmd {
                if let Some(d) = self.cur_doc() { d.pending_edit = Some(c); }
                ui.close();
            }
            ui.separator();
            if icons::menu_action(ui, Some((Icon::Search, p.cyan)), "Rechercher / Remplacer", &keys["find"], has_tab).clicked() {
                self.find_open = true;
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("find")));
                ui.close();
            }
            if icons::menu_action(ui, None, "Aller à la ligne…", &keys["goto"], has_tab).clicked() { self.dialog = Some(Dialog::Goto(String::new())); ui.close(); }
            if icons::menu_action(ui, None, "Commenter / décommenter", &keys["comment"], has_tab).clicked() {
                if let Some(i) = self.cur() { if let Some(d) = self.tabs[i].doc_mut() { d.pending_comment = true; } }
                ui.close();
            }
        });
        ui.menu_button("Exécution", |ui| {
            ui.set_min_width(330.0);
            let idle = !self.running();
            let stopped = self.debugging() && !self.frames.is_empty();
            let py = self.module.is_some();
            if icons::menu_action(ui, Some((Icon::Run, p.green)), "Exécuter", &keys["run"], idle).clicked() { self.start(false); ui.close(); }
            if py {
                let (ic, c, t) = if stopped { (Icon::Continue, p.green, "Continuer") } else { (Icon::Debug, p.orange, "Déboguer") };
                if icons::menu_action(ui, Some((ic, c)), t, &keys["debug"], idle || stopped).clicked() { if stopped { self.dbg_cmd("continue"); } else { self.start(true); } ui.close(); }
            }
            if icons::menu_action(ui, Some((Icon::Stop, p.red)), "Arrêter", &keys["stop"], !idle).clicked() { self.stop(); ui.close(); }
            if py {
                ui.separator();
                if icons::menu_action(ui, Some((Icon::Over, p.blue)), "Pas à pas", &keys["step_over"], stopped).clicked() { self.dbg_cmd("next"); ui.close(); }
                if icons::menu_action(ui, Some((Icon::Into, p.blue)), "Entrer", &keys["step_into"], stopped).clicked() { self.dbg_cmd("step"); ui.close(); }
                if icons::menu_action(ui, Some((Icon::Outof, p.blue)), "Sortir", &keys["step_out"], stopped).clicked() { self.dbg_cmd("return"); ui.close(); }
                if icons::menu_action(ui, Some((Icon::Pause, p.blue)), "Pause", "", self.debugging() && !stopped).clicked() { self.dbg_cmd("pause"); ui.close(); }
                ui.separator();
                if icons::menu_action(ui, Some((Icon::Bp, p.red)), "Point d'arrêt", &keys["breakpoint"], has_tab).clicked() { self.toggle_bp_here(); ui.close(); }
                if icons::menu_action(ui, None, "Supprimer tous les points d'arrêt", "", true).clicked() {
                    for t in self.tabs.iter_mut() { if let Some(d) = t.doc_mut() { d.breakpoints.clear(); } }
                    ui.close();
                }
            }
            ui.separator();
            if icons::menu_action(ui, None, "Arguments du programme…", "", true).clicked() { self.dialog = Some(Dialog::Args(self.run_args.clone())); ui.close(); }
        });
        ui.menu_button("Outils", |ui| {
            ui.set_min_width(330.0);
            if self.module.is_some() {
                if icons::menu_action(ui, Some((Icon::Libs, p.magenta)), "Bibliothèques", &keys["libs"], true).clicked() {
                    self.libs_open = true;
                    if !self.pkg_loaded { self.pip_list(); }
                    ui.close();
                }
                if icons::menu_action(ui, Some((Icon::Reload, p.green)), "Redémarrer l'interpréteur", "", true).clicked() {
                    self.repl_term.kill();
                    self.repl_term = Terminal::new_repl();
                    self.bottom = Bottom::Repl;
                    self.show_bottom = true;
                    ui.close();
                }
                ui.separator();
            }
            if icons::menu_action(ui, Some((Icon::Server, p.cyan)), "Connexion SSH (labs)", &keys["ssh"], true).clicked() {
                self.show_explorer = true; self.side = SideTab::Ssh; ui.close();
            }
            if icons::menu_action(ui, Some((Icon::Shield, p.green)), "VPN", "", true).clicked() {
                self.show_explorer = true; self.side = SideTab::Vpn; self.refresh_vpns(); ui.close();
            }
        });
        ui.menu_button("Volets", |ui| {
            ui.set_min_width(330.0);
            if icons::menu_action(ui, Some((Icon::SplitR, p.blue)), "Scinder à droite", &keys["split_right"], has_tab).clicked() {
                if let Some(i) = self.cur() { let (u, pa) = (self.tabs[i].uid, self.tabs[i].pane); self.split_tab(u, pa, false); }
                ui.close();
            }
            if icons::menu_action(ui, Some((Icon::SplitD, p.blue)), "Scinder en bas", "", has_tab).clicked() {
                if let Some(i) = self.cur() { let (u, pa) = (self.tabs[i].uid, self.tabs[i].pane); self.split_tab(u, pa, true); }
                ui.close();
            }
            ui.separator();
            ui.menu_button("Pages web", |ui| self.web_menu(ui));
        });
        ui.menu_button("Affichage", |ui| {
            ui.set_min_width(330.0);
            if icons::menu_check(ui, self.show_explorer, "Panneau latéral", &keys["toggle_side"]).clicked() { self.show_explorer = !self.show_explorer; ui.close(); }
            if icons::menu_check(ui, self.show_bottom, "Panneau du bas", &keys["toggle_bottom"]).clicked() { self.show_bottom = !self.show_bottom; ui.close(); }
            if icons::menu_action(ui, Some((Icon::Terminal, p.green)), "Terminal", &keys["terminal"], true).clicked() {
                self.show_bottom = true; self.bottom = Bottom::Terminal(0); ui.close();
            }
            if icons::menu_action(ui, Some((Icon::Terminal, p.green)), "Nouveau terminal", &keys["new_terminal"], true).clicked() {
                self.new_terminal_like(self.bottom); ui.close();
            }
            ui.separator();
            if icons::menu_action(ui, None, "Agrandir le texte", &keys["zoom_in"], true).clicked() { self.zoom(1.0, ctx); }
            if icons::menu_action(ui, None, "Réduire le texte", &keys["zoom_out"], true).clicked() { self.zoom(-1.0, ctx); }
            ui.separator();
            if icons::menu_check(ui, self.cfg.side_right, "Panneau latéral à droite", "").clicked() { self.cfg.side_right = !self.cfg.side_right; self.cfg_dirty = true; ui.close(); }
            if icons::menu_check(ui, self.cfg.bottom_right, "Panneau du bas à droite", "").clicked() { self.cfg.bottom_right = !self.cfg.bottom_right; self.cfg_dirty = true; ui.close(); }
            if icons::menu_check(ui, self.cfg.debug_left, "Débogage à gauche", "").clicked() { self.cfg.debug_left = !self.cfg.debug_left; self.cfg_dirty = true; ui.close(); }
            if icons::menu_check(ui, self.cfg.show_statusbar, "Barre d'état", "").clicked() { self.cfg.show_statusbar = !self.cfg.show_statusbar; self.cfg_dirty = true; ui.close(); }
            ui.separator();
            ui.menu_button("Thème", |ui| {
                ui.set_min_width(220.0);
                for (name, _) in theme::THEMES {
                    if icons::menu_check(ui, self.theme == *name, name, "").clicked() { self.set_theme(name, ctx); ui.close(); }
                }
            });
            ui.separator();
            if icons::menu_action(ui, Some((Icon::Settings, p.fg)), "Paramètres…", &keys["settings"], true).clicked() { self.settings_open = true; ui.close(); }
        });
        ui.menu_button("Aide", |ui| {
            ui.set_min_width(330.0);
            ui.label(RichText::new(format!("StudentCode {VERSION} (Rust)")).strong());
            ui.label(RichText::new(format!("Module PyScripter : {}", if self.module.is_some() { "installé" } else { "non installé" })).color(p.comment));
            ui.label(RichText::new(format!("Module Lecteur PDF : {}", if self.pdf_module { "installé" } else { "non installé" })).color(p.comment));
            ui.separator();
            let k = |id: &str| { let t = &keys[id]; if t.is_empty() { "—".to_string() } else { t.clone() } };
            ui.label(format!("{} exécuter · {} arrêter · {} déboguer / continuer\n{} point d'arrêt · {} / {} / {} pas à pas\n\
                      {} nouveau · {} ouvrir · {} enregistrer · {} fermer\n\
                      {} rechercher · {} commenter · {} aller à la ligne\n\
                      {} panneau latéral · {} panneau du bas · Ctrl+molette zoom\n\
                      {} scinder · glisser un onglet sur le bord d'un volet\n\
                      {} paramètres (thème, polices, raccourcis, disposition…)",
                      k("run"), k("stop"), k("debug"), k("breakpoint"), k("step_over"), k("step_into"), k("step_out"),
                      k("new"), k("open_file"), k("save"), k("close_tab"), k("find"), k("comment"), k("goto"),
                      k("toggle_side"), k("toggle_bottom"), k("split_right"), k("settings")));
            ui.separator();
            if ui.button("Paramètres…").clicked() { self.settings_open = true; ui.close(); }
        });
    }

    /// Barre d'état de la version Python : bloc du mode, état, fichier ; position et Python à droite.
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        egui::Panel::bottom("status").exact_size(28.0).show_separator_line(false)
            .frame(egui::Frame::new().fill(p.bg_dark).inner_margin(egui::Margin::ZERO))
            .show(ui, |ui| {
                let full = ui.max_rect();
                let cy = full.center().y - 1.5;
                let f = FontId::proportional(13.3);
                let bold = FontId::new(13.3, egui::FontFamily::Name("bold".into()));
                let width = |ui: &egui::Ui, t: &str, font: &FontId| ui.fonts_mut(|fo| fo.layout_no_wrap(t.to_string(), font.clone(), p.fg)).size().x;
                // bloc du mode (la version Python affiche « EDITION »)
                let mode = "EDITION";
                let mw = width(ui, mode, &bold) + 2.0 * 8.0;
                let mr = Rect::from_min_size(egui::pos2(full.min.x + 7.0, cy - 8.5), Vec2::new(mw, 17.0));
                ui.painter().rect_filled(mr, 0.0, p.blue);
                ui.painter().text(mr.center(), Align2::CENTER_CENTER, mode, bold.clone(), p.bg_dark);
                let mut x = mr.max.x + 6.0;
                // état de l'exécution, comme la version Python (« ▶ en cours », « ⏸ débogage : à l'arrêt »…)
                let state = if self.debugging() {
                    Some(if self.frames.is_empty() { (Some(Icon::Run), "débogage : en cours", p.orange) } else { (Some(Icon::Pause), "débogage : à l'arrêt", p.yellow) })
                } else if self.repl {
                    Some((None, "⌨ interpréteur actif", p.blue))
                } else if self.running() {
                    Some((Some(Icon::Run), "en cours", p.green))
                } else { None };
                if let Some((ic, t, c)) = state {
                    x += 8.0;
                    if let Some(ic) = ic {
                        icons::paint(ui.painter(), ic, egui::pos2(x + 6.0, cy), 12.0, c);
                        x += 16.0;
                    }
                    let r = ui.painter().text(egui::pos2(x, cy), Align2::LEFT_CENTER, t, bold.clone(), c);
                    x = r.max.x + 6.0;
                }
                // à droite d'abord (version de Python, position, SSH, VPN), pour savoir où s'arrête le texte de gauche
                let seg = format!("Python {}", crate::runner::PY_VERSION.get().map(|v| v.trim_start_matches("Python ").to_string()).unwrap_or_else(|| "…".into()));
                let sw = width(ui, &seg, &f) + 2.0 * 15.0;
                let sr = Rect::from_min_size(egui::pos2(full.max.x - 29.0 - sw, cy - 8.5), Vec2::new(sw, 17.0));
                ui.painter().rect_filled(sr, 0.0, p.bg_hl);
                ui.painter().text(sr.center(), Align2::CENTER_CENTER, &seg, f.clone(), p.blue);
                let mut rx = sr.min.x - 18.0;
                let mut right: Vec<(String, Color32)> = Vec::new();
                if let Some(d) = self.cur().and_then(|i| self.tabs[i].doc()) { right.push((format!("Ln {}, Col {}", d.cursor_line, d.cursor_col), p.fg)); }
                let ssh_on = self.sessions.iter().filter(|s| s.connected).count();
                if ssh_on > 0 { right.push((format!("SSH : {ssh_on}"), p.green)); }
                if self.vpns.iter().any(|v| v.active) { right.push(("VPN".into(), p.green)); }
                for (t, c) in right {
                    let w = width(ui, &t, &f);
                    if rx - w < x + 40.0 { break; }
                    ui.painter().text(egui::pos2(rx, cy), Align2::RIGHT_CENTER, t, f.clone(), c);
                    rx -= w + 18.0;
                }
                let clip = Rect::from_min_max(egui::pos2(x, full.min.y), egui::pos2(rx.max(x), full.max.y));
                let file = match self.cur().map(|i| &self.tabs[i]) {
                    Some(t) => match t.doc().and_then(|d| d.remote.as_ref()) {
                        Some((_, rp, lab)) => format!("SSH {lab} : {rp}"),
                        None => t.path().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "sans titre".into()),
                    },
                    None => String::new(),
                };
                let r = ui.painter().with_clip_rect(clip).text(egui::pos2(x + 15.0, cy), Align2::LEFT_CENTER, &file, f.clone(), p.fg);
                if let Some((msg, t, c)) = self.status.clone() {
                    if t.elapsed().as_secs() < 8 {
                        ui.painter().with_clip_rect(clip).text(egui::pos2(r.max.x + 24.0, cy), Align2::LEFT_CENTER, msg, f.clone(), c);
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                    }
                }
            });
    }

    /// Barre de recherche de la version Python, sous l'éditeur :
    /// Rechercher, ↑, ↓, Remplacer par, Remplacer, Tout, ✕.
    pub(super) fn find_bar(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        let mut act = 0u8; // 1 suivant, 2 précédent, 3 remplacer, 4 tout
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.add_space(6.0);
            let btn_w = |ui: &egui::Ui, t: &str| ui.fonts_mut(|f| f.layout_no_wrap(t.to_string(), FontId::proportional(13.3), p.fg)).size().x + 24.0;
            let fixed = btn_w(ui, "↑") + btn_w(ui, "↓") + btn_w(ui, "Remplacer") + btn_w(ui, "Tout") + btn_w(ui, "✕") + 7.0 * 6.0 + 12.0;
            let field = ((ui.available_width() - fixed) / 2.0).max(80.0);
            let r = icons::line_edit(ui, &p, &mut self.find_text, "Rechercher", egui::Id::new("find"), true, field);
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { act = 1; r.request_focus(); }
            if icons::push_button(ui, &p, None, "↑", false, true, btn_w(ui, "↑"), 28.0).clicked() { act = 2; }
            if icons::push_button(ui, &p, None, "↓", false, true, btn_w(ui, "↓"), 28.0).clicked() { act = 1; }
            let r2 = icons::line_edit(ui, &p, &mut self.replace_text, "Remplacer par", egui::Id::new("replace"), true, field);
            if r2.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { act = 3; r2.request_focus(); }
            if icons::push_button(ui, &p, None, "Remplacer", false, true, btn_w(ui, "Remplacer"), 28.0).clicked() { act = 3; }
            if icons::push_button(ui, &p, None, "Tout", false, true, btn_w(ui, "Tout"), 28.0).clicked() { act = 4; }
            if icons::push_button(ui, &p, None, "✕", false, true, btn_w(ui, "✕"), 28.0).on_hover_text("Fermer (Échap)").clicked() { self.find_open = false; }
        });
        let (ft, rt) = (self.find_text.clone(), self.replace_text.clone());
        let mut info = None;
        if let Some(d) = self.cur_doc() {
            match act {
                1 => d.find_next = true,
                2 => d.find_back = true,
                3 => d.replace_with = Some(rt),
                4 if !ft.is_empty() => {
                    // sans tenir compte des majuscules, comme la recherche
                    let hits = crate::editor::find_all_ci(&d.text, &ft, usize::MAX);
                    let mut out = String::new();
                    let mut last = 0;
                    let n = hits.len();
                    for (b, be) in hits {
                        out.push_str(&d.text[last..b]);
                        out.push_str(&rt);
                        last = be;
                    }
                    out.push_str(&d.text[last.min(d.text.len())..]);
                    d.text = out;
                    info = Some(format!("{n} remplacement(s)"));
                }
                _ => {}
            }
        }
        if let Some(i) = info { self.say(i, p.comment); }
    }

    /// Page d'accueil, présentée comme l'onglet « Accueil » de la version Python.
    pub(super) fn start_page(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        let full = ui.available_rect_before_wrap();
        // barre d'onglets avec l'onglet « Accueil »
        let bar = Rect::from_min_size(full.min, Vec2::new(full.width(), 32.0));
        ui.painter().rect_filled(bar, 0.0, p.bg_dark);
        let font = FontId::proportional(13.3);
        let tw = ui.fonts_mut(|f| f.layout_no_wrap("Accueil".into(), font.clone(), p.fg)).size().x;
        let tab = Rect::from_min_size(bar.min, Vec2::new(tw + 82.0, 32.0));
        ui.painter().rect_filled(tab, 0.0, p.bg);
        ui.painter().rect_filled(Rect::from_min_size(tab.min, Vec2::new(tab.width(), 2.0)), 0.0, p.gutter);
        icons::paint(ui.painter(), Icon::New, egui::pos2(tab.min.x + 21.0, tab.center().y + 1.0), 15.0, p.comment);
        ui.painter().text(egui::pos2(tab.min.x + 38.0, tab.center().y + 1.0), Align2::LEFT_CENTER, "Accueil", font.clone(), p.comment);
        icons::paint(ui.painter(), Icon::Close, egui::pos2(tab.max.x - 12.0, tab.center().y + 1.0), 14.0, p.comment);
        // colonne de 420 px : un tiers de la place au-dessus, deux tiers en dessous
        let body = Rect::from_min_max(egui::pos2(full.min.x, bar.max.y), full.max);
        let content_h = 46.0 + 18.0 + 3.0 * 40.0 + 10.0 + 38.0 + self.bookmarks.len().min(8) as f32 * 40.0;
        let top = body.min.y + ((body.height() - content_h) / 3.0).max(10.0);
        let col = Rect::from_min_size(egui::pos2(body.center().x - 201.0, top), Vec2::new(402.0, content_h));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(col).layout(egui::Layout::top_down(egui::Align::Min)));
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let tex = self.logo.get_or_insert_with(|| {
                let d = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon-256.png")).unwrap_or_default();
                let img = egui::ColorImage::from_rgba_unmultiplied([d.width as usize, d.height as usize], &d.rgba);
                ui.ctx().load_texture("logo", img, egui::TextureOptions::LINEAR)
            }).clone();
            ui.add(egui::Image::new(&tex).fit_to_exact_size(Vec2::splat(46.0)));
            let mut job = egui::text::LayoutJob::default();
            let big = FontId::new(32.0, egui::FontFamily::Name("bold".into()));
            job.append("Student", 0.0, egui::TextFormat::simple(big.clone(), if p.light { p.fg } else { Color32::WHITE }));
            job.append("Code", 0.0, egui::TextFormat::simple(big, Color32::from_rgb(0x2e, 0x7d, 0xe9)));
            ui.label(job);
        });
        ui.add_space(10.0);
        let link = |ui: &mut egui::Ui, ic: Icon, c: Color32, text: &str| -> egui::Response {
            let (r, resp) = ui.allocate_exact_size(Vec2::new(402.0, 32.0), Sense::click());
            ui.painter().rect_filled(r, 5.0, if resp.hovered() { p.bg_hl } else { p.bg });
            icons::paint(ui.painter(), ic, egui::pos2(r.min.x + 21.0, r.center().y), 18.0, c);
            ui.painter().text(egui::pos2(r.min.x + 39.0, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(13.3), p.fg);
            resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        };
        if link(&mut ui, Icon::New, p.fg, "Nouveau fichier Python").clicked() { self.new_doc(); }
        if link(&mut ui, Icon::Open, p.yellow, "Ouvrir un fichier (code, PDF, image)…").clicked() {
            if let Some(f) = rfd::FileDialog::new().set_directory(&self.root).pick_file() { self.open_file(&f); }
        }
        if link(&mut ui, Icon::Open, p.yellow, "Ouvrir un dossier de projet…").clicked() { self.open_folder_dialog(); }
        ui.add_space(10.0);
        let r = ui.horizontal(|ui| icons::line_edit(ui, &p, &mut self.start_url, "Adresse web ou recherche, puis Entrée…",
                                                    egui::Id::new("start_url"), true, 402.0)).inner;
        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.start_url.trim().is_empty() {
            let t = self.start_url.trim().to_string();
            let url = if t.contains("://") { t } else if t.contains('.') && !t.contains(' ') { format!("https://{t}") }
                      else { format!("https://duckduckgo.com/?q={}", t.replace(' ', "+")) };
            let _ = open::that_detached(&url);
            self.start_url.clear();
        }
        for (name, url) in self.bookmarks.clone().into_iter().take(8) {
            if link(&mut ui, Icon::Globe, p.cyan, &name).on_hover_text(&url).clicked() { let _ = open::that_detached(&url); }
        }
    }

    pub(super) fn bottom_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        ui.set_min_height(ui.available_height());
        // onglets comme la version Python : Sortie, Interpréteur Python, Terminal, puis les terminaux SSH
        // (groupes réordonnables en les glissant)
        let mut tabs: Vec<(Bottom, String, Icon, Color32)> = Vec::new();
        let mut groups: Vec<&'static str> = Vec::new();
        for g in self.cfg.bottom_order.clone() {
            let before = tabs.len();
            match g.as_str() {
                "out" => tabs.push((Bottom::Output, "Sortie".into(), Icon::Output, p.green)),
                "repl" => if crate::runner::python().is_some() {
                    tabs.push((Bottom::Repl, "Interpréteur Python".into(), Icon::Python, p.blue));
                },
                "term" => for n in 0..self.terms.len() {
                    tabs.push((Bottom::Terminal(n), if n == 0 { "Terminal".into() } else { format!("Terminal {}", n + 1) }, Icon::Terminal, p.green));
                },
                _ => {
                    let mut any_ssh = false;
                    for (i, s) in self.sessions.iter().enumerate() {
                        for (n, t) in s.terms.iter().enumerate() {
                            if n == 0 && !(s.connected || t.running()) { continue; }
                            any_ssh = true;
                            let name = s.conn.lab.label();
                            let label = if n == 0 { format!("SSH {name}") } else { format!("SSH {name} {}", n + 1) };
                            tabs.push((Bottom::Ssh(i, n), label, Icon::Server, if s.connected && (n == 0 || t.running()) { p.green } else { p.comment }));
                        }
                    }
                    if !any_ssh { tabs.push((Bottom::SshNone, "Terminal SSH".into(), Icon::Server, p.comment)); }
                }
            }
            let key: &'static str = match g.as_str() { "out" => "out", "repl" => "repl", "term" => "term", _ => "ssh" };
            for _ in before..tabs.len() { groups.push(key); }
        }
        if !tabs.iter().any(|t| t.0 == self.bottom) { self.bottom = Bottom::Terminal(0); }
        let bar_h = 31.0;
        let bar = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), bar_h));
        ui.painter().rect_filled(bar.expand2(Vec2::new(0.0, 3.0)).translate(Vec2::new(0.0, -3.0)), 0.0, p.bg_dark);
        let mut act: Option<(u8, Bottom)> = None; // 0 = afficher, 1 = nouveau comme, 2 = fermer
        // fond de la barre déclaré avant les onglets : un clic sur un onglet reste à l'onglet
        let rest = ui.interact(bar, ui.id().with("bottom_bar_bg"), Sense::click());
        let mut rects: Vec<(String, Rect, egui::Response)> = Vec::new();
        // panneau étroit (à droite) : les onglets défilent avec la molette
        let mut tabs_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar));
        egui::ScrollArea::horizontal().id_salt("bottom_tabs").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut tabs_ui, |ui| ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (k, (tab, label, ic, icol)) in tabs.iter().enumerate() {
                let tab = *tab;
                let sel = self.bottom == tab;
                let closable = matches!(tab, Bottom::Terminal(n) | Bottom::Ssh(_, n) if n > 0);
                let font = FontId::proportional(13.3);
                let tw = ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), font.clone(), p.fg)).size().x;
                let w = tw + 52.0 + if closable { 18.0 } else { 0.0 };
                let (r, _) = ui.allocate_exact_size(Vec2::new(w, bar_h), Sense::hover());
                let resp = ui.interact(r, egui::Id::new(("bottom_tab", label.as_str())), Sense::click_and_drag());
                if resp.drag_started() { act = Some((0, tab)); }
                rects.push((groups[k].to_string(), r, resp.clone()));
                if sel {
                    ui.painter().rect_filled(r, 0.0, p.bg);
                    ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(r.width(), 2.0)), 0.0, p.blue);
                }
                let cy = r.center().y + 1.0;
                if *ic == Icon::Python {
                    crate::explorer::file_badge(ui.painter(), egui::pos2(r.min.x + 19.0, cy), "x.py", &p);
                } else {
                    icons::paint(ui.painter(), *ic, egui::pos2(r.min.x + 19.0, cy), 15.0, *icol);
                }
                ui.painter().text(egui::pos2(r.min.x + 37.0, cy), egui::Align2::LEFT_CENTER, label,
                                  font, if sel || resp.hovered() { p.fg } else { p.comment });
                let mut on_close = false;
                if closable {
                    let cb = Rect::from_center_size(egui::pos2(r.max.x - 16.0, cy), Vec2::splat(16.0));
                    on_close = ui.rect_contains_pointer(cb);
                    icons::paint(ui.painter(), Icon::Close, cb.center(), 14.0, if on_close { p.red } else { p.comment });
                }
                if resp.clicked() { act = Some((if on_close { 2 } else { 0 }, tab)); }
                if resp.middle_clicked() && closable { act = Some((2, tab)); }
                resp.context_menu(|ui| {
                    if matches!(tab, Bottom::Terminal(_) | Bottom::Ssh(..)) {
                        let txt = if matches!(tab, Bottom::Ssh(..)) { "Nouveau terminal (même machine)" } else { "Nouveau terminal" };
                        if ui.button(txt).clicked() { act = Some((1, tab)); ui.close(); }
                    }
                    if !matches!(tab, Bottom::Terminal(_)) && ui.button("Nouveau terminal local").clicked() { act = Some((1, Bottom::Terminal(0))); ui.close(); }
                    if closable {
                        ui.separator();
                        if ui.button("Fermer ce terminal").clicked() { act = Some((2, tab)); ui.close(); }
                    }
                    ui.separator();
                    self.bottom_place_menu(ui);
                });
            }
        }));
        ui.allocate_rect(bar, Sense::hover());
        // un groupe d'onglets glissé sur un autre change de place
        let mut grp: Vec<(String, Rect, egui::Response)> = Vec::new();
        for (g, r, resp) in rects {
            match grp.iter_mut().find(|x| x.0 == g) {
                Some(x) => { x.1 = x.1.union(r); if resp.dragged() { x.2 = resp; } }
                None => grp.push((g, r, resp)),
            }
        }
        if drag_reorder(ui, &mut self.cfg.bottom_order, &grp) { self.cfg_dirty = true; }
        // clic droit dans la barre, à côté des onglets
        rest.context_menu(|ui| {
            if ui.button("Nouveau terminal").clicked() { act = Some((1, Bottom::Terminal(0))); ui.close(); }
            let names: Vec<(usize, String)> = self.sessions.iter().enumerate().filter(|(_, s)| s.connected).map(|(i, s)| (i, s.conn.lab.label())).collect();
            for (i, name) in names {
                if ui.button(format!("Nouveau terminal SSH {name}")).clicked() { act = Some((1, Bottom::Ssh(i, 0))); ui.close(); }
            }
            ui.separator();
            self.bottom_place_menu(ui);
        });
        match act {
            Some((0, t)) => self.bottom = t,
            Some((1, t)) => self.new_terminal_like(t),
            Some((2, t)) => self.close_terminal(t),
            _ => {}
        }
        let font = FontId::monospace(if self.cfg.term_font_size > 0.0 { self.cfg.term_font_size } else { self.font_size - 0.5 });
        let root = self.root.clone();
        ui.add_space(4.0);
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(ui.available_rect_before_wrap().shrink2(Vec2::new(6.0, 0.0))));
        let ui = &mut ui;
        match self.bottom {
            Bottom::Terminal(n) => { if let Some(t) = self.terms.get_mut(n) { t.ui(ui, &p, &font, &root); } }
            Bottom::Repl => self.repl_term.ui(ui, &p, &font, &root),
            Bottom::SshNone => {
                ui.add_space(4.0);
                ui.label(RichText::new("Connecte-toi à une machine (onglet SSH à gauche) : son terminal s'ouvrira ici.").color(p.comment).monospace());
            }
            Bottom::Output => self.output_view(ui),
            Bottom::Ssh(i, n) => {
                if let Some(t) = self.sessions.get_mut(i).and_then(|s| s.terms.get_mut(n)) { t.ui(ui, &p, &font, &root); }
            }
        }
    }

    /// Fin des menus du panneau du bas : le placer en bas ou à droite, le masquer.
    fn bottom_place_menu(&mut self, ui: &mut egui::Ui) {
        let right = self.cfg.bottom_right;
        if ui.button(if right { "Mettre le panneau en bas" } else { "Mettre le panneau à droite" }).clicked() {
            self.cfg.bottom_right = !right; self.cfg_dirty = true; ui.close();
        }
        if ui.button(format!("Masquer le panneau  ({})", self.cfg.shortcut_text(crate::settings::Action::ToggleBottom))).clicked() {
            self.show_bottom = false; ui.close();
        }
        if ui.button("Ordre par défaut des onglets").clicked() {
            self.cfg.bottom_order = crate::settings::Settings::default().bottom_order; self.cfg_dirty = true; ui.close();
        }
    }

    /// Onglet « Sortie » de la version Python : texte du programme, puis la ligne de saisie
    /// (entrée du programme, ou interpréteur après l'exécution).
    fn output_view(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        let font = FontId::monospace(if self.cfg.term_font_size > 0.0 { self.cfg.term_font_size } else { self.font_size - 0.5 });
        let input_h = 34.0;
        let h = (ui.available_height() - input_h).max(30.0);
        egui::ScrollArea::vertical().id_salt("output").max_height(h).min_scrolled_height(h).auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
            let mut job = egui::text::LayoutJob::default();
            for (t, k) in &self.output {
                let col = match k { OutKind::Normal => p.fg, OutKind::Error => p.red, OutKind::Info => p.comment, OutKind::Echo => p.cyan, OutKind::Warn => p.yellow, OutKind::Debug => p.magenta };
                job.append(t, 0.0, egui::TextFormat::simple(font.clone(), col));
            }
            let r = ui.add(egui::Label::new(job).selectable(true).wrap());
            r.context_menu(|ui| {
                if ui.button("Effacer").clicked() { self.output.clear(); ui.close(); }
            });
        });
        let hint = if self.repl { ">>> interpréteur actif : teste ton code ici, ex. Message([2, 3])" }
                   else { "Entrée du programme (stdin) — Entrée pour envoyer" };
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::hover());
        let id = egui::Id::new("out_input");
        let foc = ui.memory(|m| m.has_focus(id));
        ui.painter().rect_filled(r, 5.0, p.bg);
        ui.painter().rect_stroke(r, 5.0, Stroke::new(1.0, if foc { p.blue } else { p.gutter }), egui::StrokeKind::Inside);
        let tr = r.shrink2(Vec2::new(9.0, 3.0));
        let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(tr).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let te = cu.add(egui::TextEdit::singleline(&mut self.out_input).id(id).frame(egui::Frame::NONE)
            .font(font.clone()).hint_text(RichText::new(hint).font(font.clone()).color(p.comment)).desired_width(tr.width()));
        if te.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            self.send_input();
            te.request_focus();
        }
    }

    /// Panneau de débogage de la version Python : VARIABLES (Nom / Type / Valeur), champ « Évaluer »,
    /// PILE D'APPELS.
    pub(super) fn debug_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        ui.spacing_mut().item_spacing.y = 0.0;
        let stopped = !self.frames.is_empty();
        let total_h = ui.available_height();
        let title = |ui: &mut egui::Ui, ic: Icon, text: &str| {
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
            icons::paint(ui.painter(), ic, egui::pos2(r.min.x + 17.0, r.center().y + 1.0), 15.0, p.comment);
            ui.painter().text(egui::pos2(r.min.x + 31.0, r.center().y + 1.0), Align2::LEFT_CENTER, text,
                              FontId::new(13.3, egui::FontFamily::Name("bold".into())), p.comment);
        };
        title(ui, Icon::Debug, "VARIABLES");
        // en-tête des colonnes
        let (hr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
        let cols = [hr.min.x + 4.0, hr.min.x + 134.0, hr.min.x + 224.0];
        for (x, t) in cols.iter().zip(["Nom", "Type", "Valeur"]) {
            ui.painter().text(egui::pos2(*x + 4.0, hr.center().y), Align2::LEFT_CENTER, t, FontId::proportional(13.3), p.comment);
        }
        let vars_h = (total_h * 0.72 - 28.0 - 24.0 - 34.0).max(60.0);
        egui::ScrollArea::vertical().id_salt("dbg_vars").max_height(vars_h).min_scrolled_height(vars_h).auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if stopped {
                let locals = self.locals.clone();
                let globals = self.globals.clone();
                for (name, list, open) in [("Locales", &locals, true), ("Globales", &globals, locals.is_empty())] {
                    if list.is_empty() { continue; }
                    let id = ui.make_persistent_id(("vars_group", name));
                    let mut st = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, open);
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::click());
                    crate::explorer::branch_arrow(ui.painter(), egui::pos2(r.min.x + 10.0, r.center().y), st.is_open(), p.comment);
                    ui.painter().text(egui::pos2(r.min.x + 22.0, r.center().y), Align2::LEFT_CENTER, name, FontId::monospace(self.font_size - 1.0), p.comment);
                    if resp.clicked() { st.toggle(ui); }
                    let open_now = st.is_open();
                    st.store(ui.ctx());
                    if open_now { for v in list.iter() { self.var_row(ui, v, 1, cols); } }
                }
            }
        });
        // évaluer une expression à l'arrêt
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            ui.add_space(3.0);
            let w = ui.available_width() - 3.0;
            let r = icons::line_edit(ui, &p, &mut self.dbg_eval, "Évaluer une expression à l'arrêt (ex : len(suite))",
                                     egui::Id::new("dbg_eval"), stopped, w);
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.dbg_eval.trim().is_empty() && stopped {
                let e = self.dbg_eval.trim().to_string();
                self.out(&format!("(debug) {e}\n"), OutKind::Debug);
                self.bottom = Bottom::Output;
                self.show_bottom = true;
                let fi = self.frame_idx;
                if let Some(s) = self.dbg.as_mut() { s.eval(fi, &e); }
                self.dbg_eval.clear();
            }
        });
        ui.add_space(3.0);
        title(ui, Icon::Output, "PILE D'APPELS");
        let mut pick = None;
        egui::ScrollArea::vertical().id_salt("dbg_stack").auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (k, f) in self.frames.iter().enumerate() {
                let name = Path::new(&f.file).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
                if k == self.frame_idx { ui.painter().rect_filled(r, 0.0, p.bg_hl); }
                ui.painter().text(egui::pos2(r.min.x + 6.0, r.center().y), Align2::LEFT_CENTER,
                                  format!("{}   {}:{}", f.func, name, f.line), FontId::monospace(self.font_size - 1.0), p.fg);
                if resp.clicked() { pick = Some(k); }
            }
        });
        if let Some(k) = pick { self.children.clear(); self.pending_children.clear(); self.show_frame(k); }
    }

    /// Ligne de variable : nom en bleu, type en gris, valeur (verte pour une chaîne), dépliable.
    fn var_row(&mut self, ui: &mut egui::Ui, v: &Var, depth: usize, cols: [f32; 3]) {
        let p = self.p;
        let mono = FontId::monospace(self.font_size - 1.0);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::click());
        let indent = depth as f32 * 14.0;
        let id = ui.make_persistent_id(("var", &v.expr));
        let mut st = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        if v.expandable && depth < 7 {
            crate::explorer::branch_arrow(ui.painter(), egui::pos2(r.min.x + indent + 10.0 - 14.0 + 14.0, r.center().y), st.is_open(), p.comment);
        }
        let clip = |x0: f32, x1: f32| Rect::from_min_max(egui::pos2(x0, r.min.y), egui::pos2(x1, r.max.y));
        ui.painter().with_clip_rect(clip(r.min.x, cols[1])).text(egui::pos2(r.min.x + indent + 22.0, r.center().y), Align2::LEFT_CENTER, &v.name, mono.clone(), p.blue);
        ui.painter().with_clip_rect(clip(cols[1], cols[2])).text(egui::pos2(cols[1] + 4.0, r.center().y), Align2::LEFT_CENTER, &v.ty, mono.clone(), p.comment);
        ui.painter().with_clip_rect(clip(cols[2], r.max.x)).text(egui::pos2(cols[2] + 4.0, r.center().y), Align2::LEFT_CENTER, &v.repr, mono,
                                                                if v.ty.starts_with("str") { p.green } else { p.fg });
        let resp = resp.on_hover_text(&v.repr);
        if v.expandable && depth < 7 && resp.clicked() { st.toggle(ui); }
        let open = st.is_open();
        st.store(ui.ctx());
        if open && v.expandable {
            match self.children.get(&v.expr).cloned() {
                Some(items) => for c in &items { self.var_row(ui, c, depth + 1, cols); },
                None => {
                    if !self.pending_children.values().any(|e| e == &v.expr) {
                        if let Some(s) = self.dbg.as_mut() {
                            let id = s.request_children(self.frame_idx, &v.expr);
                            self.pending_children.insert(id, v.expr.clone());
                        }
                    }
                }
            }
        }
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else { return };
        let p = self.p;
        let mut keep = true;
        let enter = |ui: &egui::Ui, r: &egui::Response| (r.lost_focus() || r.has_focus()) && ui.input(|i| i.key_pressed(Key::Enter));
        let mut win = |title: &str, body: &mut dyn FnMut(&mut egui::Ui) -> bool| {
            egui::Window::new(title).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| { ui.set_min_width(360.0); if !body(ui) { keep = false; } });
        };
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let is_file = matches!(d, Dialog::NewFile(..));
        match &mut d {
            Dialog::NewFile(dir, name) | Dialog::NewDir(dir, name) => {
                let dir = dir.clone();
                let mut result: Option<String> = None;
                win(if is_file { "Nouveau fichier" } else { "Nouveau dossier" }, &mut |ui| {
                    ui.label(RichText::new(format!("dans {}", dir.display())).color(p.comment).size(12.0));
                    let r = ui.text_edit_singleline(name);
                    if ui.memory(|m| m.focused().is_none()) { r.request_focus(); }
                    let ok = enter(ui, &r);
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("Créer").clicked() || ok { result = Some(name.clone()); open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if let Some(n) = result.filter(|n| !n.trim().is_empty()) {
                    let path = dir.join(n.trim());
                    let r = if is_file {
                        if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
                        std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map(|_| ())
                    } else {
                        std::fs::create_dir_all(&path)
                    };
                    match r {
                        Ok(()) => {
                            self.explorer.refresh();
                            self.explorer.expanded.insert(dir.clone());
                            if is_file { self.open_file(&path); }
                        }
                        Err(e) => self.say(format!("Création impossible : {e}"), p.red),
                    }
                }
            }
            Dialog::Rename(path, name) => {
                let path = path.clone();
                let mut result = None;
                win("Renommer", &mut |ui| {
                    let r = ui.text_edit_singleline(name);
                    if ui.memory(|m| m.focused().is_none()) { r.request_focus(); }
                    let ok = enter(ui, &r);
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("Renommer").clicked() || ok { result = Some(name.clone()); open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if let Some(n) = result.filter(|n| !n.trim().is_empty()) {
                    let dst = path.with_file_name(n.trim());
                    if dst.exists() {
                        self.say(format!("« {} » existe déjà.", n.trim()), p.red);
                    } else {
                        match std::fs::rename(&path, &dst) {
                            Ok(()) => { self.retarget(&path, &dst); self.explorer.refresh(); }
                            Err(e) => self.say(format!("Renommage impossible : {e}"), p.red),
                        }
                    }
                }
            }
            Dialog::Delete(path) => {
                let path = path.clone();
                let mut confirm = false;
                win("Supprimer", &mut |ui| {
                    ui.label(format!("Supprimer définitivement « {} » ?", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Supprimer").color(p.red)).clicked() { confirm = true; open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if confirm {
                    let r = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
                    match r {
                        Ok(()) => {
                            let gone: Vec<u64> = self.tabs.iter().filter(|t| t.path().map(|p| p.starts_with(&path)).unwrap_or(false)).map(|t| t.uid).collect();
                            for u in gone {
                                if let Some(i) = self.tabs.iter().position(|t| t.uid == u) { self.close_tab(i, true); }
                            }
                            self.explorer.refresh();
                        }
                        Err(e) => self.say(format!("Suppression impossible : {e}"), p.red),
                    }
                }
            }
            Dialog::CloseUnsaved(uid) => {
                let uid = *uid;
                let i = self.tabs.iter().position(|t| t.uid == uid);
                let name = i.map(|i| self.tabs[i].title()).unwrap_or_default();
                let mut choice = 0;
                win("Modifications non enregistrées", &mut |ui| {
                    ui.label(format!("« {name} » a été modifié."));
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("Enregistrer").clicked() { choice = 1; open = false; }
                        if ui.button("Ne pas enregistrer").clicked() { choice = 2; open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if let Some(i) = i {
                    if choice == 1 && self.save_tab(i) { self.close_tab(i, true); }
                    if choice == 2 { self.close_tab(i, true); }
                }
            }
            Dialog::Quit => {
                let mut choice = 0;
                win("Quitter StudentCode", &mut |ui| {
                    ui.label("Des fichiers ont des modifications non enregistrées.");
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("Tout enregistrer et quitter").clicked() { choice = 1; open = false; }
                        if ui.button("Quitter sans enregistrer").clicked() { choice = 2; open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if choice == 1 {
                    for i in 0..self.tabs.len() { if self.tabs[i].dirty() { self.save_tab(i); } }
                }
                if choice > 0 {
                    self.allow_quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Dialog::Args(a) => {
                let mut done = None;
                win("Arguments du programme", &mut |ui| {
                    ui.label(RichText::new("Passés au programme à chaque exécution (séparés par des espaces).").color(p.comment).size(12.0));
                    let r = ui.text_edit_singleline(a);
                    let ok = enter(ui, &r);
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() || ok { done = Some(a.clone()); open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if let Some(a) = done { self.run_args = a; self.save_session(); }
            }
            Dialog::Goto(a) => {
                let mut done = None;
                win("Aller à la ligne", &mut |ui| {
                    let r = ui.add(egui::TextEdit::singleline(a).hint_text("numéro de ligne"));
                    let ok = enter(ui, &r);
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() || ok { done = a.trim().parse::<usize>().ok(); open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if let Some(l) = done {
                    let ctx = self.ctx.clone();
                    if let Some(d) = self.cur_doc() { d.goto_line(&ctx, l.max(1)); }
                }
            }
            Dialog::Password { lab, pw, save, why } => {
                let mut go = None;
                let title = format!("Mot de passe de {}", lab.label());
                let (l2, why2) = (lab.clone(), why.clone());
                win(&title, &mut |ui| {
                    ui.label(RichText::new(format!("{}@{}", l2.user, l2.host)).color(p.comment));
                    if !why2.is_empty() { ui.label(RichText::new(&why2).color(p.red)); }
                    let r = ui.add(egui::TextEdit::singleline(pw).password(true).hint_text("mot de passe"));
                    if ui.memory(|m| m.focused().is_none()) { r.request_focus(); }
                    ui.checkbox(save, "Retenir le mot de passe");
                    let ok = enter(ui, &r);
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("Se connecter").clicked() || ok { go = Some((pw.clone(), *save)); open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if let Some((pw, save)) = go {
                    let mut lab = lab.clone();
                    lab.save_password = save;
                    if let Some(l) = self.labs.iter_mut().find(|l| l.key() == lab.key()) { l.save_password = save; }
                    ssh::save_labs(&self.labs);
                    self.connect_lab(lab, Some(pw));
                }
            }
            Dialog::LabEdit { index, lab } => {
                let mut done = false;
                let vpn_names: Vec<String> = self.vpns.iter().map(|v| v.name.clone()).collect();
                win(if index.is_some() { "Modifier la machine" } else { "Nouvelle machine" }, &mut |ui| {
                    egui::Grid::new("labgrid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                        ui.label("Nom");
                        ui.add(egui::TextEdit::singleline(&mut lab.name).hint_text("ex. Serveur, Client 1…"));
                        ui.end_row();
                        ui.label("Utilisateur");
                        ui.text_edit_singleline(&mut lab.user);
                        ui.end_row();
                        ui.label("Adresse");
                        ui.add(egui::TextEdit::singleline(&mut lab.host).hint_text("ex. 10.11.0.2"));
                        ui.end_row();
                        ui.label("Port");
                        ui.add(egui::DragValue::new(&mut lab.port).range(1..=65535));
                        ui.end_row();
                        ui.label("VPN à activer avant");
                        egui::ComboBox::from_id_salt("labvpn").selected_text(if lab.vpn.is_empty() { "aucun".to_string() } else { lab.vpn.clone() })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut lab.vpn, String::new(), "aucun");
                                for n in &vpn_names { ui.selectable_value(&mut lab.vpn, n.clone(), n); }
                            });
                        ui.end_row();
                    });
                    ui.checkbox(&mut lab.cleanup, "À la connexion, arrêter les programmes restés d'une séance précédente");
                    ui.checkbox(&mut lab.save_password, "Retenir le mot de passe");
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!lab.host.trim().is_empty() && !lab.user.trim().is_empty(), egui::Button::new("Enregistrer")).clicked() {
                            done = true; open = false;
                        }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if done {
                    lab.host = lab.host.trim().to_string();
                    lab.user = lab.user.trim().to_string();
                    match index { Some(k) if *k < self.labs.len() => self.labs[*k] = lab.clone(), _ => self.labs.push(lab.clone()) }
                    ssh::save_labs(&self.labs);
                    if self.vpns.is_empty() { self.refresh_vpns(); }
                }
            }
            Dialog::RemoteName { session, dir, name, dir_kind } => {
                let mut result = None;
                let (sess, dirc, dk) = (*session, dir.clone(), *dir_kind);
                win(if dk { "Nouveau dossier (machine distante)" } else { "Nouveau fichier (machine distante)" }, &mut |ui| {
                    ui.label(RichText::new(format!("dans {dirc}")).color(p.comment).size(12.0));
                    let r = ui.text_edit_singleline(name);
                    if ui.memory(|m| m.focused().is_none()) { r.request_focus(); }
                    let ok = enter(ui, &r);
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("Créer").clicked() || ok { result = Some(name.clone()); open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if let (Some(n), Some(s)) = (result.filter(|n| !n.trim().is_empty()), self.sessions.get_mut(sess)) {
                    let path = format!("{}/{}", dirc.trim_end_matches('/'), n.trim());
                    s.conn.send(if dk { Req::Mkdir(path.clone()) } else { Req::Touch(path.clone()) });
                    s.expanded.insert(dirc.clone());
                    s.conn.send(Req::List(dirc));
                    if !dk { s.conn.send(Req::Read(path)); }
                }
            }
            Dialog::RemoteRename { session, path, name } => {
                let mut result = None;
                win("Renommer (machine distante)", &mut |ui| {
                    let r = ui.text_edit_singleline(name);
                    if ui.memory(|m| m.focused().is_none()) { r.request_focus(); }
                    let ok = enter(ui, &r);
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("Renommer").clicked() || ok { result = Some(name.clone()); open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if let (Some(n), Some(s)) = (result.filter(|n| !n.trim().is_empty()), self.sessions.get(*session)) {
                    let dir = path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
                    s.conn.send(Req::Rename(path.clone(), format!("{dir}/{}", n.trim())));
                }
            }
            Dialog::RemoteDelete { session, path } => {
                let mut confirm = false;
                let pc = path.clone();
                win("Supprimer (machine distante)", &mut |ui| {
                    ui.label(format!("Supprimer définitivement « {pc} » sur la machine ?"));
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Supprimer").color(p.red)).clicked() { confirm = true; open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if confirm {
                    if let Some(s) = self.sessions.get(*session) { s.conn.send(Req::Remove(path.clone())); }
                }
            }
            Dialog::VpnCreds { vpn: v, user, pw, save, why, then_lab } => {
                let mut go = None;
                let (name, why2) = (v.name.clone(), why.clone());
                win(&format!("Identifiants du VPN {name}"), &mut |ui| {
                    if !why2.is_empty() { ui.label(RichText::new(&why2).color(p.red)); }
                    ui.label(RichText::new("Ceux donnés par RemoteLabz (laisse vide si le fichier .ovpn suffit).").color(p.comment).size(12.0));
                    ui.add(egui::TextEdit::singleline(user).hint_text("identifiant"));
                    let r = ui.add(egui::TextEdit::singleline(pw).password(true).hint_text("mot de passe"));
                    ui.checkbox(save, "Retenir");
                    let ok = enter(ui, &r);
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("Connecter").clicked() || ok { go = Some((user.clone(), pw.clone(), *save)); open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if let Some((u, pw, save)) = go {
                    let mut o = vpn::opts(&v.id);
                    o.user = u.clone();
                    vpn::save_opts(&v.id, &o);
                    self.vpn_creds.insert(v.id.clone(), (u.clone(), pw.clone()));
                    if save { crate::secrets::set(&format!("vpn-{}", v.id), &format!("{u}\n{pw}")); }
                    self.vpn_status = format!("Connexion au VPN {}…", v.name);
                    self.vpn_job = Some((vpn::connect(v, &u, &pw, &self.ctx), then_lab.take(), Some(v.clone())));
                }
            }
            Dialog::Info(title, text) => {
                let (t, x) = (title.clone(), text.clone());
                win(&t, &mut |ui| {
                    ui.add(egui::Label::new(RichText::new(&x)).selectable(true).wrap());
                    !(ui.button("OK").clicked() || escape)
                });
            }
            Dialog::Bookmark(name, url) => {
                let mut done = false;
                win("Site web", &mut |ui| {
                    ui.add(egui::TextEdit::singleline(name).hint_text("nom"));
                    let r = ui.add(egui::TextEdit::singleline(url).hint_text("https://…"));
                    let ok = enter(ui, &r);
                    let mut open = !escape;
                    ui.horizontal(|ui| {
                        if ui.button("Enregistrer").clicked() || ok { done = true; open = false; }
                        if ui.button("Annuler").clicked() { open = false; }
                    });
                    open
                });
                if done && url.contains('.') {
                    let n = if name.trim().is_empty() { url.clone() } else { name.trim().to_string() };
                    self.bookmarks.push((n, url.trim().to_string()));
                    self.save_session();
                }
            }
        }
        if keep && self.dialog.is_none() {
            self.dialog = Some(d);
        }
    }
}
