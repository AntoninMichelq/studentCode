//! Thèmes de couleurs (les mêmes que la version Python) et réglage de l'apparence d'egui.
use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub bg_dark: Color32,
    pub bg_hl: Color32,
    pub fg: Color32,
    pub comment: Color32,
    pub gutter: Color32,
    pub blue: Color32,
    pub cyan: Color32,
    pub green: Color32,
    pub magenta: Color32,
    pub orange: Color32,
    pub red: Color32,
    pub yellow: Color32,
    pub teal: Color32,
    pub sel: Color32,
    pub curline: Color32,
    pub err_bg: Color32,
    pub light: bool,
}

fn c(hex: &str) -> Color32 {
    Color32::from_hex(hex).unwrap_or(Color32::WHITE)
}

/// (nom, 17 couleurs : fond, fond sombre, surlignage, texte, commentaire, gouttière, bleu, cyan,
/// vert, magenta, orange, rouge, jaune, sarcelle, sélection, ligne courante, fond d'erreur)
pub const THEMES: &[(&str, [&str; 17])] = &[
    ("Tokyo Night", ["#1a1b26", "#16161e", "#292e42", "#c0caf5", "#565f89", "#3b4261", "#7aa2f7", "#7dcfff",
        "#9ece6a", "#bb9af7", "#ff9e64", "#f7768e", "#e0af68", "#73daca", "#283457", "#1f2335", "#5a2233"]),
    ("Tokyo Night Storm", ["#24283b", "#1f2335", "#2f354f", "#c0caf5", "#636da6", "#3b4261", "#7aa2f7", "#7dcfff",
        "#9ece6a", "#bb9af7", "#ff9e64", "#f7768e", "#e0af68", "#73daca", "#2e3c64", "#292e42", "#5c2a3b"]),
    ("Catppuccin Mocha", ["#1e1e2e", "#181825", "#313244", "#cdd6f4", "#6c7086", "#45475a", "#89b4fa", "#89dceb",
        "#a6e3a1", "#cba6f7", "#fab387", "#f38ba8", "#f9e2af", "#94e2d5", "#3d4466", "#26263a", "#5a2d3d"]),
    ("Everforest", ["#2d353b", "#272e33", "#3d484d", "#d3c6aa", "#859289", "#56635f", "#7fbbb3", "#83c092",
        "#a7c080", "#d699b6", "#e69875", "#e67e80", "#dbbc7f", "#83c092", "#475258", "#343f44", "#5c3f45"]),
    ("Gruvbox", ["#282828", "#1d2021", "#3c3836", "#ebdbb2", "#928374", "#504945", "#83a598", "#8ec07c",
        "#b8bb26", "#d3869b", "#fe8019", "#fb4934", "#fabd2f", "#8ec07c", "#504945", "#32302f", "#5c2a27"]),
    ("Nord", ["#2e3440", "#292e39", "#3b4252", "#d8dee9", "#6f7a8f", "#4c566a", "#81a1c1", "#88c0d0",
        "#a3be8c", "#b48ead", "#d08770", "#bf616a", "#ebcb8b", "#8fbcbb", "#434c5e", "#353b49", "#5b3a40"]),
    ("Dracula", ["#282a36", "#21222c", "#343746", "#f8f8f2", "#6272a4", "#44475a", "#8be9fd", "#8be9fd",
        "#50fa7b", "#bd93f9", "#ffb86c", "#ff5555", "#f1fa8c", "#50fa7b", "#44475a", "#2f3140", "#5e2d33"]),
    ("Rosé Pine", ["#191724", "#14121d", "#26233a", "#e0def4", "#6e6a86", "#403d52", "#9ccfd8", "#9ccfd8",
        "#95b1ac", "#c4a7e7", "#ebbcba", "#eb6f92", "#f6c177", "#9ccfd8", "#312f44", "#211f2d", "#532c3f"]),
    ("Tokyo Night Day (clair)", ["#e1e2e7", "#d5d6db", "#c4c8da", "#3760bf", "#848cb5", "#a8aecb", "#2e7de9", "#007197",
        "#587539", "#9854f1", "#b15c00", "#f52a65", "#8c6c3e", "#118c74", "#b7c1e3", "#d8dae4", "#f5c9d3"]),
    ("Catppuccin Latte (clair)", ["#eff1f5", "#e6e9ef", "#ccd0da", "#4c4f69", "#8c8fa1", "#acb0be", "#1e66f5", "#04a5e5",
        "#40a02b", "#8839ef", "#fe640b", "#d20f39", "#df8e1d", "#179299", "#bcc0cc", "#e6e9ef", "#f3c4cf"]),
];

