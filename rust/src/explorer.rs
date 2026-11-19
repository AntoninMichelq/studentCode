//! Explorateur de fichiers : arborescence du dossier ouvert, dossiers dépliés retenus, menu
//! clic droit (nouveau, renommer, supprimer…), glisser-déposer pour déplacer.
use crate::theme::Palette;
use eframe::egui::{self, Color32, FontId, Rect, Sense, Vec2};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub dir: bool,
}

pub enum Action {
    Open(PathBuf),
    NewFile(PathBuf),
    NewDir(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
    Terminal(PathBuf),
    Moved(PathBuf, PathBuf),
    Upload(PathBuf),
}

pub struct Explorer {
    pub expanded: BTreeSet<PathBuf>,
    cache: HashMap<PathBuf, Vec<Entry>>,
    last_refresh: Instant,
    pub selected: Option<PathBuf>,
    pub show_hidden: bool,
    dragging: Option<PathBuf>,
}

const HIDDEN: &[&str] = &["__pycache__", "node_modules", "target", ".git", ".idea", ".vscode", ".mypy_cache"];

impl Explorer {
    pub fn new() -> Explorer {
        Explorer { expanded: BTreeSet::new(), cache: HashMap::new(), last_refresh: Instant::now(), selected: None,
                   show_hidden: false, dragging: None }
    }

    pub fn refresh(&mut self) {
        self.cache.clear();
    }

    fn list(&mut self, dir: &Path) -> &Vec<Entry> {
        let show_hidden = self.show_hidden;
        self.cache.entry(dir.to_path_buf()).or_insert_with(|| {
            let mut v: Vec<Entry> = std::fs::read_dir(dir).map(|rd| rd.flatten().filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if !show_hidden && (name.starts_with('.') || HIDDEN.contains(&name.as_str())) {
                    return None;
                }
                let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                    || (e.file_type().map(|t| t.is_symlink()).unwrap_or(false) && e.path().is_dir());
                Some(Entry { path: e.path(), name, dir })
            }).collect()).unwrap_or_default();
            v.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
            v
        })
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, root: &Path, p: &Palette) -> Vec<Action> {
        if self.last_refresh.elapsed().as_secs() >= 3 {
            self.cache.clear(); // les fichiers créés ailleurs apparaissent tout seuls
            self.last_refresh = Instant::now();
        }
        let mut actions = Vec::new();
        let title = root.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_else(|| root.to_string_lossy().to_string());
        let head = ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(title).strong().color(p.comment).size(12.0));
        }).response;
        head.context_menu(|ui| self.dir_menu(ui, root, &mut actions));
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            self.tree(ui, root, 0, p, &mut actions);
            // zone vide : menu du dossier racine et dépôt à la racine
            let rest = ui.allocate_response(Vec2::new(ui.available_width(), ui.available_height().max(40.0)), Sense::click());
            rest.context_menu(|ui| self.dir_menu(ui, root, &mut actions));
            if let Some(src) = self.drop_target(ui, &rest) {
                actions.push(Action::Moved(src, root.to_path_buf()));
            }
        });
        if ui.input(|i| i.pointer.any_released()) {
            self.dragging = None;
        }
        actions
    }

    fn drop_target(&mut self, ui: &egui::Ui, resp: &egui::Response) -> Option<PathBuf> {
        if self.dragging.is_some() && resp.contains_pointer() && ui.input(|i| i.pointer.any_released()) {
            return self.dragging.take();
        }
        None
    }

    fn tree(&mut self, ui: &mut egui::Ui, dir: &Path, depth: usize, p: &Palette, actions: &mut Vec<Action>) {
        let entries: Vec<(PathBuf, String, bool)> = self.list(dir).iter().map(|e| (e.path.clone(), e.name.clone(), e.dir)).collect();
        for (path, name, is_dir) in entries {
            let row_h = 22.0;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click_and_drag());
            let selected = self.selected.as_deref() == Some(path.as_path());
            let drop_here = is_dir && self.dragging.is_some() && resp.contains_pointer()
                && self.dragging.as_deref() != Some(path.as_path());
            if selected || resp.hovered() || drop_here {
                ui.painter().rect_filled(rect, 3.0, if selected { p.bg_hl } else { p.bg_hl.gamma_multiply(0.6) });
            }
            let x = rect.min.x + 6.0 + depth as f32 * 14.0;
            let cy = rect.center().y;
            let open = self.expanded.contains(&path);
            if is_dir {
                crate::icons::paint(ui.painter(), if open { crate::icons::Icon::ChevronDown } else { crate::icons::Icon::ChevronRight },
                                    egui::pos2(x + 4.0, cy), 12.0, p.comment);
                folder_icon(ui.painter(), egui::pos2(x + 18.0, cy), p.blue);
            } else {
                file_badge(ui.painter(), egui::pos2(x + 18.0, cy), &name, p);
            }
            ui.painter().text(egui::pos2(x + 30.0, cy), egui::Align2::LEFT_CENTER, &name, FontId::proportional(13.5),
                              if selected { p.fg } else { p.fg.gamma_multiply(0.92) });
            if resp.clicked() {
                self.selected = Some(path.clone());
                if is_dir {
                    if !self.expanded.remove(&path) {
                        self.expanded.insert(path.clone());
                    }
                } else {
                    actions.push(Action::Open(path.clone()));
                }
            }
            if resp.drag_started() {
                self.dragging = Some(path.clone());
            }
            if is_dir {
                if let Some(src) = self.drop_target(ui, &resp) {
                    if src != path && !path.starts_with(&src) {
                        actions.push(Action::Moved(src, path.clone()));
                    }
                }
            }
            let target = if is_dir { path.clone() } else { path.parent().unwrap_or(dir).to_path_buf() };
            resp.context_menu(|ui| {
                self.selected = Some(path.clone());
                if !is_dir && ui.button("Ouvrir").clicked() {
                    actions.push(Action::Open(path.clone()));
                    ui.close();
                }
                self.dir_menu(ui, &target, actions);
                ui.separator();
                if ui.button("Renommer…").clicked() {
                    actions.push(Action::Rename(path.clone()));
                    ui.close();
                }
                if ui.button("Copier le chemin").clicked() {
                    ui.ctx().copy_text(path.to_string_lossy().to_string());
                    ui.close();
                }
                if ui.button("Envoyer sur la machine SSH").clicked() {
                    actions.push(Action::Upload(path.clone()));
                    ui.close();
                }
                if ui.button(egui::RichText::new("Supprimer…").color(p.red)).clicked() {
                    actions.push(Action::Delete(path.clone()));
                    ui.close();
                }
            });
            if is_dir && open {
                self.tree(ui, &path, depth + 1, p, actions);
            }
        }
    }

    fn dir_menu(&mut self, ui: &mut egui::Ui, dir: &Path, actions: &mut Vec<Action>) {
        if ui.button("Nouveau fichier…").clicked() {
            actions.push(Action::NewFile(dir.to_path_buf()));
            ui.close();
        }
        if ui.button("Nouveau dossier…").clicked() {
            actions.push(Action::NewDir(dir.to_path_buf()));
            ui.close();
        }
        if ui.button("Ouvrir dans le terminal").clicked() {
            actions.push(Action::Terminal(dir.to_path_buf()));
            ui.close();
        }
        if ui.button(if self.show_hidden { "Masquer les fichiers cachés" } else { "Afficher les fichiers cachés" }).clicked() {
            self.show_hidden = !self.show_hidden;
            self.cache.clear();
            ui.close();
        }
    }
}

