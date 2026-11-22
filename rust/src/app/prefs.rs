//! Fenêtre « Paramètres » (Ctrl+,) : apparence, disposition, éditeur, terminal et exécution,
//! raccourcis clavier. Tout s'applique tout de suite et est enregistré dans settings.json.
use super::*;
use crate::settings::{self, Action, Settings, Shortcut};
use egui::{Rect, RichText, Sense};

const PAGES: &[&str] = &["Apparence", "Disposition", "Éditeur", "Terminal et exécution", "Raccourcis clavier"];

impl App {
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            self.key_capture = None;
            return;
        }
        let p = self.p;
        let before = self.cfg.clone();
        let mut open = true;
        egui::Window::new("Paramètres").open(&mut open).collapsible(false).resizable(true)
            .default_size([760.0, 560.0]).min_width(560.0).min_height(360.0)
            .pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.content_rect().center())
            .frame(egui::Frame::window(&ctx.global_style()).fill(p.bg_dark).inner_margin(egui::Margin::same(10)))
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    // pages à gauche
                    ui.vertical(|ui| {
                        ui.set_width(190.0);
                        for (i, name) in PAGES.iter().enumerate() {
                            let sel = self.settings_page as usize == i;
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(190.0, 30.0), Sense::click());
                            if sel || resp.hovered() { ui.painter().rect_filled(r, 4.0, if sel { p.bg_hl } else { p.bg }); }
                            if sel { ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(3.0, r.height())), 1.0, p.blue); }
                            ui.painter().text(egui::pos2(r.min.x + 12.0, r.center().y), egui::Align2::LEFT_CENTER, *name,
                                              FontId::proportional(13.3), if sel { p.fg } else { p.comment });
                            if resp.clicked() { self.settings_page = i as u8; self.key_capture = None; }
                        }
                        ui.add_space(12.0);
                        if ui.button("Tout remettre par défaut").on_hover_text("Tous les réglages et raccourcis").clicked() {
                            let fonts_changed = !self.cfg.code_font.is_empty() || !self.cfg.ui_font.is_empty();
                            self.cfg = Settings::default();
                            if fonts_changed { crate::theme::load_fonts(ctx, "", ""); }
                        }
                        if let Some(f) = Settings::path() {
                            if ui.button("Ouvrir settings.json").clicked() {
                                self.cfg.save();
                                self.open_file(&f);
                            }
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical().id_salt("settings_page").auto_shrink(false).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 6.0;
                            match self.settings_page {
                                0 => self.page_appearance(ui, ctx),
                                1 => self.page_layout(ui),
                                2 => self.page_editor(ui),
                                3 => self.page_terminal(ui),
                                _ => self.page_keys(ui, ctx),
                            }
                        });
                    });
                });
            });
        if !open { self.settings_open = false; self.key_capture = None; }
        if self.cfg != before {
            self.cfg.fix();
            if (self.cfg.ui_scale - before.ui_scale).abs() > 0.001 { ctx.set_zoom_factor(self.cfg.ui_scale); }
            if self.cfg.code_font != before.code_font || self.cfg.ui_font != before.ui_font {
                crate::theme::load_fonts(ctx, &self.cfg.code_font, &self.cfg.ui_font);
            }
            self.cfg_dirty = true;
        }
    }

    fn page_appearance(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = self.p;
        heading(ui, &p, "Thème");
        let mut theme = self.theme.clone();
        egui::ComboBox::from_id_salt("set_theme").width(260.0).selected_text(&theme).show_ui(ui, |ui| {
            for (name, _) in crate::theme::THEMES { ui.selectable_value(&mut theme, name.to_string(), *name); }
        });
        if theme != self.theme { self.set_theme(&theme, ctx); }

        heading(ui, &p, "Tailles");
        ui.horizontal(|ui| {
            ui.label("Taille de l'interface");
            let cur = self.cfg.ui_scale;
            egui::ComboBox::from_id_salt("set_scale").selected_text(format!("{:.0} %", cur * 100.0)).show_ui(ui, |ui| {
                for v in [0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5] {
                    ui.selectable_value(&mut self.cfg.ui_scale, v, format!("{:.0} %", v * 100.0));
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Taille du texte du code");
            let mut fs = self.font_size;
            if ui.add(egui::DragValue::new(&mut fs).range(9.0..=28.0).speed(0.2).fixed_decimals(1).suffix(" px")).changed() {
                self.font_size = fs;
                crate::theme::apply(ctx, &self.p, self.font_size);
            }
            ui.label(RichText::new(format!("({} / {})", self.cfg.shortcut_text(Action::ZoomIn), self.cfg.shortcut_text(Action::ZoomOut))).color(p.comment));
        });

        heading(ui, &p, "Polices");
        font_combo(ui, "Code", "set_code_font", &mut self.cfg.code_font, true);
        font_combo(ui, "Interface", "set_ui_font", &mut self.cfg.ui_font, false);
        ui.label(RichText::new("« Automatique » prend la même police que la version Python (JetBrains Mono, Ubuntu…).").color(p.comment).small());

        heading(ui, &p, "Barres");
        ui.checkbox(&mut self.cfg.show_statusbar, "Barre d'état en bas de la fenêtre");
        ui.horizontal(|ui| {
            ui.label("Barre d'outils :");
            if ui.link("choisir les boutons et leur ordre (Disposition)").clicked() { self.settings_page = 1; }
        });
    }

    fn page_layout(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        heading(ui, &p, "Panneaux");
        egui::Grid::new("set_panels").num_columns(3).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label("Fichiers / SSH / VPN");
            ui.radio_value(&mut self.cfg.side_right, false, "à gauche");
            ui.radio_value(&mut self.cfg.side_right, true, "à droite");
            ui.end_row();
            ui.label("Sortie et terminaux");
            ui.radio_value(&mut self.cfg.bottom_right, false, "en bas");
            ui.radio_value(&mut self.cfg.bottom_right, true, "à droite");
            ui.end_row();
            ui.label("Débogage (variables)");
            ui.radio_value(&mut self.cfg.debug_left, false, "à droite");
            ui.radio_value(&mut self.cfg.debug_left, true, "à gauche");
            ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_explorer, "Afficher le panneau latéral");
            ui.checkbox(&mut self.show_bottom, "Afficher le panneau du bas");
        });
        ui.label(RichText::new("Astuce : clic droit sur les onglets d'un panneau pour le déplacer, et glisser un onglet pour changer l'ordre. \
            Les onglets de fichiers se glissent aussi d'un volet à l'autre, ou vers le bord d'un volet pour le scinder.").color(p.comment).small());

        heading(ui, &p, "Ordre des onglets du panneau latéral");
        let side = |k: &str| match k { "files" => "Fichiers", "ssh" => "SSH", _ => "VPN" }.to_string();
        order_list(ui, &p, "side_ord", &mut self.cfg.side_order, side, None);
        heading(ui, &p, "Ordre des onglets du panneau du bas");
        let bottom = |k: &str| match k { "out" => "Sortie", "repl" => "Interpréteur Python", "term" => "Terminaux", _ => "Terminaux SSH" }.to_string();
        order_list(ui, &p, "bottom_ord", &mut self.cfg.bottom_order, bottom, None);

        heading(ui, &p, "Barre d'outils");
        ui.label(RichText::new("Coche les boutons à afficher ; les flèches changent l'ordre. Certains n'apparaissent que \
            lorsqu'ils servent (débogage, fichier Python…).").color(p.comment).small());
        let label = |k: &str| if k == "|" { "──── séparateur ────".to_string() }
                              else { settings::TOOLBAR.iter().find(|(id, _)| *id == k).map(|(_, n)| n.to_string()).unwrap_or_default() };
        let hidden = &mut self.cfg.toolbar_hidden;
        order_list(ui, &p, "tb_ord", &mut self.cfg.toolbar_order, label, Some(hidden));
        ui.horizontal(|ui| {
            if ui.button("Ajouter un séparateur").clicked() { self.cfg.toolbar_order.push("|".into()); }
            if ui.button("Barre d'outils par défaut").clicked() {
                let d = Settings::default();
                self.cfg.toolbar_order = d.toolbar_order;
                self.cfg.toolbar_hidden = d.toolbar_hidden;
            }
        });
    }

    fn page_editor(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        let c = &mut self.cfg;
        heading(ui, &p, "Mode Vim");
        ui.checkbox(&mut c.vim_mode, "Mode Vim, avec les raccourcis de LazyVim (Espace = touche leader)");
        ui.add_enabled(c.vim_mode, egui::Checkbox::new(&mut c.relative_numbers, "Numéros de ligne relatifs"));
        ui.label(RichText::new("Échap : mode Normal · i : écrire · v / V : sélection · : commandes (:w, :q…) · Espace : menu des raccourcis (comme which-key)").color(p.comment).small());
        heading(ui, &p, "Indentation");
        ui.horizontal(|ui| {
            ui.label("Largeur d'une tabulation");
            ui.add(egui::DragValue::new(&mut c.tab_width).range(1..=12).suffix(" espaces"));
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut c.use_spaces, true, "Insérer des espaces (conseillé en Python)");
            ui.radio_value(&mut c.use_spaces, false, "Insérer une tabulation");
        });
        ui.checkbox(&mut c.auto_indent, "Indentation automatique après Entrée (après « : », « { »…)");
        ui.checkbox(&mut c.close_brackets, "Fermer automatiquement ( [ { \" '");

        heading(ui, &p, "Affichage");
        ui.checkbox(&mut c.line_numbers, "Numéros de ligne");
        ui.checkbox(&mut c.folding, "Replier les blocs (flèches dans la marge)");
        ui.checkbox(&mut c.indent_guides, "Guides d'indentation (pointillés)");
        ui.checkbox(&mut c.caret_line, "Surligner la ligne du curseur");
        ui.checkbox(&mut c.bracket_match, "Montrer la parenthèse correspondante");
        ui.checkbox(&mut c.word_wrap, "Retour à la ligne automatique des lignes longues");

        heading(ui, &p, "Enregistrement");
        ui.horizontal(|ui| {
            ui.label("Enregistrement automatique");
            let names = ["jamais", "en changeant d'onglet ou de fenêtre", "une seconde après la dernière frappe"];
            egui::ComboBox::from_id_salt("set_autosave").selected_text(names[c.autosave.min(2) as usize]).width(280.0).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() { ui.selectable_value(&mut c.autosave, i as u8, *n); }
            });
        });
        ui.checkbox(&mut c.trim_on_save, "Enlever les espaces en fin de ligne en enregistrant");
    }

    fn page_terminal(&mut self, ui: &mut egui::Ui) {
        let p = self.p;
        let c = &mut self.cfg;
        heading(ui, &p, "Terminaux");
        ui.horizontal(|ui| {
            ui.label("Shell");
            ui.add(egui::TextEdit::singleline(&mut c.shell).desired_width(300.0)
                .hint_text(if cfg!(windows) { "automatique (PowerShell)" } else { "automatique ($SHELL)" }));
        });
        ui.label(RichText::new("Ex. /bin/zsh, fish, bash --login. S'applique aux nouveaux terminaux.").color(p.comment).small());
        ui.horizontal(|ui| {
            ui.label("Taille du texte");
            let mut auto = c.term_font_size == 0.0;
            if ui.checkbox(&mut auto, "comme l'éditeur").changed() { c.term_font_size = if auto { 0.0 } else { 13.0 }; }
            if !auto { ui.add(egui::DragValue::new(&mut c.term_font_size).range(8.0..=32.0).speed(0.2).fixed_decimals(1).suffix(" px")); }
        });
        ui.horizontal(|ui| {
            ui.label("Lignes gardées en mémoire");
            ui.add(egui::DragValue::new(&mut c.scrollback).range(100..=100_000).speed(50));
        });

        heading(ui, &p, "Exécution des programmes");
        ui.horizontal(|ui| {
            ui.label("Interpréteur Python");
            ui.add(egui::TextEdit::singleline(&mut c.python).desired_width(260.0)
                .hint_text(if cfg!(windows) { "automatique (py -3)" } else { "automatique (python3)" }));
            if ui.button("Parcourir…").clicked() {
                if let Some(f) = rfd::FileDialog::new().pick_file() { c.python = f.to_string_lossy().to_string(); }
            }
        });
        if !c.python.trim().is_empty() {
            let first = c.python.split_whitespace().next().unwrap_or("");
            if !std::path::Path::new(first).is_file() && crate::runner::which(first).is_none() {
                ui.label(RichText::new("Introuvable : l'interpréteur automatique sera utilisé.").color(p.red).small());
            }
        }
        ui.label(RichText::new("Ex. un environnement virtuel : ~/projet/.venv/bin/python").color(p.comment).small());
        ui.checkbox(&mut c.save_before_run, "Enregistrer aussi les autres fichiers modifiés avant d'exécuter");
        ui.checkbox(&mut c.clear_output, "Effacer la Sortie à chaque exécution");
    }

    fn page_keys(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = self.p;
        heading(ui, &p, "Raccourcis clavier");
        ui.label(RichText::new("Clique sur un raccourci puis appuie sur les nouvelles touches (Échap : annuler, Retour arrière : aucun).")
            .color(p.comment).small());
        // touche attendue
        if let Some(a) = self.key_capture {
            let got = ctx.input_mut(|i| {
                let mut res = None;
                for e in &i.events {
                    if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                        if settings::is_modifier(*key) { continue; }
                        res = Some((*key, *modifiers));
                        break;
                    }
                }
                // la touche ne doit rien faire d'autre (ni texte, ni raccourci, ni copier / coller)
                i.events.retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)));
                res
            });
            if let Some((key, m)) = got {
                let plain = !(m.ctrl || m.command || m.alt || m.shift);
                if key == egui::Key::Escape && plain {
                    self.key_capture = None;
                } else if (key == egui::Key::Backspace || key == egui::Key::Delete) && plain {
                    self.cfg.keys.insert(a.id().into(), String::new());
                    self.key_capture = None;
                } else {
                    let txt = Shortcut::from_event(key, m).to_string();
                    if txt == a.default_keys() {
                        self.cfg.keys.remove(a.id());
                    } else {
                        self.cfg.keys.insert(a.id().into(), txt);
                    }
                    self.key_capture = None;
                }
            }
        }
        let filter_id = egui::Id::new("keys_filter");
        let mut filter = ui.data(|d| d.get_temp::<String>(filter_id)).unwrap_or_default();
        ui.add(egui::TextEdit::singleline(&mut filter).hint_text("Chercher une action ou une touche…").desired_width(320.0));
        ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        let fl = filter.to_lowercase();
        // raccourcis en double
        let mut used: std::collections::HashMap<String, Vec<Action>> = Default::default();
        for a in settings::ACTIONS {
            for sc in self.cfg.shortcuts(*a) { used.entry(sc.to_string()).or_default().push(*a); }
        }
        egui::Grid::new("keys_grid").num_columns(3).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            for a in settings::ACTIONS {
                let txt = self.cfg.shortcuts_text(*a);
                if !fl.is_empty() && !a.label().to_lowercase().contains(&fl) && !txt.to_lowercase().contains(&fl) { continue; }
                ui.label(a.label());
                let capturing = self.key_capture == Some(*a);
                let dup: Option<Vec<Action>> = {
                    let v: Vec<Action> = self.cfg.shortcuts(*a).iter().flat_map(|sc| used.get(&sc.to_string()).cloned().unwrap_or_default())
                        .filter(|x| x != a).collect();
                    if v.is_empty() { None } else { Some(v) }
                };
                let shown = if capturing { "Appuie sur les touches…".to_string() } else if txt.is_empty() { "—".into() } else { txt.clone() };
                let col = if capturing { p.blue } else if dup.is_some() { p.red } else { p.fg };
                let mut b = ui.add_sized([210.0, 24.0], egui::Button::new(RichText::new(shown).monospace().color(col)).selected(capturing));
                if let Some(v) = &dup {
                    let others: Vec<&str> = v.iter().map(|x| x.label()).collect();
                    b = b.on_hover_text(format!("Déjà utilisé par : {}", others.join(", ")));
                }
                if b.clicked() { self.key_capture = if capturing { None } else { Some(*a) }; }
                ui.horizontal(|ui| {
                    let custom = self.cfg.keys.contains_key(a.id());
                    if ui.add_enabled(custom, egui::Button::new("↺")).on_hover_text(format!("Par défaut : {}", a.default_keys())).clicked() {
                        self.cfg.keys.remove(a.id());
                    }
                    if ui.add_enabled(!txt.is_empty(), egui::Button::new("✕")).on_hover_text("Aucun raccourci").clicked() {
                        self.cfg.keys.insert(a.id().into(), String::new());
                    }
                });
                ui.end_row();
            }
        });
        if ui.button("Raccourcis par défaut").clicked() { self.cfg.keys.clear(); self.key_capture = None; }
    }
}

