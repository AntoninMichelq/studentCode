//! Panneau de gauche : Fichiers, SSH (labs et explorateur distant), VPN, Web, Bibliothèques Python.
use super::*;
use crate::explorer::{file_badge, folder_icon};
use crate::icons::{self, Icon};
use egui::{Align2, Rect, Sense, Stroke};

impl App {
    pub(super) fn side_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().button_padding = Vec2::new(7.0, 3.0);
            let mut tabs = vec![(SideTab::Files, "Fichiers"), (SideTab::Ssh, "SSH"), (SideTab::Vpn, "VPN"), (SideTab::Web, "Web")];
            if self.module.is_some() { tabs.push((SideTab::Libs, "Python")); }
            for (t, label) in tabs {
                let sel = self.side == t;
                let w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_string(), FontId::proportional(12.5), p.fg)).size().x + 16.0;
                let r = ui.add(egui::Button::new(RichText::new(label).size(12.5).color(if sel { p.fg } else { p.comment }))
                    .frame(false).min_size(Vec2::new(w, 24.0)));
                if sel { ui.painter().line_segment([r.rect.left_bottom(), r.rect.right_bottom()], Stroke::new(2.0, p.blue)); }
                if r.clicked() {
                    self.side = t;
                    if t == SideTab::Vpn { self.refresh_vpns(); }
                    if t == SideTab::Libs && !self.pkg_loaded { self.pip_list(); }
                }
            }
        });
        ui.add_space(4.0);
        match self.side {
            SideTab::Files => {
                let root = self.root.clone();
                let actions = self.explorer.ui(ui, &root, &p);
                for a in actions { self.explorer_action(a); }
            }
            SideTab::Ssh => self.ssh_panel(ui),
            SideTab::Vpn => self.vpn_panel(ui),
            SideTab::Web => self.web_panel(ui),
            SideTab::Libs => self.libs_panel(ui),
        }
    }

    // ------------------------------------------------------------------ SSH
    fn ssh_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        let (go, r) = ui.horizontal(|ui| ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let b = ui.button("Connecter").clicked();
            let r = ui.add(egui::TextEdit::singleline(&mut self.quick).hint_text("ubuntu@10.11.0.2  ou  Nom = ubuntu@ip")
                .desired_width(ui.available_width()));
            (b || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))), r)
        }).inner).inner;
        let _ = r;
        {
            if go {
                match ssh::quick_parse(&self.quick) {
                    Some(mut lab) => {
                        // lab enregistré pour la prochaine fois (même adresse : on le reprend)
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
                    None => self.say("Adresse attendue : utilisateur@machine (ex. ubuntu@10.11.0.2)", p.yellow),
                }
            }
        }
        ui.add_space(4.0);
        // labs enregistrés
        ui.horizontal(|ui| {
            ui.label(RichText::new("MACHINES").color(p.comment).size(11.5));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+ Ajouter").clicked() { self.dialog = Some(Dialog::LabEdit { index: None, lab: Lab::default() }); }
            });
        });
        let labs = self.labs.clone();
        for (k, lab) in labs.iter().enumerate() {
            let state = self.sessions.iter().find(|s| s.conn.lab.key() == lab.key())
                .map(|s| if s.connected { 2 } else if s.failed.is_some() { 3 } else { 1 }).unwrap_or(0);
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
            if resp.hovered() { ui.painter().rect_filled(rect, 3.0, p.bg_hl.gamma_multiply(0.6)); }
            let col = match state { 2 => p.green, 1 => p.yellow, 3 => p.red, _ => p.comment };
            ui.painter().circle_filled(egui::pos2(rect.min.x + 10.0, rect.center().y), 4.0, col);
            ui.painter().text(egui::pos2(rect.min.x + 22.0, rect.center().y), Align2::LEFT_CENTER, lab.label(), FontId::proportional(13.5), p.fg);
            if !lab.name.is_empty() {
                ui.painter().text(egui::pos2(rect.max.x - 6.0, rect.center().y), Align2::RIGHT_CENTER,
                                  format!("{}@{}", lab.user, lab.host), FontId::proportional(11.0), p.comment);
            }
            if resp.double_clicked() || (resp.clicked() && state == 0) { self.connect_lab(lab.clone(), None); }
            else if resp.clicked() {
                if let Some(i) = self.sessions.iter().position(|s| s.conn.lab.key() == lab.key()) { self.ssh_sel = i; }
            }
            resp.context_menu(|ui| {
                if state == 2 || state == 1 {
                    if ui.button("Déconnecter").clicked() {
                        if let Some(i) = self.sessions.iter().position(|s| s.conn.lab.key() == lab.key()) { self.disconnect(i); }
                        ui.close();
                    }
                } else if ui.button("Connecter").clicked() { self.connect_lab(lab.clone(), None); ui.close(); }
                if ui.button("Modifier…").clicked() { self.dialog = Some(Dialog::LabEdit { index: Some(k), lab: lab.clone() }); ui.close(); }
                if ui.button("Oublier le mot de passe").clicked() { crate::secrets::delete(&lab.key()); self.say("Mot de passe oublié.", p.comment); ui.close(); }
                if ui.button(RichText::new("Supprimer").color(p.red)).clicked() {
                    self.labs.remove(k);
                    ssh::save_labs(&self.labs);
                    ui.close();
                }
            });
        }
        if labs.is_empty() {
            ui.label(RichText::new("Tape l'adresse d'une machine ci-dessus (ex. ubuntu@10.11.0.2), puis Entrée.")
                .color(p.comment).size(12.0));
        }
        if self.sessions.is_empty() { return; }
        ui.separator();
        // une machine connectée par sous-onglet
        ui.horizontal_wrapped(|ui| {
            for (i, s) in self.sessions.iter().enumerate() {
                let sel = i == self.ssh_sel;
                let col = if s.connected { p.green } else if s.failed.is_some() { p.red } else { p.yellow };
                if ui.add(egui::Button::new(RichText::new(s.conn.lab.label()).size(12.5).color(if sel { p.fg } else { p.comment }))
                    .selected(sel)).on_hover_text(format!("{}@{}", s.conn.lab.user, s.conn.lab.host)).clicked() { self.ssh_sel = i; }
                let r = ui.min_rect();
                let _ = (r, col);
            }
        });
        let i = self.ssh_sel.min(self.sessions.len() - 1);
        let (connected, failed, label) = {
            let s = &self.sessions[i];
            (s.connected, s.failed.clone(), s.conn.lab.label())
        };
        ui.horizontal_wrapped(|ui| {
            if connected {
                if icons::button(ui, Icon::Terminal, p.fg, p.bg_hl, "Terminal de cette machine").clicked() {
                    self.show_bottom = true;
                    self.bottom = Bottom::Ssh(i);
                }
                if icons::button(ui, Icon::New, p.fg, p.bg_hl, "Nouveau fichier ici").clicked() {
                    let dir = self.sessions[i].selected_dir();
                    self.dialog = Some(Dialog::RemoteName { session: i, dir, name: String::new(), dir_kind: false });
                }
                if icons::button(ui, Icon::Folder, p.yellow, p.bg_hl, "Envoyer des fichiers de l'ordinateur ici").clicked() {
                    if let Some(files) = rfd::FileDialog::new().set_directory(&self.root).pick_files() {
                        let dir = self.sessions[i].selected_dir();
                        for f in files {
                            let n = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            self.sessions[i].conn.send(Req::Upload(f, format!("{}/{n}", dir.trim_end_matches('/'))));
                        }
                    }
                }
                if ui.small_button("Actualiser").clicked() {
                    let dirs: Vec<String> = self.sessions[i].listing.keys().cloned().collect();
                    for d in dirs { self.sessions[i].conn.send(Req::List(d)); }
                }
            }
            if ui.small_button("Déconnecter").clicked() { self.disconnect(i); }
        });
        if i >= self.sessions.len() { return; }
        if let Some(f) = failed {
            ui.label(RichText::new(f).color(p.red).size(12.5));
            if ui.button("Réessayer").clicked() {
                let lab = self.sessions[i].conn.lab.clone();
                self.disconnect(i);
                self.connect_lab(lab, None);
            }
            return;
        }
        if !connected {
            ui.label(RichText::new(format!("Connexion à {label}…")).color(p.comment));
            return;
        }
        egui::ScrollArea::vertical().id_salt("remote_tree").auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let home = self.sessions[i].conn.home.clone().unwrap_or("/".into());
            self.remote_tree(ui, i, &home, 0);
        });
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
            if e.name.starts_with('.') { continue; }
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::click_and_drag());
            let selected = self.sessions[i].selected.as_deref() == Some(e.path.as_str());
            if selected || resp.hovered() { ui.painter().rect_filled(rect, 3.0, if selected { p.bg_hl } else { p.bg_hl.gamma_multiply(0.6) }); }
            let x = rect.min.x + 6.0 + depth as f32 * 14.0;
            let cy = rect.center().y;
            let open = self.sessions[i].expanded.contains(&e.path);
            if e.dir {
                icons::paint(ui.painter(), if open { Icon::ChevronDown } else { Icon::ChevronRight }, egui::pos2(x + 4.0, cy), 12.0, p.comment);
                folder_icon(ui.painter(), egui::pos2(x + 18.0, cy), p.blue);
            } else {
                file_badge(ui.painter(), egui::pos2(x + 18.0, cy), &e.name, &p);
            }
            ui.painter().text(egui::pos2(x + 30.0, cy), Align2::LEFT_CENTER, &e.name, FontId::proportional(13.5), p.fg);
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
                if ui.button("Ouvrir un terminal ici").clicked() {
                    let d = dir_of.clone();
                    self.show_bottom = true;
                    self.bottom = Bottom::Ssh(i);
                    self.sessions[i].term.write(format!("\x15cd '{}'\r", d.replace('\'', "'\\''")).as_bytes());
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
    fn vpn_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        if let Err(e) = vpn::available() {
            ui.label(RichText::new(e).color(p.yellow));
            return;
        }
        ui.horizontal(|ui| {
            if ui.button("+ Ajouter un VPN (.ovpn)").clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter("OpenVPN", &["ovpn", "conf"]).pick_file() {
                    self.vpn_status = "Import du VPN…".into();
                    self.vpn_job = Some((vpn::import(&f, &self.ctx), None, None));
                }
            }
            if ui.small_button("Actualiser").clicked() { self.refresh_vpns(); }
        });
        ui.add_space(4.0);
        for v in self.vpns.clone() {
            let sel = self.vpn_sel.as_deref() == Some(v.id.as_str());
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
            if sel || resp.hovered() { ui.painter().rect_filled(rect, 3.0, if sel { p.bg_hl } else { p.bg_hl.gamma_multiply(0.6) }); }
            icons::paint(ui.painter(), Icon::Shield, egui::pos2(rect.min.x + 12.0, rect.center().y), 14.0, if v.active { p.green } else { p.comment });
            ui.painter().text(egui::pos2(rect.min.x + 26.0, rect.center().y), Align2::LEFT_CENTER, &v.name, FontId::proportional(13.5), p.fg);
            if v.active {
                ui.painter().text(egui::pos2(rect.max.x - 6.0, rect.center().y), Align2::RIGHT_CENTER, "connecté", FontId::proportional(11.5), p.green);
            }
            if resp.clicked() { self.vpn_sel = Some(v.id.clone()); }
            if resp.double_clicked() {
                if v.active { self.vpn_job = Some((vpn::disconnect(&v, &self.ctx), None, None)); } else { self.vpn_connect(v.clone(), None, false); }
            }
        }
        if self.vpns.is_empty() && self.vpn_list.is_none() {
            ui.label(RichText::new("Aucun VPN : ajoute le fichier .ovpn donné pour le lab (RemoteLabz → OpenVPN).").color(p.comment).size(12.0));
        }
        let Some(v) = self.vpn_sel.as_ref().and_then(|id| self.vpns.iter().find(|v| &v.id == id)).cloned() else {
            ui.label(RichText::new(&self.vpn_status).color(p.comment).size(12.0));
            return;
        };
        ui.separator();
        let busy = self.vpn_job.is_some();
        ui.horizontal(|ui| {
            let label = if v.active { "Déconnecter" } else { "Connecter" };
            if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                if v.active { self.vpn_job = Some((vpn::disconnect(&v, &self.ctx), None, None)); } else { self.vpn_connect(v.clone(), None, false); }
            }
            if ui.add_enabled(!busy && !v.active, egui::Button::new("Identifiants…")).clicked() { self.vpn_connect(v.clone(), None, true); }
            if ui.add_enabled(!busy, egui::Button::new(RichText::new("Supprimer").color(p.red))).clicked() {
                self.vpn_job = Some((vpn::delete(&v, &self.ctx), None, None));
                self.vpn_sel = None;
            }
        });
        let mut o = vpn::opts(&v.id);
        let before = (o.split, o.routes.clone());
        ui.checkbox(&mut o.split, "Garder internet : VPN seulement pour le lab")
            .on_hover_text("Coché : ta connexion internet ne passe pas par le VPN ; seuls les réseaux du lab \
                            (indiqués ci-dessous et ceux des machines SSH liées) passent par lui.");
        if o.split {
            ui.add(egui::TextEdit::singleline(&mut o.routes).hint_text("réseaux du lab, ex : 10.11.0.0/28").desired_width(f32::INFINITY));
            if !o.hosts.is_empty() {
                ui.label(RichText::new(format!("Machines des labs : {}", o.hosts.join(", "))).color(p.comment).size(11.5));
            }
        }
        if (o.split, o.routes.clone()) != before {
            vpn::save_opts(&v.id, &o);
            if v.active { self.vpn_status = "Réglage enregistré : reconnecte le VPN pour l'appliquer.".into(); }
        }
        if busy { ui.horizontal(|ui| { ui.spinner(); ui.label(RichText::new(&self.vpn_status).color(p.comment).size(12.0)); }); }
        else { ui.label(RichText::new(&self.vpn_status).color(p.comment).size(12.0)); }
    }

    // ------------------------------------------------------------------ Web
    fn web_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        ui.label(RichText::new("Les pages s'ouvrent dans ton navigateur (plus léger qu'un navigateur intégré).")
            .color(p.comment).size(12.0));
        ui.add_space(4.0);
        let mut remove = None;
        for (k, (name, url)) in self.bookmarks.clone().into_iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
            if resp.hovered() { ui.painter().rect_filled(rect, 3.0, p.bg_hl.gamma_multiply(0.6)); }
            icons::paint(ui.painter(), Icon::Globe, egui::pos2(rect.min.x + 12.0, rect.center().y), 14.0, p.blue);
            ui.painter().text(egui::pos2(rect.min.x + 26.0, rect.center().y), Align2::LEFT_CENTER, &name, FontId::proportional(13.5), p.fg);
            if resp.clicked() { let _ = open::that_detached(&url); }
            let resp = resp.on_hover_text(&url);
            resp.context_menu(|ui| {
                if ui.button("Ouvrir").clicked() { let _ = open::that_detached(&url); ui.close(); }
                if ui.button("Modifier…").clicked() { self.dialog = Some(Dialog::Bookmark(name.clone(), url.clone())); remove = Some(k); ui.close(); }
                if ui.button("Copier l'adresse").clicked() { ui.ctx().copy_text(url.clone()); ui.close(); }
                if ui.button(RichText::new("Supprimer").color(p.red)).clicked() { remove = Some(k); ui.close(); }
            });
        }
        if let Some(k) = remove { self.bookmarks.remove(k); self.save_session(); }
        ui.add_space(6.0);
        if ui.button("+ Ajouter un site").clicked() { self.dialog = Some(Dialog::Bookmark(String::new(), "https://".into())); }
    }

    // ------------------------------------------------------------------ bibliothèques Python
    fn libs_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        let py = runner::python();
        ui.label(RichText::new(match &py { Some(c) => format!("Python : {}", c.join(" ")), None => runner::PYTHON_MISSING.into() })
            .color(p.comment).size(11.5));
        ui.horizontal(|ui| ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let b = ui.add_enabled(!self.pkg_busy, egui::Button::new("Installer")).clicked();
            let r = ui.add(egui::TextEdit::singleline(&mut self.pkg_name).hint_text("nom de la bibliothèque").desired_width(ui.available_width()));
            let go = b || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
            if go && !self.pkg_name.trim().is_empty() {
                let names = self.pkg_name.split_whitespace().map(String::from).collect();
                self.pip_run(true, names);
                self.pkg_name.clear();
            }
        }));
        if self.pkg_busy { ui.horizontal(|ui| { ui.spinner(); ui.label(RichText::new("pip travaille…").color(p.comment)); }); }
        egui::ScrollArea::vertical().id_salt("pkgs").max_height(ui.available_height() * 0.62).auto_shrink(false).show(ui, |ui| {
            ui.label(RichText::new("SUGGESTIONS").color(p.comment).size(11.5));
            let installed: std::collections::HashSet<String> = self.pkgs.iter().map(|(n, _)| n.to_lowercase()).collect();
            for (name, what) in pip::SUGGESTED {
                ui.horizontal(|ui| {
                    let has = installed.contains(&name.to_lowercase());
                    ui.label(RichText::new(*name).color(if has { p.green } else { p.fg }));
                    ui.label(RichText::new(*what).color(p.comment).size(11.5));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if has { ui.label(RichText::new("installée").color(p.green).size(11.5)); }
                        else if ui.add_enabled(!self.pkg_busy, egui::Button::new("Installer").small()).clicked() { self.pip_run(true, vec![name.to_string()]); }
                    });
                });
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("INSTALLÉES ({})", self.pkgs.len())).color(p.comment).size(11.5));
                ui.add(egui::TextEdit::singleline(&mut self.pkg_filter).hint_text("filtrer").desired_width(100.0));
            });
            let f = self.pkg_filter.to_lowercase();
            for (name, ver) in self.pkgs.clone() {
                if !f.is_empty() && !name.to_lowercase().contains(&f) { continue; }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&name).color(p.fg));
                    ui.label(RichText::new(&ver).color(p.comment).size(11.5));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!self.pkg_busy, egui::Button::new(RichText::new("Retirer").color(p.red)).small()).clicked() {
                            self.pip_run(false, vec![name.clone()]);
                        }
                    });
                });
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("piplog").stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(&self.pkg_log).monospace().size(11.5).color(p.comment)).selectable(true));
        });
        let _ = Rect::NOTHING;
    }
}

impl SshSession {
    fn selected_dir(&self) -> String {
        match &self.selected {
            Some(s) if self.listing.contains_key(s) => s.clone(),
            Some(s) => s.rsplit_once('/').map(|(d, _)| if d.is_empty() { "/".to_string() } else { d.to_string() }).unwrap_or("/".into()),
            None => self.conn.home.clone().unwrap_or("/".into()),
        }
    }
}