pub fn folder_icon(painter: &egui::Painter, c: egui::Pos2, col: Color32) {
    let body = Rect::from_center_size(c + Vec2::new(0.0, 1.0), Vec2::new(14.0, 10.0));
    let tab = Rect::from_min_size(body.min - Vec2::new(0.0, 2.5), Vec2::new(6.0, 4.0));
    painter.rect_filled(tab, 1.5, col);
    painter.rect_filled(body, 2.0, col);
}

/// Petite pastille colorée avec le type du fichier (comme la version Python).
pub fn file_badge(painter: &egui::Painter, c: egui::Pos2, name: &str, p: &Palette) {
    let lower = name.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let (label, col): (&str, Color32) = match (lower.as_str(), ext) {
        ("makefile", _) | (_, "mk") => ("mk", p.orange),
        ("dockerfile", _) => ("D", p.blue),
        (_, "py" | "pyw") => ("py", p.yellow),
        (_, "c") => ("C", p.blue),
        (_, "h" | "hpp") => ("H", p.magenta),
        (_, "cpp" | "cc" | "cxx") => ("C+", p.blue),
        (_, "java") => ("J", p.orange),
        (_, "class" | "jar") => ("J", p.comment),
        (_, "js" | "mjs") => ("JS", p.yellow),
        (_, "ts" | "tsx") => ("TS", p.blue),
        (_, "rs") => ("R", p.orange),
        (_, "go") => ("Go", p.cyan),
        (_, "cs") => ("C#", p.magenta),
        (_, "kt" | "kts" | "gradle") => ("K", p.magenta),
        (_, "html" | "htm") => ("<>", p.orange),
        (_, "css" | "scss") => ("#", p.blue),
        (_, "json") => ("{}", p.yellow),
        (_, "md") => ("M", p.cyan),
        (_, "sh" | "bash" | "zsh") => ("$", p.green),
        (_, "ps1" | "bat" | "cmd") => ("ps", p.blue),
        (_, "pdf") => ("pdf", p.red),
        (_, "png" | "jpg" | "jpeg" | "gif" | "svg" | "bmp" | "webp") => ("img", p.magenta),
        (_, "txt" | "log") => ("txt", p.comment),
        (_, "sql") => ("sql", p.cyan),
        (_, "xml" | "yml" | "yaml" | "toml" | "ini" | "cfg") => ("cfg", p.teal),
        (_, "zip" | "gz" | "tar" | "7z") => ("zip", p.comment),
        _ => ("", p.comment),
    };
    if label.is_empty() {
        let r = Rect::from_center_size(c, Vec2::new(10.0, 13.0));
        painter.rect_stroke(r, 1.5, egui::Stroke::new(1.2, col), egui::StrokeKind::Inside);
        return;
    }
    let w = if label.len() >= 3 { 18.0 } else { 15.0 };
    let r = Rect::from_center_size(c, Vec2::new(w, 13.0));
    painter.rect_filled(r, 3.0, col);
    painter.text(c, egui::Align2::CENTER_CENTER, label, FontId::proportional(if label.len() >= 3 { 7.5 } else { 9.0 }),
                 p.bg_dark);
}
