//! Barres du haut et du bas, panneau du bas (terminaux, sortie), débogueur, boîtes de dialogue.
use super::*;
use crate::icons::{self, Icon};
use egui::{Align2, Sense, Stroke};

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

    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = self.p;
        egui::Panel::top("top").exact_size(36.0)
            .frame(egui::Frame::new().fill(p.bg_dark).inner_margin(egui::Margin::symmetric(6, 2)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let menu = icons::button(ui, Icon::Menu, p.fg, p.bg_hl, "Menu");
                    egui::Popup::menu(&menu).show(|ui| self.main_menu(ui, ctx));
                    ui.add_space(4.0);
                    if icons::button(ui, Icon::New, p.fg, p.bg_hl, "Nouveau fichier (Ctrl+N)").clicked() { self.new_doc(); }
                    if icons::button(ui, Icon::Folder, p.yellow, p.bg_hl, "Ouvrir un dossier (Ctrl+O)").clicked() { self.open_folder_dialog(); }
                    if icons::button(ui, Icon::Save, p.blue, p.bg_hl, "Enregistrer (Ctrl+S)").clicked() {
                        if let Some(i) = self.cur() { self.save_tab(i); }
                    }
                    ui.separator();
                    let stopped = self.debugging() && !self.frames.is_empty();
                    if icons::button(ui, if stopped { Icon::Continue } else { Icon::Run }, p.green, p.bg_hl,
                                     if stopped { "Continuer (F5)" } else { "Exécuter (F5)" }).clicked() {
                        self.start(false);
                    }
                    // boutons qui n'apparaissent que lorsqu'ils servent
                    if self.module.is_some() && self.is_python() && !self.debugging()
                        && icons::button(ui, Icon::Debug, p.orange, p.bg_hl, "Déboguer pas à pas (F6)").clicked() {
                        self.start(true);
                    }
                    if stopped {
                        if icons::button(ui, Icon::StepOver, p.blue, p.bg_hl, "Pas à pas principal (F10)").clicked() { self.dbg_cmd("next"); }
                        if icons::button(ui, Icon::StepInto, p.blue, p.bg_hl, "Entrer dans la fonction (F11)").clicked() { self.dbg_cmd("step"); }
                        if icons::button(ui, Icon::StepOut, p.blue, p.bg_hl, "Sortir de la fonction (Maj+F11)").clicked() { self.dbg_cmd("return"); }
                    }
                    if self.running() && icons::button(ui, Icon::Stop, p.red, p.bg_hl, "Arrêter (Maj+F5)").clicked() { self.stop(); }
                    ui.separator();
                    if icons::button(ui, Icon::Search, p.fg, p.bg_hl, "Rechercher (Ctrl+F)").clicked() {
                        self.find_open = !self.find_open;
                        ctx.memory_mut(|m| m.request_focus(egui::Id::new("find")));
                    }
                    if !self.tabs.is_empty() && icons::button(ui, Icon::Split, p.fg, p.bg_hl,
                        "Séparer : l'onglet passe dans un volet à droite (Ctrl+\\). On peut aussi glisser un onglet sur le bord d'un volet.").clicked() {
                        if let Some(i) = self.cur() { let (u, pa) = (self.tabs[i].uid, self.tabs[i].pane); self.split_tab(u, pa, false); }
                    }
                    if icons::button(ui, Icon::Terminal, if self.show_bottom { p.blue } else { p.fg }, p.bg_hl, "Panneau du bas (Ctrl+J)").clicked() {
                        self.show_bottom = !self.show_bottom;
                    }
                    if icons::button(ui, Icon::Sidebar, if self.show_explorer { p.blue } else { p.fg }, p.bg_hl, "Panneau de gauche (Ctrl+B)").clicked() {
                        self.show_explorer = !self.show_explorer;
                    }
                    // zone libre : déplacer la fenêtre, double-clic pour agrandir
                    let right_w = 98.0;
                    let free = ui.available_width() - right_w;
                    if free > 20.0 {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(free, 30.0), Sense::click_and_drag());
                        ui.painter().text(rect.center(), Align2::CENTER_CENTER, &self.title, FontId::proportional(13.0), p.comment);
                        if resp.drag_started() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                        if resp.double_clicked() {
                            let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                        }
                    }
                    if icons::button(ui, Icon::Minimize, p.fg, p.bg_hl, "Réduire").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    if icons::button(ui, Icon::Maximize, p.fg, p.bg_hl, "Agrandir").clicked() {
                        let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                    }
                    if icons::button(ui, Icon::Close, p.fg, p.red.gamma_multiply(0.5), "Fermer").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
    }

    fn main_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.set_min_width(240.0);
        ui.menu_button("Fichier", |ui| {
            if ui.button("Nouveau fichier          Ctrl+N").clicked() { self.new_doc(); ui.close(); }
            if ui.button("Ouvrir un dossier…       Ctrl+O").clicked() { self.open_folder_dialog(); ui.close(); }
            if ui.button("Ouvrir un fichier…").clicked() {
                if let Some(f) = rfd::FileDialog::new().set_directory(&self.root).pick_file() { self.open_file(&f); }
                ui.close();
            }
            ui.menu_button("Dossiers récents", |ui| {
                for r in self.recent.clone() {
                    if ui.button(r.to_string_lossy()).clicked() { self.set_root(r); self.save_session(); ui.close(); }
                }
            });
            ui.separator();
            if ui.button("Enregistrer              Ctrl+S").clicked() { if let Some(i) = self.cur() { self.save_tab(i); } ui.close(); }
            if ui.button("Enregistrer sous…").clicked() {
                if let Some(i) = self.cur() {
                    if let Some(p) = rfd::FileDialog::new().set_directory(&self.root).save_file() {
                        if let Some(d) = self.tabs[i].doc_mut() {
                            d.remote = None;
                            let r = d.save_as(p);
                            self.report_save(r);
                        }
                    }
                }
                ui.close();
            }
            if ui.button("Tout enregistrer    Ctrl+Maj+S").clicked() {
                for i in 0..self.tabs.len() { if self.tabs[i].dirty() { self.save_tab(i); } }
                ui.close();
            }
            if ui.button("Fermer l'onglet          Ctrl+W").clicked() { if let Some(i) = self.cur() { self.close_tab(i, false); } ui.close(); }
            ui.separator();
            if ui.button("Quitter").clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); ui.close(); }
        });
        ui.menu_button("Exécuter", |ui| {
            if ui.button("Exécuter                    F5").clicked() { self.start(false); ui.close(); }
            if self.module.is_some() {
                if ui.button("Déboguer pas à pas          F6").clicked() { self.start(true); ui.close(); }
                if ui.button("Point d'arrêt               F9").clicked() {
                    if let Some(i) = self.cur() {
                        if let Some(l) = self.tabs[i].doc().map(|d| d.cursor_line) { self.toggle_breakpoint(i, l); }
                    }
                    ui.close();
                }
            }
            if ui.button("Arrêter                 Maj+F5").clicked() { self.stop(); ui.close(); }
            if ui.button("Arguments du programme…").clicked() { self.dialog = Some(Dialog::Args(self.run_args.clone())); ui.close(); }
        });
        ui.menu_button("Affichage", |ui| {
            ui.menu_button("Thème", |ui| {
                for (name, _) in theme::THEMES {
                    if ui.radio(self.theme == *name, *name).clicked() { self.set_theme(name, ctx); ui.close(); }
                }
            });
            if ui.button("Agrandir le texte    Ctrl++").clicked() { self.zoom(1.0, ctx); }
            if ui.button("Réduire le texte     Ctrl+-").clicked() { self.zoom(-1.0, ctx); }
            ui.separator();
            if ui.button("Séparer à droite       Ctrl+\\").clicked() {
                if let Some(i) = self.cur() { let (u, pa) = (self.tabs[i].uid, self.tabs[i].pane); self.split_tab(u, pa, false); }
                ui.close();
            }
            if ui.button("Séparer en bas").clicked() {
                if let Some(i) = self.cur() { let (u, pa) = (self.tabs[i].uid, self.tabs[i].pane); self.split_tab(u, pa, true); }
                ui.close();
            }
            ui.separator();
            if ui.checkbox(&mut self.show_explorer, "Panneau de gauche  Ctrl+B").clicked() { ui.close(); }
            if ui.checkbox(&mut self.show_bottom, "Panneau du bas     Ctrl+J").clicked() { ui.close(); }
        });
        ui.menu_button("Aide", |ui| {
            ui.label(RichText::new(format!("StudentCode {VERSION} (Rust)")).strong());
            ui.label(format!("Module PyScripter : {}", if self.module.is_some() { "installé" } else { "non installé" }));
            ui.label(format!("Module Lecteur PDF : {}", if self.pdf_module { "installé" } else { "non installé" }));
            ui.separator();
            ui.label("Ctrl+/  commenter · Tab / Maj+Tab  indenter\nCtrl+D  dupliquer la ligne · Ctrl+F  rechercher\n\
                      F5  exécuter · F6  déboguer · F9  point d'arrêt\nF10 / F11 / Maj+F11  pas à pas\n\
                      Ctrl+\\  séparer · glisser un onglet sur le bord d'un volet");
        });
    }

    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        egui::Panel::bottom("status").exact_size(22.0)
            .frame(egui::Frame::new().fill(p.bg_dark).inner_margin(egui::Margin::symmetric(8, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let state = if self.debugging() {
                        if self.frames.is_empty() { ("débogage en cours", p.orange) } else { ("en pause (débogueur)", p.yellow) }
                    } else if self.repl {
                        ("console Python", p.cyan)
                    } else if self.running() {
                        (if self.mode == Mode::Remote { "programme en cours (distant)" } else { "programme en cours" }, p.green)
                    } else {
                        ("prêt", p.comment)
                    };
                    ui.label(RichText::new(state.0).color(state.1).size(12.0));
                    if let Some((msg, t, col)) = self.status.clone() {
                        if t.elapsed().as_secs() < 8 {
                            ui.label(RichText::new(msg).color(col).size(12.0));
                            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("StudentCode {VERSION}")).color(p.comment).size(11.5));
                        if let Some(d) = self.cur().and_then(|i| self.tabs[i].doc()) {
                            ui.label(RichText::new(format!("Ln {}, Col {}", d.cursor_line, d.cursor_col)).color(p.comment).size(12.0));
                            ui.label(RichText::new(d.lang.name()).color(p.blue).size(12.0));
                        }
                        let ssh_on = self.sessions.iter().filter(|s| s.connected).count();
                        if ssh_on > 0 { ui.label(RichText::new(format!("SSH : {ssh_on}")).color(p.green).size(12.0)); }
                        if self.vpns.iter().any(|v| v.active) { ui.label(RichText::new("VPN").color(p.green).size(12.0)); }
                        ui.label(RichText::new(self.root.to_string_lossy()).color(p.comment).size(11.5));
                    });
                });
            });
    }

    pub(super) fn find_bar(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        egui::Frame::new().fill(p.bg_dark).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut self.find_text).id(egui::Id::new("find"))
                    .hint_text("Rechercher").desired_width(220.0));
                let go = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                ui.add(egui::TextEdit::singleline(&mut self.replace_text).hint_text("Remplacer par").desired_width(180.0));
                let next = ui.button("Suivant").clicked() || go;
                let replace_all = ui.button("Tout remplacer").clicked();
                ui.label(RichText::new(&self.find_info).color(p.comment).size(12.0));
                if icons::button(ui, Icon::Close, p.comment, p.bg_hl, "Fermer (Échap)").clicked() { self.find_open = false; }
                let (ft, rt) = (self.find_text.clone(), self.replace_text.clone());
                let mut info = None;
                if let Some(d) = self.cur_doc() {
                    if next { d.find_next = true; }
                    if replace_all && !ft.is_empty() {
                        let n = d.text.matches(ft.as_str()).count();
                        d.text = d.text.replace(ft.as_str(), &rt);
                        info = Some(format!("{n} remplacement(s)"));
                    }
                }
                if let Some(i) = info { self.find_info = i; }
                if go { r.request_focus(); }
            });
        });
    }

    pub(super) fn start_page(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.16).max(20.0));
            let tex = self.logo.get_or_insert_with(|| {
                let d = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon-256.png")).unwrap_or_default();
                let img = egui::ColorImage::from_rgba_unmultiplied([d.width as usize, d.height as usize], &d.rgba);
                ui.ctx().load_texture("logo", img, egui::TextureOptions::LINEAR)
            }).clone();
            ui.add(egui::Image::new(&tex).fit_to_exact_size(Vec2::splat(72.0)));
            ui.add_space(6.0);
            let mut job = egui::text::LayoutJob::default();
            job.append("Student", 0.0, egui::TextFormat::simple(FontId::proportional(40.0), if p.light { p.fg } else { Color32::WHITE }));
            job.append("Code", 0.0, egui::TextFormat::simple(FontId::proportional(40.0), Color32::from_rgb(0x2e, 0x7d, 0xe9)));
            ui.label(job);
            ui.add_space(18.0);
            if ui.add(egui::Button::new(RichText::new("Ouvrir un dossier…").size(15.0)).min_size(Vec2::new(260.0, 34.0))).clicked() {
                self.open_folder_dialog();
            }
            if ui.add(egui::Button::new(RichText::new("Nouveau fichier").size(15.0)).min_size(Vec2::new(260.0, 34.0))).clicked() {
                self.new_doc();
            }
            if ui.add(egui::Button::new(RichText::new("Se connecter à une machine (SSH)").size(15.0)).min_size(Vec2::new(260.0, 34.0))).clicked() {
                self.show_explorer = true;
                self.side = SideTab::Ssh;
            }
            ui.add_space(14.0);
            if !self.recent.is_empty() {
                ui.label(RichText::new("Récents").color(p.comment));
                for r in self.recent.clone().into_iter().take(6) {
                    let name = r.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| r.to_string_lossy().to_string());
                    if ui.add(egui::Button::new(RichText::new(name).color(p.blue)).frame(false)).on_hover_text(r.to_string_lossy()).clicked() {
                        self.set_root(r);
                        self.save_session();
                    }
                }
            }
            ui.add_space(14.0);
            ui.label(RichText::new("Ctrl+O ouvrir un dossier · F5 exécuter · Ctrl+J terminal").color(p.comment).size(12.0));
        });
    }

    pub(super) fn bottom_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        ui.set_min_height(ui.available_height());
        let mut tabs: Vec<(Bottom, String, Icon)> = vec![(Bottom::Terminal, "Terminal".into(), Icon::Terminal), (Bottom::Output, "Sortie".into(), Icon::Output)];
        for (i, s) in self.sessions.iter().enumerate() {
            if s.connected || s.term.running() { tabs.push((Bottom::Ssh(i), format!("SSH {}", s.conn.lab.label()), Icon::Terminal)); }
        }
        if let Bottom::Ssh(i) = self.bottom {
            if i >= self.sessions.len() { self.bottom = Bottom::Terminal; }
        }
        ui.horizontal(|ui| {
            for (tab, label, ic) in tabs {
                let sel = self.bottom == tab;
                let r = ui.add(egui::Button::new(RichText::new(format!("      {label}")).color(if sel { p.fg } else { p.comment }).size(13.0))
                    .frame(false).min_size(Vec2::new(96.0, 26.0)));
                icons::paint(ui.painter(), ic, egui::pos2(r.rect.min.x + 16.0, r.rect.center().y), 13.0,
                             if sel { p.blue } else if matches!(tab, Bottom::Ssh(_)) { p.green } else { p.comment });
                if sel {
                    let rr = r.rect;
                    ui.painter().line_segment([rr.left_bottom(), rr.right_bottom()], Stroke::new(2.0, p.blue));
                }
                if r.clicked() { self.bottom = tab; }
            }
            if self.bottom == Bottom::Output {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Effacer").clicked() { self.output.clear(); }
                });
            }
        });
        let font = FontId::monospace(self.font_size - 0.5);
        match self.bottom {
            Bottom::Terminal => {
                let root = self.root.clone();
                self.term.ui(ui, &p, &font, &root);
            }
            Bottom::Output => self.output_view(ui),
            Bottom::Ssh(i) => {
                let root = self.root.clone();
                if let Some(s) = self.sessions.get_mut(i) { s.term.ui(ui, &p, &font, &root); }
            }
        }
    }

    fn output_view(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        let font = FontId::monospace(self.font_size - 0.5);
        let input_h = 30.0;
        let h = (ui.available_height() - input_h).max(30.0);
        egui::ScrollArea::vertical().id_salt("output").max_height(h).auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
            let mut job = egui::text::LayoutJob::default();
            for (t, k) in &self.output {
                let col = match k { OutKind::Normal => p.fg, OutKind::Error => p.red, OutKind::Info => p.comment, OutKind::Echo => p.cyan };
                let mut f = egui::TextFormat::simple(font.clone(), col);
                f.italics = *k == OutKind::Info;
                job.append(t, 0.0, f);
            }
            ui.add(egui::Label::new(job).selectable(true).wrap());
        });
        ui.horizontal(|ui| {
            let paused = self.dbg.is_some() && !self.frames.is_empty();
            let prompt = if paused { "»" } else if self.repl { ">>>" } else { "›" };
            ui.label(RichText::new(prompt).monospace().color(p.cyan));
            let hint = if paused { "expression Python évaluée au point d'arrêt (variables visibles à droite)" }
                       else if self.repl { "expression Python sur les variables du programme" }
                       else if self.running() { "saisie envoyée au programme (Entrée)" } else { "lance un programme avec F5" };
            let r = ui.add(egui::TextEdit::singleline(&mut self.out_input).id(egui::Id::new("out_input"))
                .font(font.clone()).hint_text(hint).desired_width(f32::INFINITY));
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.send_input();
                r.request_focus();
            }
        });
    }

    pub(super) fn debug_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        ui.label(RichText::new("DÉBOGUEUR").strong().color(p.comment).size(12.0));
        if self.frames.is_empty() {
            ui.label(RichText::new("En cours d'exécution…").color(p.comment));
            if ui.button("Arrêter (Maj+F5)").clicked() { self.stop(); }
            return;
        }
        ui.horizontal(|ui| {
            if icons::button(ui, Icon::Continue, p.green, p.bg_hl, "Continuer (F5)").clicked() { self.dbg_cmd("continue"); }
            if icons::button(ui, Icon::StepOver, p.blue, p.bg_hl, "Pas à pas principal (F10)").clicked() { self.dbg_cmd("next"); }
            if icons::button(ui, Icon::StepInto, p.blue, p.bg_hl, "Entrer dans la fonction (F11)").clicked() { self.dbg_cmd("step"); }
            if icons::button(ui, Icon::StepOut, p.blue, p.bg_hl, "Sortir de la fonction (Maj+F11)").clicked() { self.dbg_cmd("return"); }
            if icons::button(ui, Icon::Stop, p.red, p.bg_hl, "Arrêter (Maj+F5)").clicked() { self.stop(); }
        });
        ui.add_space(4.0);
        ui.label(RichText::new("Pile d'appels").color(p.comment).size(12.0));
        let mut pick = None;
        for (k, f) in self.frames.iter().enumerate() {
            let name = Path::new(&f.file).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let txt = RichText::new(format!("{}  {}:{}", f.func, name, f.line)).monospace().size(12.0)
                .color(if k == self.frame_idx { p.yellow } else { p.fg });
            if ui.add(egui::Button::new(txt).frame(false)).clicked() { pick = Some(k); }
        }
        if let Some(k) = pick { self.children.clear(); self.pending_children.clear(); self.show_frame(k); }
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let locals = self.locals.clone();
            let globals = self.globals.clone();
            if !locals.is_empty() {
                egui::CollapsingHeader::new(RichText::new("Variables locales").color(p.comment)).default_open(true)
                    .show(ui, |ui| for v in &locals { self.var_row(ui, v, 0); });
            }
            egui::CollapsingHeader::new(RichText::new("Variables globales").color(p.comment)).default_open(locals.is_empty())
                .show(ui, |ui| for v in &globals { self.var_row(ui, v, 0); });
        });
    }

    fn var_row(&mut self, ui: &mut egui::Ui, v: &Var, depth: usize) {
        let p = self.p;
        let mut job = egui::text::LayoutJob::default();
        let mono = FontId::monospace(12.5);
        job.append(&v.name, 0.0, egui::TextFormat::simple(mono.clone(), p.blue));
        job.append(&format!("  {}", v.ty), 0.0, egui::TextFormat::simple(FontId::proportional(11.0), p.comment));
        job.append(&format!("  = {}", v.repr), 0.0, egui::TextFormat::simple(mono, p.fg));
        if v.expandable && depth < 6 {
            let id = ui.make_persistent_id(("var", &v.expr));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                .show_header(ui, |ui| { ui.label(job); })
                .body(|ui| {
                    match self.children.get(&v.expr).cloned() {
                        Some(items) => for c in &items { self.var_row(ui, c, depth + 1); },
                        None => {
                            ui.label(RichText::new("…").color(p.comment));
                            if !self.pending_children.values().any(|e| e == &v.expr) {
                                if let Some(s) = self.dbg.as_mut() {
                                    let id = s.request_children(self.frame_idx, &v.expr);
                                    self.pending_children.insert(id, v.expr.clone());
                                }
                            }
                        }
                    }
                });
        } else {
            ui.horizontal(|ui| { ui.add_space(18.0); ui.label(job); });
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