fn heading(ui: &mut egui::Ui, p: &Palette, t: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(t).strong().color(p.blue).size(14.0));
}

/// Liste ordonnable : flèches pour monter / descendre, case à cocher si `hidden` est donné.
fn order_list(ui: &mut egui::Ui, p: &Palette, id: &str, list: &mut Vec<String>, label: impl Fn(&str) -> String,
              mut hidden: Option<&mut Vec<String>>) {
    let mut mv: Option<(usize, bool)> = None;
    let mut del: Option<usize> = None;
    egui::Grid::new(id).num_columns(2).spacing([6.0, 2.0]).show(ui, |ui| {
        for (i, k) in list.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.add_enabled(i > 0, egui::Button::new("▲").small()).clicked() { mv = Some((i, true)); }
                if ui.add_enabled(i + 1 < list.len(), egui::Button::new("▼").small()).clicked() { mv = Some((i, false)); }
            });
            match hidden.as_deref_mut() {
                Some(h) if k != "|" => {
                    let mut on = !h.contains(k);
                    if ui.checkbox(&mut on, label(k)).changed() {
                        if on { h.retain(|x| x != k); } else { h.push(k.clone()); }
                    }
                }
                Some(_) => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(label(k)).color(p.comment));
                        if ui.small_button("✕").on_hover_text("Enlever ce séparateur").clicked() { del = Some(i); }
                    });
                }
                None => { ui.label(label(k)); }
            }
            ui.end_row();
        }
    });
    if let Some((i, up)) = mv {
        if up { list.swap(i, i - 1); } else { list.swap(i, i + 1); }
    }
    if let Some(i) = del { list.remove(i); }
}

fn font_combo(ui: &mut egui::Ui, label: &str, id: &str, value: &mut String, code: bool) {
    let fonts = crate::theme::installed_fonts();
    let cur = if value.is_empty() { "Automatique".to_string() }
              else { fonts.iter().find(|f| &f.1 == value).map(|f| f.0.clone())
                     .unwrap_or_else(|| std::path::Path::new(value.as_str()).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()) };
    ui.horizontal(|ui| {
        ui.add_sized([80.0, 20.0], egui::Label::new(label));
        egui::ComboBox::from_id_salt(id).width(300.0).height(360.0).selected_text(cur).show_ui(ui, |ui| {
            ui.selectable_value(value, String::new(), "Automatique");
            let mut shown_other = false;
            for (name, file, mono) in fonts {
                if code && !*mono && !shown_other { ui.separator(); shown_other = true; }
                let r = ui.selectable_label(value == file, name);
                if r.clicked() { *value = file.clone(); }
            }
        });
        if ui.button("Fichier…").on_hover_text("Choisir un fichier .ttf / .otf").clicked() {
            if let Some(f) = rfd::FileDialog::new().add_filter("Police", &["ttf", "otf"]).pick_file() {
                *value = f.to_string_lossy().to_string();
            }
        }
    });
}
