//! Icônes dessinées en vectoriel (nettes à toutes les tailles, aux couleurs du thème, sans
//! dépendre des polices installées).
use eframe::egui::{self, pos2, vec2, Color32, Painter, Pos2, Rect, Sense, Shape, Stroke, Vec2};

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Menu, New, Folder, Save, Run, Debug, Stop, StepOver, StepInto, StepOut, Search, Terminal, Sidebar,
    Minimize, Maximize, Close, ChevronRight, ChevronDown, Output, Continue, Shield, Globe, Split, ChevronLeft, ChevronUp,
}

pub fn paint(p: &Painter, icon: Icon, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.6, col);
    let r = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
    match icon {
        Icon::Menu => {
            for y in [-0.33, 0.0, 0.33] {
                p.line_segment([r(-0.42, y), r(0.42, y)], st);
            }
        }
        Icon::New => {
            p.add(Shape::closed_line(vec![r(-0.32, -0.45), r(0.12, -0.45), r(0.34, -0.22), r(0.34, 0.45), r(-0.32, 0.45)], st));
            p.line_segment([r(0.0, -0.05), r(0.0, 0.25)], st);
            p.line_segment([r(-0.15, 0.1), r(0.15, 0.1)], st);
        }
        Icon::Folder => {
            p.add(Shape::closed_line(vec![r(-0.45, -0.32), r(-0.12, -0.32), r(0.0, -0.18), r(0.45, -0.18), r(0.45, 0.35),
                                          r(-0.45, 0.35)], st));
        }
        Icon::Save => {
            p.add(Shape::closed_line(vec![r(-0.4, -0.4), r(0.25, -0.4), r(0.4, -0.25), r(0.4, 0.4), r(-0.4, 0.4)], st));
            p.rect_stroke(Rect::from_min_max(r(-0.22, -0.4), r(0.18, -0.12)), 0.0, st, egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_min_max(r(-0.22, 0.08), r(0.22, 0.4)), 0.0, st, egui::StrokeKind::Middle);
        }
        Icon::Run | Icon::Continue => {
            p.add(Shape::convex_polygon(vec![r(-0.3, -0.42), r(0.42, 0.0), r(-0.3, 0.42)], col, Stroke::NONE));
            if icon == Icon::Continue {
                p.line_segment([r(-0.45, -0.42), r(-0.45, 0.42)], st);
            }
        }
        Icon::Debug => {
            p.circle_stroke(r(0.0, 0.08), 0.28 * s, st);
            p.circle_filled(r(0.0, -0.3), 0.13 * s, col);
            for (y, dx) in [(-0.05, 0.45), (0.12, 0.48), (0.3, 0.42)] {
                p.line_segment([r(-0.28, y), r(-dx, y + 0.06)], st);
                p.line_segment([r(0.28, y), r(dx, y + 0.06)], st);
            }
            p.line_segment([r(0.0, -0.12), r(0.0, 0.36)], Stroke::new(1.2, col));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(0.62 * s, 0.62 * s)), 2.0, col);
        }
        Icon::StepOver => {
            let pts: Vec<Pos2> = (0..=12).map(|k| {
                let a = std::f32::consts::PI * (1.0 + k as f32 / 12.0);
                r(0.32 * a.cos(), 0.1 + 0.32 * a.sin())
            }).collect();
            p.add(Shape::line(pts, st));
            p.add(Shape::convex_polygon(vec![r(0.2, 0.0), r(0.46, 0.04), r(0.34, 0.24)], col, Stroke::NONE));
            p.circle_filled(r(0.0, 0.38), 0.08 * s, col);
        }
        Icon::StepInto => {
            p.line_segment([r(0.0, -0.45), r(0.0, 0.12)], st);
            p.add(Shape::convex_polygon(vec![r(-0.18, 0.0), r(0.18, 0.0), r(0.0, 0.22)], col, Stroke::NONE));
            p.circle_filled(r(0.0, 0.4), 0.08 * s, col);
        }
        Icon::StepOut => {
            p.line_segment([r(0.0, 0.25), r(0.0, -0.3)], st);
            p.add(Shape::convex_polygon(vec![r(-0.18, -0.18), r(0.0, -0.42), r(0.18, -0.18)], col, Stroke::NONE));
            p.circle_filled(r(0.0, 0.4), 0.08 * s, col);
        }
        Icon::Search => {
            p.circle_stroke(r(-0.08, -0.08), 0.27 * s, st);
            p.line_segment([r(0.13, 0.13), r(0.42, 0.42)], Stroke::new(2.0, col));
        }
        Icon::Terminal => {
            p.rect_stroke(Rect::from_min_max(r(-0.45, -0.38), r(0.45, 0.38)), 2.0, st, egui::StrokeKind::Middle);
            p.add(Shape::line(vec![r(-0.25, -0.15), r(-0.08, 0.0), r(-0.25, 0.15)], st));
            p.line_segment([r(0.0, 0.18), r(0.22, 0.18)], st);
        }
        Icon::Sidebar => {
            p.rect_stroke(Rect::from_min_max(r(-0.45, -0.38), r(0.45, 0.38)), 2.0, st, egui::StrokeKind::Middle);
            p.rect_filled(Rect::from_min_max(r(-0.45, -0.38), r(-0.12, 0.38)), 1.0, col.gamma_multiply(0.6));
        }
        Icon::Minimize => {
            p.line_segment([r(-0.32, 0.0), r(0.32, 0.0)], st);
        }
        Icon::Maximize => {
            p.rect_stroke(Rect::from_min_max(r(-0.3, -0.3), r(0.3, 0.3)), 1.5, st, egui::StrokeKind::Middle);
        }
        Icon::Close => {
            p.line_segment([r(-0.3, -0.3), r(0.3, 0.3)], st);
            p.line_segment([r(0.3, -0.3), r(-0.3, 0.3)], st);
        }
        Icon::ChevronRight => {
            p.add(Shape::line(vec![r(-0.12, -0.25), r(0.13, 0.0), r(-0.12, 0.25)], st));
        }
        Icon::ChevronLeft => {
            p.add(Shape::line(vec![r(0.12, -0.25), r(-0.13, 0.0), r(0.12, 0.25)], st));
        }
        Icon::ChevronUp => {
            p.add(Shape::line(vec![r(-0.25, 0.1), r(0.0, -0.15), r(0.25, 0.1)], st));
        }
        Icon::ChevronDown => {
            p.add(Shape::line(vec![r(-0.25, -0.1), r(0.0, 0.15), r(0.25, -0.1)], st));
        }
        Icon::Shield => {
            p.add(Shape::closed_line(vec![r(0.0, -0.45), r(0.38, -0.3), r(0.32, 0.12), r(0.0, 0.45), r(-0.32, 0.12), r(-0.38, -0.3)], st));
        }
        Icon::Globe => {
            p.circle_stroke(c, 0.42 * s, st);
            p.line_segment([r(-0.42, 0.0), r(0.42, 0.0)], Stroke::new(1.2, col));
            let pts: Vec<Pos2> = (0..=16).map(|k| { let a = std::f32::consts::PI * (k as f32 / 16.0 - 0.5); r(0.18 * a.cos(), 0.42 * a.sin()) }).collect();
            p.add(Shape::line(pts.clone(), Stroke::new(1.2, col)));
            p.add(Shape::line(pts.into_iter().map(|q| pos2(2.0 * c.x - q.x, q.y)).collect(), Stroke::new(1.2, col)));
        }
        Icon::Split => {
            p.rect_stroke(Rect::from_min_max(r(-0.45, -0.38), r(0.45, 0.38)), 2.0, st, egui::StrokeKind::Middle);
            p.line_segment([r(0.0, -0.38), r(0.0, 0.38)], st);
        }
        Icon::Output => {
            for (y, w) in [(-0.28, 0.4), (-0.05, 0.3), (0.18, 0.4), (0.4, 0.2)] {
                p.line_segment([r(-0.4, y), r(-0.4 + 2.0 * w, y)], st);
            }
        }
    }
}

/// Bouton plat avec une icône ; survol discret.
pub fn button(ui: &mut egui::Ui, icon: Icon, col: Color32, hover_bg: Color32, tip: &str) -> egui::Response {
    sized_button(ui, icon, col, hover_bg, tip, Vec2::new(30.0, 28.0), 17.0)
}

pub fn small(ui: &mut egui::Ui, icon: Icon, col: Color32, hover_bg: Color32, tip: &str) -> egui::Response {
    sized_button(ui, icon, col, hover_bg, tip, Vec2::new(22.0, 20.0), 14.0)
}

fn sized_button(ui: &mut egui::Ui, icon: Icon, col: Color32, hover_bg: Color32, tip: &str, size: Vec2, s: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect.shrink(1.0), 5.0, hover_bg);
    }
    paint(ui.painter(), icon, rect.center(), s, col);
    resp.on_hover_text(tip)
}
