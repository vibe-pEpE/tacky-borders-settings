//! A live preview: two mock windows with the configured borders drawn around them.

use crate::model::{ColorSetting, Offset, Rgba, Settings, angle_to_coords};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Vec2, epaint::Mesh};

/// Stand-in for the Windows accent color (Windows' default blue).
const ACCENT: Rgba = [0x00, 0x78, 0xd4, 255];

/// Points around a rounded rectangle, clockwise, with a fixed count per corner
/// so the inner and outer outlines line up vertex-by-vertex.
fn rounded_points(rect: Rect, radius: f32) -> Vec<Pos2> {
    const SEG: usize = 10;
    let r = radius.clamp(0.0, rect.width().min(rect.height()) / 2.0);
    let corners = [
        (Pos2::new(rect.right() - r, rect.top() + r), -90.0f32),
        (Pos2::new(rect.right() - r, rect.bottom() - r), 0.0),
        (Pos2::new(rect.left() + r, rect.bottom() - r), 90.0),
        (Pos2::new(rect.left() + r, rect.top() + r), 180.0),
    ];
    let mut pts = Vec::with_capacity(4 * (SEG + 1));
    for (c, start) in corners {
        for i in 0..=SEG {
            let a = (start + 90.0 * i as f32 / SEG as f32).to_radians();
            pts.push(c + Vec2::new(a.cos(), a.sin()) * r);
        }
    }
    pts
}

fn lerp(a: Rgba, b: Rgba, t: f32) -> Color32 {
    let m = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(m(0), m(1), m(2), m(3))
}

/// Returns a function that gives the border color at a point.
fn color_fn(c: &ColorSetting, bounds: Rect) -> Box<dyn Fn(Pos2) -> Color32> {
    match c.clone() {
        ColorSetting::Accent => Box::new(|_| lerp(ACCENT, ACCENT, 0.0)),
        ColorSetting::Solid(x) => Box::new(move |_| lerp(x, x, 0.0)),
        ColorSetting::Gradient { from, to, angle } => {
            let (s, e) = angle_to_coords(angle);
            let (s, e) = (Vec2::new(s[0], s[1]), Vec2::new(e[0], e[1]));
            let d = e - s;
            let len2 = d.length_sq().max(1e-6);
            Box::new(move |p| {
                let n = (p - bounds.min) / bounds.size();
                let t = ((n - s).dot(d) / len2).clamp(0.0, 1.0);
                lerp(from, to, t)
            })
        }
        // Can't render the advanced formats faithfully; show a neutral gray.
        ColorSetting::Advanced(_) => Box::new(|_| Color32::from_gray(140)),
    }
}

fn border_shape(window: Rect, s: &Settings, color: &ColorSetting) -> Shape {
    let offset = match s.border_offset {
        Offset::Uniform(n) => n as f32,
        Offset::PerSide(_) => 0.0,
    };
    let w = s.border_width.max(0.0);
    // The border's inner edge sits `offset` px from the window edge.
    let inner = window.expand(offset);
    let outer = inner.expand(w);
    let r = s.border_radius.preview_px();
    let outer_pts = rounded_points(outer, r + w);
    let inner_pts = rounded_points(inner, r);
    let col = color_fn(color, outer);

    let mut mesh = Mesh::default();
    for (o, i) in outer_pts.iter().zip(&inner_pts) {
        mesh.colored_vertex(*o, col(*o));
        mesh.colored_vertex(*i, col(*i));
    }
    let n = outer_pts.len() as u32;
    for k in 0..n {
        let (a, b) = (2 * k, 2 * ((k + 1) % n));
        mesh.add_triangle(a, a + 1, b);
        mesh.add_triangle(a + 1, b + 1, b);
    }
    Shape::mesh(mesh)
}

fn mock_window(painter: &egui::Painter, rect: Rect, radius: f32, title: &str, active: bool) {
    let bg = if active { Color32::from_gray(38) } else { Color32::from_gray(30) };
    painter.rect_filled(rect, radius, bg);
    let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 26.0));
    painter.text(
        bar.left_center() + Vec2::new(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(12.0),
        Color32::from_gray(if active { 220 } else { 130 }),
    );
    for (i, line) in [0.8, 0.6, 0.7].iter().enumerate() {
        let y = rect.top() + 44.0 + i as f32 * 16.0;
        let line_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 12.0, y),
            Vec2::new((rect.width() - 24.0) * line, 6.0),
        );
        painter.rect_filled(line_rect, 3.0, Color32::from_gray(if active { 70 } else { 50 }));
    }
}

pub fn draw(ui: &mut egui::Ui, s: &Settings) {
    let size = Vec2::new(ui.available_width().min(480.0), 200.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let area = resp.rect;
    painter.rect_filled(area, 6.0, Color32::from_rgb(24, 26, 38));

    let r = s.border_radius.preview_px();
    let win = Vec2::new(area.width() * 0.48, 120.0);

    let back = Rect::from_min_size(area.min + Vec2::new(28.0, 24.0), win);
    let front = Rect::from_min_size(area.min + Vec2::new(area.width() * 0.40, 56.0), win);

    let painter = painter.with_clip_rect(area);
    mock_window(&painter, back, r, "Inactive window", false);
    painter.add(border_shape(back, s, &s.inactive_color));
    mock_window(&painter, front, r, "Active window", true);
    painter.add(border_shape(front, s, &s.active_color));
}
