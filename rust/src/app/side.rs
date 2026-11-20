//! Panneau de gauche : Fichiers, SSH (labs et explorateur distant), VPN, Web, Bibliothèques Python.
use super::*;
use crate::icons::{self, Icon};
use egui::{Align2, Rect, Sense, Stroke};

impl App {
    pub(super) fn side_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        ui.spacing_mut().item_spacing.y = 0.0;
        // onglets comme la version Python (QTabBar : icône 15 px, texte, trait bleu en haut de l'onglet actif) ;
        // on peut les réordonner en les glissant, et déplacer le panneau par un clic droit
        let mut rects = Vec::new();
        let mut menu_resp = Vec::new();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for key in self.cfg.side_order.clone() {
                let (t, label, ic, icol) = match key.as_str() {
                    "ssh" => (SideTab::Ssh, "SSH", Icon::Server, p.cyan),
                    "vpn" => (SideTab::Vpn, "VPN", Icon::Shield, p.green),
                    _ => (SideTab::Files, "Fichiers", Icon::Folder, p.blue),
                };
                let sel = self.side == t;
                let font = FontId::proportional(13.3);
                let tw = ui.fonts_mut(|f| f.layout_no_wrap(label.to_string(), font.clone(), p.fg)).size().x;
                let (r, _) = ui.allocate_exact_size(Vec2::new(tw + 52.0, 31.0), Sense::hover());
                let resp = ui.interact(r, egui::Id::new(("side_tab", key.as_str())), Sense::click_and_drag());
                if sel {
                    ui.painter().rect_filled(r, 0.0, p.bg);
                    ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(r.width(), 2.0)), 0.0, p.blue);
                }
                icons::paint(ui.painter(), ic, egui::pos2(r.min.x + 19.0, r.center().y + 1.0), 15.0, icol);
                ui.painter().text(egui::pos2(r.min.x + 38.0, r.center().y + 1.0), Align2::LEFT_CENTER, label, font,
                                  if sel || resp.hovered() { p.fg } else { p.comment });
                if resp.clicked() || resp.drag_started() {
                    self.side = t;
                    if t == SideTab::Vpn { self.refresh_vpns(); }
                }
                rects.push((key.clone(), r, resp.clone()));
                menu_resp.push(resp);
            }
            let rest = ui.available_rect_before_wrap();
            let rest = Rect::from_min_size(rest.min, Vec2::new(rest.width().max(0.0), 31.0));
            menu_resp.push(ui.interact(rest, egui::Id::new("side_tabbar_rest"), Sense::click()));
        });
        if super::ui::drag_reorder(ui, &mut self.cfg.side_order, &rects) { self.cfg_dirty = true; }
        for r in menu_resp { r.context_menu(|ui| self.side_menu(ui)); }
        match self.side {
            SideTab::Files => {
                let root = self.root.clone();
                let actions = self.explorer.ui(ui, &root, &p);
                for a in actions { self.explorer_action(a); }
            }
            SideTab::Ssh => self.ssh_panel(ui),
            SideTab::Vpn => self.vpn_panel(ui),
        }
    }

    /// Clic droit sur les onglets du panneau latéral : le déplacer ou le masquer.
    fn side_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(240.0);
        let right = self.cfg.side_right;
        if ui.button(if right { "Mettre le panneau à gauche" } else { "Mettre le panneau à droite" }).clicked() {
            self.cfg.side_right = !right; self.cfg_dirty = true; ui.close();
        }
        if ui.button(format!("Masquer le panneau  ({})", self.cfg.shortcut_text(crate::settings::Action::ToggleSide))).clicked() {
            self.show_explorer = false; ui.close();
        }
        ui.separator();
        if ui.button("Ordre par défaut des onglets").clicked() {
            self.cfg.side_order = crate::settings::Settings::default().side_order; self.cfg_dirty = true; ui.close();
        }
        if ui.button("Paramètres…").clicked() { self.settings_open = true; self.settings_page = 1; ui.close(); }
    }

    // ------------------------------------------------------------------ SSH
    /// Panneau SSH identique à la version Python : sous-onglets des machines (+ nouvelle connexion),
    /// liste déroulante modifiable avec la prise et « … », chemin distant, arbre, état en bas.
    fn ssh_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        if self.sessions.is_empty() { self.ssh_new = true; }
        self.ssh_sel = self.ssh_sel.min(self.sessions.len().saturating_sub(1));
        let full_h = ui.available_height();
        let top = ui.cursor().min.y;
        // ---- sous-onglets (QTabWidget de la version Python, onglets fermables)
        let mut close: Option<usize> = None;
        let mut new_term: Option<usize> = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(5.0);
            let mut tabs: Vec<(Option<usize>, String, Color32)> = self.sessions.iter().enumerate().map(|(i, s)| {
                (Some(i), s.conn.lab.label(), if s.connected { p.green } else { p.comment })
            }).collect();
            if self.ssh_new { tabs.push((None, "Nouvelle".into(), p.comment)); }
            for (idx, label, col) in tabs {
                let sel = match idx { Some(i) => !self.ssh_new && i == self.ssh_sel, None => self.ssh_new };
                let font = FontId::proportional(13.3);
                let tw = ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), font.clone(), p.fg)).size().x;
                let w = (tw + 70.0).min(ui.available_width() - 34.0).max(70.0);
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
                if sel {
                    ui.painter().rect_filled(r, 0.0, p.bg);
                    ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(r.width(), 2.0)), 0.0, p.blue);
                }
                let cy = r.center().y + 1.0;
                icons::paint(ui.painter(), Icon::Server, egui::pos2(r.min.x + 16.0, cy), 15.0, col);
                let tr = Rect::from_min_max(egui::pos2(r.min.x + 34.0, r.min.y), egui::pos2(r.max.x - 22.0, r.max.y));
                ui.painter().with_clip_rect(tr).text(egui::pos2(tr.min.x, cy), Align2::LEFT_CENTER, &label, font,
                                                     if sel || resp.hovered() { p.fg } else { p.comment });
                let cb = Rect::from_center_size(egui::pos2(r.max.x - 11.0, cy), Vec2::splat(14.0));
                let on_close = ui.rect_contains_pointer(cb);
                icons::paint(ui.painter(), Icon::Close, cb.center(), 14.0, if on_close { p.red } else { p.comment });
                if resp.clicked() {
                    match idx {
                        Some(i) if on_close => close = Some(i),
                        Some(i) => { self.ssh_sel = i; self.ssh_new = false; }
                        None if on_close => { if !self.sessions.is_empty() { self.ssh_new = false; } }
                        None => {}
                    }
                }
                if let Some(i) = idx {
                    resp.context_menu(|ui| {
                        if icons::menu_item(ui, Icon::Terminal, p.green, "Nouveau terminal").clicked() { new_term = Some(i); ui.close(); }
                        if icons::menu_item(ui, Icon::Plug, p.red, "Se déconnecter").clicked() { close = Some(i); ui.close(); }
                    });
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                if icons::sized_button(ui, Icon::New, p.green, p.bg_hl, "Nouvelle connexion SSH (une autre machine en même temps)",
                                       Vec2::new(30.0, 28.0), 18.0).clicked() {
                    self.ssh_new = true;
                    self.quick.clear();
                    ui.ctx().memory_mut(|m| m.request_focus(egui::Id::new("ssh_quick")));
                }
            });
        });
        if let Some(i) = new_term { self.new_ssh_terminal(i, None); }
        if let Some(i) = close { self.disconnect(i); return; }
        let new_panel = self.ssh_new || self.sessions.is_empty();
        let i = self.ssh_sel;
        let connected = !new_panel && self.sessions[i].connected;
        // ---- liste déroulante (modifiable dans « Nouvelle »), prise, « … »
        let mut go = false;
        let mut act_disconnect = false;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(11.0);
            let combo_w = (ui.available_width() - 80.0).max(80.0);
            let (r, _) = ui.allocate_exact_size(Vec2::new(combo_w, 29.0), Sense::hover());
            let quick_focus = ui.memory(|m| m.has_focus(egui::Id::new("ssh_quick")));
            ui.painter().rect_filled(r, 5.0, p.bg);
            ui.painter().rect_stroke(r, 5.0, Stroke::new(1.0, if quick_focus { p.blue } else { p.gutter }), egui::StrokeKind::Inside);
            let arrow = Rect::from_min_max(egui::pos2(r.max.x - 15.0, r.min.y + 1.0), egui::pos2(r.max.x - 1.0, r.max.y - 1.0));
            ui.painter().rect_filled(arrow, egui::CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, p.bg_dark);
            ui.painter().line_segment([arrow.left_top(), arrow.left_bottom()], Stroke::new(1.0, p.gutter));
            let ac = arrow.center();
            ui.painter().add(egui::Shape::convex_polygon(vec![ac + Vec2::new(-3.5, -1.5), ac + Vec2::new(3.5, -1.5), ac + Vec2::new(0.0, 2.5)], p.comment, Stroke::NONE));
            let current_lab = if new_panel {
                self.labs.iter().find(|l| self.quick.trim() == lab_display(l)).cloned()
            } else { Some(self.sessions[i].conn.lab.clone()) };
            let text_x = if current_lab.is_some() { r.min.x + 28.0 } else { r.min.x + 9.0 };
            if current_lab.is_some() {
                icons::paint(ui.painter(), Icon::Server, egui::pos2(r.min.x + 16.0, r.center().y), 15.0, if connected { p.gutter } else { p.cyan });
            }
            let tr = Rect::from_min_max(egui::pos2(text_x, r.min.y + 1.0), egui::pos2(arrow.min.x - 2.0, r.max.y - 1.0));
            if new_panel {
                let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(tr).layout(egui::Layout::left_to_right(egui::Align::Center)));
                let te = cu.add(egui::TextEdit::singleline(&mut self.quick).id(egui::Id::new("ssh_quick"))
                    .hint_text("Serveur = ubuntu@adresse-ip").frame(egui::Frame::NONE).desired_width(tr.width())
                    .font(FontId::proportional(13.3)));
                let te = te.on_hover_text("Tape utilisateur@machine (ex : ubuntu@10.0.3.7) puis Entrée.\nPour lui donner un nom : Serveur = ubuntu@10.0.3.7\nLes machines déjà utilisées restent dans la liste.");
                if te.lost_focus() && ui.input(|k| k.key_pressed(Key::Enter)) { go = true; }
            } else {
                ui.painter().with_clip_rect(tr).text(egui::pos2(tr.min.x, r.center().y), Align2::LEFT_CENTER,
                                                     lab_display(&self.sessions[i].conn.lab), FontId::proportional(13.3), p.fg);
            }
            // flèche : les machines enregistrées
            let ar = ui.interact(arrow, ui.id().with("ssh_combo_arrow"), Sense::click());
            let ar = if new_panel { ar } else { ui.interact(r, ui.id().with("ssh_combo_all"), Sense::click()) };
            egui::Popup::menu(&ar).anchor(r).align(egui::RectAlign::BOTTOM_START).gap(1.0).width(r.width() - 12.0).show(|ui| {
                if self.labs.is_empty() {
                    ui.label(RichText::new("Aucune machine pour l'instant").color(p.comment));
                }
                for lab in self.labs.clone() {
                    if icons::menu_item(ui, Icon::Server, p.cyan, &lab_display(&lab)).clicked() {
                        if new_panel {
                            self.quick = lab_display(&lab);
                        } else {
                            self.connect_lab(lab, None);
                        }
                        ui.close();
                    }
                }
            });
            let plug_tip = if connected { "Se déconnecter" } else { "Se connecter" };
            if icons::sized_button(ui, Icon::Plug, if connected { p.red } else { p.green }, p.bg_hl, plug_tip, Vec2::new(32.0, 29.0), 18.0).clicked() {
                if new_panel { go = true; } else { act_disconnect = true; }
            }
            let more = icons::sized_button(ui, Icon::More, p.fg, p.bg_hl, "Gérer les labs", Vec2::new(32.0, 29.0), 18.0);
            egui::Popup::menu(&more).show(|ui| self.ssh_more_menu(ui, if new_panel { current_lab.clone() } else { Some(self.sessions[i].conn.lab.clone()) }, if new_panel { None } else { Some(i) }));
        });
        if go { self.quick_connect(); return; }
        if act_disconnect { self.disconnect(i); return; }
        // ---- chemin distant : dossier parent, chemin, actualiser
        let mut go_to: Option<String> = None;
        let mut refresh = false;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(11.0);
            let en = connected;
            if icons::sized_button(ui, Icon::Up, p.fg, p.bg_hl, "Dossier parent", Vec2::new(32.0, 29.0), 18.0).clicked() && en {
                let root = self.sessions[i].root.clone();
                let parent = root.trim_end_matches('/').rsplit_once('/').map(|(d, _)| if d.is_empty() { "/".to_string() } else { d.to_string() }).unwrap_or("/".into());
                go_to = Some(parent);
            }
            let w = (ui.available_width() - 34.0 - 7.0).max(60.0);
            let (r, _) = ui.allocate_exact_size(Vec2::new(w, 29.0), Sense::hover());
            let pid = egui::Id::new(("ssh_path", i, new_panel));
            let foc = ui.memory(|m| m.has_focus(pid));
            ui.painter().rect_filled(r, 5.0, p.bg);
            ui.painter().rect_stroke(r, 5.0, Stroke::new(1.0, if foc { p.blue } else { p.gutter }), egui::StrokeKind::Inside);
            let tr = r.shrink2(Vec2::new(8.0, 3.0));
            let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(tr).layout(egui::Layout::left_to_right(egui::Align::Center)));
            if en {
                let te = cu.add(egui::TextEdit::singleline(&mut self.sessions[i].path_edit).id(pid).frame(egui::Frame::NONE)
                    .desired_width(tr.width()).font(FontId::proportional(13.3)));
                if te.lost_focus() && ui.input(|k| k.key_pressed(Key::Enter)) { go_to = Some(self.sessions[i].path_edit.trim().to_string()); }
            } else {
                cu.label(RichText::new("dossier distant").color(p.comment).size(13.3));
            }
            if icons::sized_button(ui, Icon::Reload, p.fg, p.bg_hl, "Actualiser", Vec2::new(32.0, 29.0), 18.0).clicked() && en { refresh = true; }
        });
        if let Some(d) = go_to.filter(|d| !d.is_empty()) {
            let s = &mut self.sessions[i];
            s.root = d.clone();
            s.path_edit = d.clone();
            s.selected = None;
            s.conn.send(Req::List(d));
        }
        if refresh {
            let dirs: Vec<String> = self.sessions[i].listing.keys().cloned().collect();
            for d in dirs { self.sessions[i].conn.send(Req::List(d)); }
        }
        ui.add_space(4.0);
        // ---- arbre (ou message d'erreur), puis l'état tout en bas
        let used = ui.cursor().min.y - top;
        let tree_h = (full_h - used - 22.0).max(30.0);
        let (status, scol) = if new_panel {
            ("Non connecté".to_string(), p.comment)
        } else {
            let s = &self.sessions[i];
            match (&s.failed, s.connected) {
                (Some(f), _) => (format!("Échec : {f}"), p.red),
                (None, true) => {
                    let who = format!("{}@{}", s.conn.lab.user, s.conn.lab.host);
                    (if s.conn.lab.name.trim().is_empty() { format!("● Connecté : {who}") } else { format!("● {} : {who}", s.conn.lab.name) }, p.green)
                }
                (None, false) => (format!("Connexion à {}…", s.conn.lab.label()), p.yellow),
            }
        };
        egui::ScrollArea::vertical().id_salt(("remote_tree", i, new_panel)).max_height(tree_h).min_scrolled_height(tree_h).auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if connected {
                let root = self.sessions[i].root.clone();
                self.remote_tree(ui, i, &root, 0);
            } else if !new_panel && self.sessions[i].failed.is_some() {
                ui.add_space(6.0);
                if ui.button("Réessayer").clicked() {
                    let lab = self.sessions[i].conn.lab.clone();
                    self.disconnect(i);
                    self.connect_lab(lab, None);
                }
            }
        });
        // état (QLabel avec 5 px 8 px de marge, comme la version Python)
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.add(egui::Label::new(RichText::new(status).color(scol).size(13.3)).wrap());
        });
    }

    /// Menu « … » du panneau SSH (mêmes entrées que la version Python).
    fn ssh_more_menu(&mut self, ui: &mut egui::Ui, lab: Option<Lab>, session: Option<usize>) {
        let p = self.p;
        ui.set_min_width(330.0);
        let k = lab.as_ref().and_then(|l| self.labs.iter().position(|x| x.key() == l.key()));
        let has = k.is_some();
        ui.add_enabled_ui(has, |ui| {
            if icons::menu_item(ui, Icon::Edit, p.green, "Renommer cette machine…").clicked() {
                if let Some(k) = k { self.dialog = Some(Dialog::LabEdit { index: Some(k), lab: self.labs[k].clone() }); }
                ui.close();
            }
            if icons::menu_item(ui, Icon::Edit, p.blue, "Options avancées (port, clé, VPN, relais…)").clicked() {
                if let Some(k) = k { self.dialog = Some(Dialog::LabEdit { index: Some(k), lab: self.labs[k].clone() }); }
                ui.close();
            }
            if icons::menu_item(ui, Icon::Trash, p.red, "Retirer cette machine de la liste").clicked() {
                if let Some(k) = k {
                    self.labs.remove(k);
                    ssh::save_labs(&self.labs);
                    if session.is_none() { self.quick.clear(); }
                }
                ui.close();
            }
        });
        ui.separator();
        ui.add_enabled_ui(lab.is_some(), |ui| {
            if icons::menu_item(ui, Icon::Key, p.yellow, "Oublier le mot de passe enregistré").clicked() {
                if let Some(l) = &lab { crate::secrets::delete(&l.key()); self.say("Mot de passe oublié.", p.comment); }
                ui.close();
            }
        });
        ui.add_enabled_ui(session.map(|i| self.sessions[i].connected).unwrap_or(false), |ui| {
            if icons::menu_item(ui, Icon::Terminal, p.green, "Nouveau terminal SSH (dans StudentCode)").clicked() {
                if let Some(i) = session { self.new_ssh_terminal(i, None); }
                ui.close();
            }
        });
        ui.add_enabled_ui(lab.is_some(), |ui| {
            if icons::menu_item(ui, Icon::Terminal, p.green, "Ouvrir un terminal système sur ce lab").clicked() {
                if let Some(l) = &lab { crate::ssh::external_terminal(l); }
                ui.close();
            }
        });
        ui.separator();
        let mut hidden = self.ssh_hidden;
        if ui.checkbox(&mut hidden, "Afficher les fichiers cachés").clicked() {
            self.ssh_hidden = hidden;
            ui.close();
        }
        if session.is_none() {
            ui.separator();
            if icons::menu_item(ui, Icon::NewPlus, p.fg, "Ajouter une machine (port, clé, VPN…)").clicked() {
                self.dialog = Some(Dialog::LabEdit { index: None, lab: Lab::default() });
                ui.close();
            }
        }
    }

    /// Connexion depuis la liste « Nouvelle » : machine enregistrée choisie, ou adresse tapée.
    fn quick_connect(&mut self) {
        let p = self.p;
        if let Some(lab) = self.labs.iter().find(|l| self.quick.trim() == lab_display(l)).cloned() {
            self.quick.clear();
            self.connect_lab(lab, None);
            return;
        }
        match ssh::quick_parse(&self.quick) {
            Some(mut lab) => {
                // machine enregistrée pour la prochaine fois (même adresse : on la reprend)
                if let Some(old) = self.labs.iter().find(|l| l.key() == lab.key()) {
                    if lab.name.is_empty() { lab.name = old.name.clone(); }
                    lab.vpn = old.vpn.clone();
                } else {
                    self.labs.push(lab.clone());
                }
                if let Some(old) = self.labs.iter_mut().find(|l| l.key() == lab.key()) { old.name = lab.name.clone(); }
                ssh::save_labs(&self.labs);
                self.quick.clear();
                self.connect_lab(lab, None);
            }
            None => self.say("Écris l'adresse sous la forme ubuntu@adresse-ip", p.red),
        }
    }

    fn remote_tree(&mut self, ui: &mut egui::Ui, i: usize, dir: &str, depth: usize) {
        let p = self.p;
        let entries = match self.sessions[i].listing.get(dir) {
            Some(v) => v.clone(),
            None => {
                ui.label(RichText::new("…").color(p.comment));
                return;
            }
        };
        for e in entries {
            if e.name.starts_with('.') && !self.ssh_hidden { continue; }
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click_and_drag());
            let selected = self.sessions[i].selected.as_deref() == Some(e.path.as_str());
            if selected { ui.painter().rect_filled(rect, 0.0, p.bg_hl); }
            let x = rect.min.x + depth as f32 * 14.0;
            let cy = rect.center().y;
            let open = self.sessions[i].expanded.contains(&e.path);
            if e.dir {
                crate::explorer::branch_arrow(ui.painter(), egui::pos2(x + 11.0, cy), open, p.comment);
                icons::paint(ui.painter(), Icon::Folder, egui::pos2(x + 31.0, cy), 16.0, p.blue);
            } else {
                crate::explorer::file_badge(ui.painter(), egui::pos2(x + 31.0, cy), &e.name, &p);
            }
            ui.painter().text(egui::pos2(x + 46.0, cy), Align2::LEFT_CENTER, &e.name, FontId::proportional(13.3), p.fg);
            if resp.clicked() {
                self.sessions[i].selected = Some(e.path.clone());
                if e.dir {
                    let s = &mut self.sessions[i];
                    if !s.expanded.remove(&e.path) {
                        s.expanded.insert(e.path.clone());
                        s.conn.send(Req::List(e.path.clone()));
                    }
                } else {
                    self.open_remote(i, &e.path);
                }
            }
            // glisser un fichier sur un dossier distant : le déplacer
            if resp.drag_started() { self.sessions[i].selected = Some(e.path.clone()); ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("rdrag"), e.path.clone())); }
            if e.dir && resp.contains_pointer() && ui.input(|i| i.pointer.any_released()) {
                if let Some(src) = ui.ctx().data_mut(|d| d.remove_temp::<String>(egui::Id::new("rdrag"))) {
                    if src != e.path && !e.path.starts_with(&format!("{src}/")) {
                        let name = src.rsplit('/').next().unwrap_or(&src).to_string();
                        self.sessions[i].conn.send(Req::Rename(src, format!("{}/{name}", e.path)));
                    }
                }
            }
            let path = e.path.clone();
            let dir_of = if e.dir { e.path.clone() } else { dir.to_string() };
            resp.context_menu(|ui| {
                if !e.dir && ui.button("Ouvrir").clicked() { self.open_remote(i, &path); ui.close(); }
                if !e.dir && ui.button("Exécuter sur la machine").clicked() {
                    self.open_remote(i, &path);
                    self.say("Ouvre le fichier puis F5 pour l'exécuter sur la machine.", p.comment);
                    ui.close();
                }
                if ui.button("Nouveau fichier…").clicked() {
                    self.dialog = Some(Dialog::RemoteName { session: i, dir: dir_of.clone(), name: String::new(), dir_kind: false }); ui.close();
                }
                if ui.button("Nouveau dossier…").clicked() {
                    self.dialog = Some(Dialog::RemoteName { session: i, dir: dir_of.clone(), name: String::new(), dir_kind: true }); ui.close();
                }
                if ui.button("Télécharger dans le dossier ouvert").clicked() {
                    let name = path.rsplit('/').next().unwrap_or("fichier").to_string();
                    self.sessions[i].conn.send(Req::Download(path.clone(), self.root.join(name)));
                    self.explorer.refresh();
                    ui.close();
                }
                if ui.button("Nouveau terminal ici").clicked() {
                    self.new_ssh_terminal(i, Some(dir_of.clone()));
                    ui.close();
                }
                if ui.button("Aller ici dans le terminal").clicked() {
                    let d = dir_of.clone();
                    self.show_bottom = true;
                    let n = match self.bottom { Bottom::Ssh(k, n) if k == i => n, _ => 0 };
                    self.bottom = Bottom::Ssh(i, n);
                    if let Some(t) = self.sessions[i].terms.get_mut(n) {
                        t.write(format!("\x15cd '{}'\r", d.replace('\'', "'\\''")).as_bytes());
                    }
                    ui.close();
                }
                if ui.button("Copier le chemin").clicked() { ui.ctx().copy_text(path.clone()); ui.close(); }
                ui.separator();
                if ui.button("Renommer…").clicked() {
                    let name = path.rsplit('/').next().unwrap_or("").to_string();
                    self.dialog = Some(Dialog::RemoteRename { session: i, path: path.clone(), name }); ui.close();
                }
                if ui.button(RichText::new("Supprimer…").color(p.red)).clicked() {
                    self.dialog = Some(Dialog::RemoteDelete { session: i, path: path.clone() }); ui.close();
                }
            });
            if e.dir && open {
                self.remote_tree(ui, i, &e.path, depth + 1);
            }
        }
    }

    fn open_remote(&mut self, i: usize, path: &str) {
        let key = self.sessions[i].conn.lab.key();
        if let Some(k) = self.tabs.iter().position(|t| t.doc().and_then(|d| d.remote.as_ref()).map(|(kk, pp, _)| *kk == key && pp == path).unwrap_or(false)) {
            self.activate(k);
            return;
        }
        self.sessions[i].conn.send(Req::Read(path.to_string()));
    }

    // ------------------------------------------------------------------ VPN
    /// Panneau VPN identique à la version Python : bouton d'ajout, liste, options, Connecter / clé / corbeille.
    fn vpn_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        let full_h = ui.available_height();
        let top = ui.cursor().min.y;
        let avail = vpn::available();
        // ---- Ajouter un VPN (.ovpn) + actualiser
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(11.0);
            let w = ui.available_width() - 34.0 - 9.0;
            if icons::push_button(ui, &p, Some((Icon::New, p.fg)), "Ajouter un VPN (.ovpn)", false, avail.is_ok(), w, 28.0).clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter("OpenVPN", &["ovpn", "conf"]).pick_file() {
                    self.vpn_status = "Import du VPN…".into();
                    self.vpn_job = Some((vpn::import(&f, &self.ctx), None, None));
                }
            }
            if icons::sized_button(ui, Icon::Reload, p.fg, p.bg_hl, "Actualiser", Vec2::new(32.0, 28.0), 18.0).clicked() { self.refresh_vpns(); }
        });
        ui.add_space(4.0);
        // ---- liste (nom, puis « connecté » en deuxième colonne)
        if self.vpn_sel.as_ref().map(|id| !self.vpns.iter().any(|v| &v.id == id)).unwrap_or(true) {
            self.vpn_sel = self.vpns.first().map(|v| v.id.clone());
        }
        let bottom_h = 126.0;
        let list_h = (full_h - (ui.cursor().min.y - top) - bottom_h).max(30.0);
        egui::ScrollArea::vertical().id_salt("vpn_list").max_height(list_h).min_scrolled_height(list_h).auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for v in self.vpns.clone() {
                let sel = self.vpn_sel.as_deref() == Some(v.id.as_str());
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
                if sel { ui.painter().rect_filled(rect.shrink2(Vec2::new(4.0, 0.0)), 0.0, p.bg_hl); }
                icons::paint(ui.painter(), Icon::Shield, egui::pos2(rect.min.x + 17.0, rect.center().y), 16.0, if v.active { p.green } else { p.comment });
                let nr = Rect::from_min_max(egui::pos2(rect.min.x + 32.0, rect.min.y), egui::pos2(rect.min.x + 128.0, rect.max.y));
                ui.painter().with_clip_rect(nr).text(egui::pos2(nr.min.x, rect.center().y), Align2::LEFT_CENTER, &v.name, FontId::proportional(13.3), p.fg);
                let busy_here = self.vpn_job.is_some() && sel;
                let state = if v.active { "connecté" } else if busy_here { "…" } else { "" };
                ui.painter().text(egui::pos2(rect.min.x + 135.0, rect.center().y), Align2::LEFT_CENTER, state, FontId::proportional(13.3),
                                  if v.active { p.green } else { p.comment });
                if resp.clicked() { self.vpn_sel = Some(v.id.clone()); }
                if resp.double_clicked() && self.vpn_job.is_none() {
                    if v.active { self.vpn_job = Some((vpn::disconnect(&v, &self.ctx), None, None)); } else { self.vpn_connect(v.clone(), None, false); }
                }
            }
        });
        let cur = self.vpn_sel.as_ref().and_then(|id| self.vpns.iter().find(|v| &v.id == id)).cloned();
        let busy = self.vpn_job.is_some();
        // ---- options : garder internet, réseaux du lab
        ui.add_space(8.0);
        let mut o = cur.as_ref().map(|v| vpn::opts(&v.id)).unwrap_or_default();
        let before = (o.split, o.routes.clone());
        ui.horizontal(|ui| {
            ui.add_space(15.0);
            icons::check_box(ui, &p, &mut o.split, "Garder internet : VPN seulement pour le lab", cur.is_some())
                .on_hover_text("Coché : seul le réseau du lab passe par le VPN, le reste d'internet passe par ta connexion normale.\nDécoché : tout ton trafic passe par le VPN.");
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(11.0);
            let w = ui.available_width() - 9.0;
            icons::line_edit(ui, &p, &mut o.routes, "réseaux du lab via le VPN, ex : 10.22.0.0/16", egui::Id::new("vpn_routes"),
                             cur.is_some() && o.split, w)
                .on_hover_text("Facultatif. Réseaux ou adresses à joindre par le VPN, séparés par des virgules.\nLes réseaux annoncés par le serveur VPN et les machines de tes labs SSH liés sont ajoutés tout seuls.");
        });
        if let Some(v) = &cur {
            if (o.split, o.routes.clone()) != before {
                vpn::save_opts(&v.id, &o);
                if v.active { self.vpn_status = "Réglage enregistré : reconnecte le VPN pour l'appliquer.".into(); }
            }
        }
        // ---- Connecter / Déconnecter, oublier les identifiants, supprimer
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(11.0);
            let w = ui.available_width() - 2.0 * 34.0 - 9.0;
            let label = if cur.as_ref().map(|v| v.active).unwrap_or(false) { "Déconnecter" } else { "Connecter" };
            if icons::push_button(ui, &p, None, label, true, cur.is_some() && !busy, w, 28.0).clicked() {
                if let Some(v) = cur.clone() {
                    if v.active { self.vpn_job = Some((vpn::disconnect(&v, &self.ctx), None, None)); } else { self.vpn_connect(v, None, false); }
                }
            }
            if icons::sized_button(ui, Icon::Key, if cur.is_some() { p.yellow } else { p.gutter }, p.bg_hl, "Oublier les identifiants enregistrés",
                                   Vec2::new(32.0, 28.0), 18.0).clicked() {
                if let Some(v) = &cur {
                    crate::secrets::delete(&format!("vpn-{}", v.id));
                    self.vpn_creds.remove(&v.id);
                    self.say("Identifiants du VPN oubliés.", p.comment);
                }
            }
            if icons::sized_button(ui, Icon::Trash, if cur.is_some() && !busy { p.red } else { p.gutter }, p.bg_hl, "Supprimer ce VPN",
                                   Vec2::new(32.0, 28.0), 18.0).clicked() && !busy {
                if let Some(v) = &cur {
                    self.vpn_job = Some((vpn::delete(v, &self.ctx), None, None));
                    self.vpn_sel = None;
                }
            }
        });
        // ---- état
        let (text, col) = if let Err(e) = &avail {
            (e.to_string(), p.red)
        } else if busy {
            (self.vpn_status.clone(), p.yellow)
        } else if let Some(names) = Some(self.vpns.iter().filter(|v| v.active).map(|v| v.name.clone()).collect::<Vec<_>>()).filter(|n| !n.is_empty()) {
            (format!("● VPN actif : {}", names.join(", ")), p.green)
        } else if self.vpns.is_empty() && self.vpn_list.is_none() {
            ("Aucun VPN. Clique sur « Ajouter » et choisis le fichier .ovpn fourni par ta fac.".into(), p.comment)
        } else {
            (self.vpn_status.clone(), p.comment)
        };
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.add(egui::Label::new(RichText::new(text).color(col).size(13.3)).wrap());
        });
    }

    // ------------------------------------------------------------------ Web
    /// Menu du globe : les favoris web (ouverts dans le navigateur, plus léger qu'un navigateur intégré).
    pub(super) fn web_menu(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        ui.set_min_width(240.0);
        let mut remove = None;
        for (k, (name, url)) in self.bookmarks.clone().into_iter().enumerate() {
            let r = icons::menu_item(ui, Icon::Globe, p.cyan, &name).on_hover_text(&url);
            if r.clicked() { let _ = open::that_detached(&url); ui.close(); }
            r.context_menu(|ui| {
                if ui.button("Modifier…").clicked() { self.dialog = Some(Dialog::Bookmark(name.clone(), url.clone())); remove = Some(k); ui.close(); }
                if ui.button("Copier l'adresse").clicked() { ui.ctx().copy_text(url.clone()); ui.close(); }
                if ui.button(RichText::new("Supprimer").color(p.red)).clicked() { remove = Some(k); ui.close(); }
            });
        }
        if let Some(k) = remove { self.bookmarks.remove(k); self.save_session(); }
        ui.separator();
        if ui.button("Ajouter un site…").clicked() { self.dialog = Some(Dialog::Bookmark(String::new(), "https://".into())); ui.close(); }
        ui.label(RichText::new("Clic droit sur un site : modifier, supprimer").color(p.comment).size(11.5));
    }

    // ------------------------------------------------------------------ bibliothèques Python
    /// Fenêtre « Bibliothèques Python » : la même que la version Python (installer, filtrer,
    /// liste Bibliothèque / Version, Mettre à jour, Désinstaller, Rafraîchir, journal de pip).
    pub(super) fn libs_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        const POPULAR: &[&str] = &["numpy", "pandas", "matplotlib", "scipy", "scikit-learn", "sympy", "requests",
            "beautifulsoup4", "pillow", "pygame", "seaborn", "networkx", "flask", "fastapi", "django", "pytest",
            "opencv-python", "torch", "tensorflow", "plotly", "tqdm", "openpyxl", "sqlalchemy", "pyserial", "pyyaml",
            "rich", "numba", "jupyter"];
        ui.spacing_mut().item_spacing.y = 6.0;
        let busy = self.pkg_busy;
        // ---- paquet à installer (liste modifiable des bibliothèques courantes) + Installer
        let mut install = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let w = ui.available_width() - 100.0;
            let r = icons::line_edit(ui, &p, &mut self.pkg_name, "Nom du paquet à installer (ex : requests, numpy==2.1)",
                                     egui::Id::new("pkg_name"), true, w);
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { install = true; }
            // flèche de la liste
            let arrow = Rect::from_min_max(egui::pos2(r.rect.max.x - 16.0, r.rect.min.y - 3.0), egui::pos2(r.rect.max.x + 8.0, r.rect.max.y + 3.0));
            let ac = arrow.center();
            ui.painter().add(egui::Shape::convex_polygon(vec![ac + Vec2::new(-3.5, -1.5), ac + Vec2::new(3.5, -1.5), ac + Vec2::new(0.0, 2.5)], p.comment, Stroke::NONE));
            let ar = ui.interact(arrow, ui.id().with("pkg_arrow"), Sense::click());
            egui::Popup::menu(&ar).anchor(r.rect.expand2(Vec2::new(8.0, 3.0))).align(egui::RectAlign::BOTTOM_START).width(w - 12.0).show(|ui| {
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for n in POPULAR {
                        if icons::menu_action(ui, None, n, "", true).clicked() { self.pkg_name = n.to_string(); ui.close(); }
                    }
                });
            });
            if icons::push_button(ui, &p, None, "Installer", true, !busy, 0.0, 30.0).clicked() { install = true; }
        });
        if install && !self.pkg_name.trim().is_empty() && !busy {
            let names = self.pkg_name.split_whitespace().map(String::from).collect();
            self.pip_run(true, names);
            self.pkg_name.clear();
        }
        icons::line_edit(ui, &p, &mut self.pkg_filter, "Filtrer les bibliothèques installées…", egui::Id::new("pkg_filter"), true, 0.0);
        // ---- liste
        let log_h = 170.0;
        let list_h = (ui.available_height() - log_h - 40.0 - 24.0).max(80.0);
        let (hr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
        ui.painter().rect_filled(hr, 0.0, p.bg_dark);
        ui.painter().text(egui::pos2(hr.min.x + 6.0, hr.center().y), Align2::LEFT_CENTER, "Bibliothèque", FontId::proportional(13.3), p.comment);
        ui.painter().text(egui::pos2(hr.min.x + 426.0, hr.center().y), Align2::LEFT_CENTER, "Version", FontId::proportional(13.3), p.comment);
        let f = self.pkg_filter.to_lowercase();
        let mut shown = 0;
        egui::ScrollArea::vertical().id_salt("pkgs").max_height(list_h).min_scrolled_height(list_h).auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (name, ver) in self.pkgs.clone() {
                if !f.is_empty() && !name.to_lowercase().contains(&f) { continue; }
                shown += 1;
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
                let sel = self.pkg_sel.as_deref() == Some(name.as_str());
                if sel { ui.painter().rect_filled(r, 0.0, p.bg_hl); }
                ui.painter().text(egui::pos2(r.min.x + 6.0, r.center().y), Align2::LEFT_CENTER, &name, FontId::proportional(13.3), p.fg);
                ui.painter().text(egui::pos2(r.min.x + 426.0, r.center().y), Align2::LEFT_CENTER, &ver, FontId::proportional(13.3), p.fg);
                if resp.clicked() { self.pkg_sel = Some(name.clone()); }
            }
        });
        // ---- informations + boutons
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let info = if busy { "pip travaille…".to_string() } else { format!("{} bibliothèque(s) installée(s){}", self.pkgs.len(),
                if shown != self.pkgs.len() { format!(", {shown} affichée(s)") } else { String::new() }) };
            let w = ui.available_width() - 3.0 * 6.0 - ["Mettre à jour", "Désinstaller", "Rafraîchir"].iter()
                .map(|t| ui.fonts_mut(|fo| fo.layout_no_wrap(t.to_string(), FontId::proportional(13.3), p.fg)).size().x + 24.0).sum::<f32>();
            let (ir, _) = ui.allocate_exact_size(Vec2::new(w.max(10.0), 30.0), Sense::hover());
            ui.painter().with_clip_rect(ir).text(egui::pos2(ir.min.x, ir.center().y), Align2::LEFT_CENTER, info, FontId::proportional(13.3), p.comment);
            let sel = self.pkg_sel.clone();
            for (t, act) in [("Mettre à jour", 1), ("Désinstaller", 2), ("Rafraîchir", 3)] {
                let bw = ui.fonts_mut(|fo| fo.layout_no_wrap(t.to_string(), FontId::proportional(13.3), p.fg)).size().x + 24.0;
                if icons::push_button(ui, &p, None, t, false, !busy, bw, 30.0).clicked() {
                    match (act, &sel) {
                        (1, Some(n)) => self.pip_run(true, vec![n.clone()]),
                        (2, Some(n)) => { self.pip_run(false, vec![n.clone()]); self.pkg_sel = None; }
                        (3, _) => self.pip_list(),
                        _ => self.say("Choisis d'abord une bibliothèque dans la liste.", p.yellow),
                    }
                }
            }
        });
        // ---- journal de pip
        let (lr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ui.available_height().min(log_h)), Sense::hover());
        ui.painter().rect_filled(lr, 0.0, p.bg);
        let mut lu = ui.new_child(egui::UiBuilder::new().max_rect(lr.shrink(4.0)));
        egui::ScrollArea::vertical().id_salt("piplog").stick_to_bottom(true).auto_shrink(false).show(&mut lu, |ui| {
            ui.add(egui::Label::new(RichText::new(&self.pkg_log).monospace().size(self.font_size - 1.0).color(p.fg)).selectable(true));
        });
    }
}

impl SshSession {
    fn selected_dir(&self) -> String {
        match &self.selected {
            Some(s) if self.listing.contains_key(s) => s.clone(),
            Some(s) => s.rsplit_once('/').map(|(d, _)| if d.is_empty() { "/".to_string() } else { d.to_string() }).unwrap_or("/".into()),
            None if !self.root.is_empty() => self.root.clone(),
            None => self.conn.home.clone().unwrap_or("/".into()),
        }
    }
}

/// Nom affiché d'une machine dans la liste (« Serveur — 10.11.0.2 », ou « ubuntu@10.11.0.2 »).
pub(crate) fn lab_display(l: &Lab) -> String {
    if l.name.trim().is_empty() { format!("{}@{}", l.user, l.host) } else { format!("{} — {}", l.name, l.host) }
}
