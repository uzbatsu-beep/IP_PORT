//! Small original line icons drawn in logical coordinates; no font dependency.
use egui::{Color32, Painter, Pos2, Rect, Stroke};

#[derive(Clone, Copy)]
pub enum Icon {
    Discovery,
    Devices,
    Diagnostics,
    Knowledge,
    Reports,
    Settings,
    Menu,
}

pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let p = |x: f32, y: f32| Pos2::new(rect.left() + x * rect.width(), rect.top() + y * rect.height());
    let stroke = Stroke::new(1.5, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([p(a.0, a.1), p(b.0, b.1)], stroke);
    };
    let circle = |x: f32, y: f32, r: f32| {
        painter.circle_stroke(p(x, y), r * rect.width(), stroke);
    };
    let outline = |x: f32, y: f32, w: f32, h: f32| {
        line((x, y), (x + w, y));
        line((x + w, y), (x + w, y + h));
        line((x + w, y + h), (x, y + h));
        line((x, y + h), (x, y));
    };
    match icon {
        Icon::Discovery => {
            circle(0.45, 0.45, 0.34);
            circle(0.45, 0.45, 0.17);
            line((0.45, 0.45), (0.86, 0.14));
        }
        Icon::Devices => {
            outline(0.31, 0.06, 0.38, 0.27);
            outline(0.05, 0.65, 0.35, 0.26);
            outline(0.6, 0.65, 0.35, 0.26);
            line((0.5, 0.33), (0.5, 0.5));
            line((0.22, 0.5), (0.78, 0.5));
            line((0.22, 0.5), (0.22, 0.65));
            line((0.78, 0.5), (0.78, 0.65));
        }
        Icon::Diagnostics => {
            line((0.05, 0.55), (0.25, 0.55));
            line((0.25, 0.55), (0.4, 0.18));
            line((0.4, 0.18), (0.6, 0.85));
            line((0.6, 0.85), (0.75, 0.55));
            line((0.75, 0.55), (0.95, 0.55));
        }
        Icon::Knowledge => {
            outline(0.13, 0.09, 0.74, 0.82);
            line((0.5, 0.09), (0.5, 0.91));
            line((0.23, 0.33), (0.4, 0.33));
            line((0.6, 0.33), (0.77, 0.33));
            line((0.23, 0.53), (0.4, 0.53));
            line((0.6, 0.53), (0.77, 0.53));
        }
        Icon::Reports => {
            outline(0.2, 0.08, 0.6, 0.84);
            for y in [0.34, 0.53, 0.72] {
                line((0.32, y), (0.68, y));
            }
        }
        Icon::Settings => {
            circle(0.5, 0.5, 0.3);
            circle(0.5, 0.5, 0.12);
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::FRAC_PI_4;
                let (sin, cos) = angle.sin_cos();
                line((0.5 + cos * 0.3, 0.5 + sin * 0.3), (0.5 + cos * 0.44, 0.5 + sin * 0.44));
            }
        }
        Icon::Menu => {
            for y in [0.25, 0.5, 0.75] {
                line((0.12, y), (0.88, y));
            }
        }
    }
}