pub fn palette(name: &str) -> Palette {
    let (n, v) = THEMES.iter().find(|(n, _)| *n == name).unwrap_or(&THEMES[0]);
    Palette {
        bg: c(v[0]), bg_dark: c(v[1]), bg_hl: c(v[2]), fg: c(v[3]), comment: c(v[4]), gutter: c(v[5]),
        blue: c(v[6]), cyan: c(v[7]), green: c(v[8]), magenta: c(v[9]), orange: c(v[10]), red: c(v[11]),
        yellow: c(v[12]), teal: c(v[13]), sel: c(v[14]), curline: c(v[15]), err_bg: c(v[16]),
        light: n.contains("clair"),
    }
}

pub fn apply(ctx: &egui::Context, p: &Palette, font_size: f32) {
    let mut v = if p.light { Visuals::light() } else { Visuals::dark() };
    v.panel_fill = p.bg_dark;
    v.window_fill = p.bg_dark;
    v.extreme_bg_color = p.bg;
    v.faint_bg_color = p.bg_hl;
    v.code_bg_color = p.bg;
    v.override_text_color = Some(p.fg);
    v.hyperlink_color = p.blue;
    v.selection.bg_fill = p.sel;
    v.selection.stroke = Stroke::new(1.0, p.fg);
    v.window_stroke = Stroke::new(1.0, p.bg_hl);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.text_cursor.stroke = Stroke::new(2.0, p.blue);
    for (w, fill) in [
        (&mut v.widgets.noninteractive, p.bg_dark),
        (&mut v.widgets.inactive, p.bg_dark),
        (&mut v.widgets.hovered, p.bg_hl),
        (&mut v.widgets.active, p.sel),
        (&mut v.widgets.open, p.bg_hl),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.corner_radius = CornerRadius::same(5);
        w.fg_stroke.color = p.fg;
        w.bg_stroke = Stroke::NONE;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.bg_hl);
    v.widgets.noninteractive.fg_stroke.color = p.fg;
    v.widgets.inactive.fg_stroke.color = p.fg;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        use egui::{FontFamily, FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::new(13.5, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(13.5, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(11.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(font_size, FontFamily::Monospace));
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(7.0, 3.0);
        s.spacing.scroll.floating = true;
        s.spacing.scroll.bar_width = 8.0;
    });
}

/// Police de code : JetBrains Mono, Cascadia Mono ou Consolas si elles sont installées.
pub fn load_fonts(ctx: &egui::Context) {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(h) = directories::BaseDirs::new() {
        dirs.push(h.home_dir().join(".local/share/fonts"));
        dirs.push(h.home_dir().join(".fonts"));
        dirs.push(h.data_local_dir().join("Microsoft/Windows/Fonts"));
    }
    dirs.push("/usr/share/fonts".into());
    dirs.push("/usr/local/share/fonts".into());
    dirs.push("C:\\Windows\\Fonts".into());
    let wanted = ["JetBrainsMonoNerdFont-Regular.ttf", "JetBrainsMono-Regular.ttf", "CascadiaMono.ttf",
                  "FiraCode-Regular.ttf", "UbuntuMono-R.ttf", "consola.ttf"];
    let mut found: Option<Vec<u8>> = None;
    'outer: for name in wanted {
        for d in &dirs {
            if let Some(p) = find_file(d, name, 4) {
                if let Ok(b) = std::fs::read(p) {
                    found = Some(b);
                    break 'outer;
                }
            }
        }
    }
    if let Some(bytes) = found {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert("code".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "code".into());
        ctx.set_fonts(fonts);
    }
}

fn find_file(dir: &std::path::Path, name: &str, depth: u32) -> Option<std::path::PathBuf> {
    let p = dir.join(name);
    if p.is_file() {
        return Some(p);
    }
    if depth == 0 {
        return None;
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(f) = find_file(&e.path(), name, depth - 1) {
                return Some(f);
            }
        }
    }
    None
}
