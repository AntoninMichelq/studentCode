//! Explorateur de fichiers : arborescence du dossier ouvert, dossiers dépliés retenus, menu
//! clic droit (nouveau, renommer, supprimer…), glisser-déposer pour déplacer.
use crate::theme::Palette;
use eframe::egui::{self, Color32, FontId, Sense, Vec2};
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
        // titre du dossier (QLabel#title de la version Python : gras, gris, 26 px de haut)
        let (hr, head) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
        ui.painter().text(egui::pos2(hr.min.x + 23.0, hr.min.y + 12.0), egui::Align2::LEFT_CENTER, title,
                          FontId::new(13.3, egui::FontFamily::Name("bold".into())), p.comment);
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
            // mêmes mesures que l'arbre Qt de la version Python : lignes de 24 px, flèche, icône, nom
            let row_h = 24.0;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click_and_drag());
            let selected = self.selected.as_deref() == Some(path.as_path());
            let drop_here = is_dir && self.dragging.is_some() && resp.contains_pointer()
                && self.dragging.as_deref() != Some(path.as_path());
            if selected || drop_here {
                ui.painter().rect_filled(rect, 0.0, p.bg_hl);
            }
            let x = rect.min.x + depth as f32 * 14.0;
            let cy = rect.center().y;
            let open = self.expanded.contains(&path);
            if is_dir {
                branch_arrow(ui.painter(), egui::pos2(x + 11.0, cy), open, p.comment);
                crate::icons::paint(ui.painter(), crate::icons::Icon::Folder, egui::pos2(x + 31.0, cy), 16.0, p.blue);
            } else {
                file_badge(ui.painter(), egui::pos2(x + 31.0, cy), &name, p);
            }
            ui.painter().text(egui::pos2(x + 46.0, cy), egui::Align2::LEFT_CENTER, &name, FontId::proportional(13.3), p.fg);
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


/// Petite flèche pleine des arbres Qt (▸ fermé, ▾ ouvert).
pub fn branch_arrow(painter: &egui::Painter, c: egui::Pos2, open: bool, col: Color32) {
    let pts = if open {
        vec![c + Vec2::new(-3.5, -1.8), c + Vec2::new(3.5, -1.8), c + Vec2::new(0.0, 2.2)]
    } else {
        vec![c + Vec2::new(-1.8, -3.5), c + Vec2::new(2.2, 0.0), c + Vec2::new(-1.8, 3.5)]
    };
    painter.add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
}

/// Icône du type de fichier (exactement celles de la version Python).
pub fn file_badge(painter: &egui::Painter, c: egui::Pos2, name: &str, p: &Palette) {
    crate::icons::paint_file(painter, c, name, 15.0, p);
}
