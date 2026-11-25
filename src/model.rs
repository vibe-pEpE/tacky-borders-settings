//! The settings this app knows how to edit, and how they map to/from config.yaml.

use crate::yaml_edit::{NewValue, YamlDoc};
use serde_yaml_ng::Value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Radius {
    Auto,
    Square,
    Round,
    RoundSmall,
    Custom(f32),
}

impl Radius {
    pub const PRESETS: [(Radius, &'static str); 4] = [
        (Radius::Auto, "Auto (follow Windows)"),
        (Radius::Square, "Square"),
        (Radius::RoundSmall, "Slightly rounded"),
        (Radius::Round, "Rounded"),
    ];

    /// Approximate radius in pixels, used only for the preview.
    pub fn preview_px(&self) -> f32 {
        match self {
            Radius::Auto | Radius::Round => 8.0,
            Radius::RoundSmall => 4.0,
            Radius::Square => 0.0,
            Radius::Custom(r) => r.max(0.0),
        }
    }

    fn to_yaml(self) -> String {
        match self {
            Radius::Auto => "Auto".into(),
            Radius::Square => "Square".into(),
            Radius::Round => "Round".into(),
            Radius::RoundSmall => "RoundSmall".into(),
            Radius::Custom(r) => fmt_num(r),
        }
    }
}

/// RGBA, not premultiplied.
pub type Rgba = [u8; 4];

#[derive(Clone, Debug, PartialEq)]
pub enum ColorSetting {
    /// Windows accent color
    Accent,
    Solid(Rgba),
    Gradient { from: Rgba, to: Rgba, angle: f32 },
    /// A format this app can't edit yet (theme-aware, 3+ colors, ...). Left untouched.
    Advanced(String),
}

impl ColorSetting {
    fn from_yaml(v: Option<&Value>) -> Self {
        let Some(v) = v else {
            return ColorSetting::Accent; // tacky-borders' default
        };
        match v {
            Value::String(s) if s.eq_ignore_ascii_case("accent") => ColorSetting::Accent,
            Value::String(s) => match parse_hex(s) {
                Some(c) => ColorSetting::Solid(c),
                None => ColorSetting::Advanced(describe(v)),
            },
            Value::Mapping(m) => {
                let colors = m.get("colors").and_then(|c| c.as_sequence());
                let dir = m.get("direction");
                if let (Some(colors), Some(dir)) = (colors, dir) {
                    let hexes: Vec<_> = colors
                        .iter()
                        .filter_map(|c| c.as_str().and_then(parse_hex))
                        .collect();
                    if colors.len() == 2 && hexes.len() == 2 {
                        if let Some(angle) = parse_direction(dir) {
                            return ColorSetting::Gradient {
                                from: hexes[0],
                                to: hexes[1],
                                angle,
                            };
                        }
                    }
                }
                ColorSetting::Advanced(describe(v))
            }
            _ => ColorSetting::Advanced(describe(v)),
        }
    }

    fn to_yaml(&self) -> Option<NewValue> {
        match self {
            ColorSetting::Accent => Some(NewValue::Inline("\"accent\"".into())),
            ColorSetting::Solid(c) => Some(NewValue::Inline(format!("\"{}\"", to_hex(*c)))),
            ColorSetting::Gradient { from, to, angle } => Some(NewValue::Block(vec![
                format!("colors: [\"{}\", \"{}\"]", to_hex(*from), to_hex(*to)),
                format!("direction: {}deg", fmt_num(angle.round())),
            ])),
            ColorSetting::Advanced(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Offset {
    Uniform(i32),
    /// Per-side offsets: shown read-only, left untouched unless replaced.
    PerSide(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub border_width: f32,
    pub border_offset: Offset,
    pub border_radius: Radius,
    pub active_color: ColorSetting,
    pub inactive_color: ColorSetting,
    pub animations_enabled: bool,
    pub effects_enabled: bool,
}

impl Settings {
    pub fn from_yaml(text: &str) -> Result<Self, String> {
        let root: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_yaml_ng::from_str(text).map_err(|e| e.to_string())?
        };
        let global = root.get("global");
        let g = |k: &str| global.and_then(|g| g.get(k));

        let border_width = g("border_width").and_then(Value::as_f64).unwrap_or(4.0) as f32;

        let border_offset = match g("border_offset") {
            None => Offset::Uniform(-1),
            Some(v) => match v.as_i64() {
                Some(n) => Offset::Uniform(n as i32),
                None => Offset::PerSide(describe(v)),
            },
        };

        let border_radius = match g("border_radius") {
            None => Radius::Auto,
            Some(v) => match (v.as_str(), v.as_f64()) {
                (Some("Square"), _) => Radius::Square,
                (Some("Round"), _) => Radius::Round,
                (Some("RoundSmall"), _) => Radius::RoundSmall,
                (_, Some(n)) if n == -1.0 => Radius::Auto, // legacy value
                (_, Some(n)) => Radius::Custom(n as f32),
                _ => Radius::Auto,
            },
        };

        let enabled = |section: &str| {
            g(section)
                .and_then(|s| s.get("enabled"))
                .map(as_bool)
                .unwrap_or(false)
        };

        Ok(Settings {
            border_width,
            border_offset,
            border_radius,
            active_color: ColorSetting::from_yaml(g("active_color")),
            inactive_color: ColorSetting::from_yaml(g("inactive_color")),
            animations_enabled: enabled("animations"),
            effects_enabled: enabled("effects"),
        })
    }

    /// Apply only the settings that differ from `original` to the YAML text.
    pub fn apply_changes(&self, original: &Settings, text: &str) -> String {
        let mut doc = YamlDoc::parse(text);
        if self.border_width != original.border_width {
            doc.set(
                &["global", "border_width"],
                NewValue::Inline(fmt_num(self.border_width)),
            );
        }
        if self.border_offset != original.border_offset {
            if let Offset::Uniform(n) = self.border_offset {
                doc.set(&["global", "border_offset"], NewValue::Inline(n.to_string()));
            }
        }
        if self.border_radius != original.border_radius {
            doc.set(
                &["global", "border_radius"],
                NewValue::Inline(self.border_radius.to_yaml()),
            );
        }
        for (key, now, before) in [
            ("active_color", &self.active_color, &original.active_color),
            ("inactive_color", &self.inactive_color, &original.inactive_color),
        ] {
            if now != before {
                if let Some(v) = now.to_yaml() {
                    doc.set(&["global", key], v);
                }
            }
        }
        for (key, now, before) in [
            ("animations", self.animations_enabled, original.animations_enabled),
            ("effects", self.effects_enabled, original.effects_enabled),
        ] {
            if now != before {
                doc.set(&["global", key, "enabled"], NewValue::Inline(yaml_bool(now)));
            }
        }
        doc.to_text()
    }
}

fn as_bool(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::String(s) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn yaml_bool(b: bool) -> String {
    // tacky-borders' own config uses this capitalization
    if b { "True" } else { "False" }.into()
}

fn fmt_num(n: f32) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{:.1}", n)
    }
}

fn describe(v: &Value) -> String {
    serde_yaml_ng::to_string(v)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Supports #RGB, #RGBA, #RRGGBB, #RRGGBBAA (same as tacky-borders).
pub fn parse_hex(s: &str) -> Option<Rgba> {
    let h = s.trim().strip_prefix('#')?;
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let d = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|v| v * 17);
    let dd = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some(match h.len() {
        3 => [d(0)?, d(1)?, d(2)?, 255],
        4 => [d(0)?, d(1)?, d(2)?, d(3)?],
        6 => [dd(0)?, dd(2)?, dd(4)?, 255],
        8 => [dd(0)?, dd(2)?, dd(4)?, dd(6)?],
        _ => return None,
    })
}

pub fn to_hex(c: Rgba) -> String {
    if c[3] == 255 {
        format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
    }
}

/// Returns the gradient angle in degrees (0 = left→right, 90 = bottom→top).
fn parse_direction(v: &Value) -> Option<f32> {
    if let Some(s) = v.as_str() {
        return s.strip_suffix("deg")?.trim().parse().ok();
    }
    let pt = |k: &str| -> Option<(f64, f64)> {
        let seq = v.get(k)?.as_sequence()?;
        Some((seq.first()?.as_f64()?, seq.get(1)?.as_f64()?))
    };
    let (s, e) = (pt("start")?, pt("end")?);
    // tacky-borders flips y so that angles point "up" like on paper.
    let (dx, dy) = (e.0 - s.0, -(e.1 - s.1));
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    Some(dy.atan2(dx).to_degrees().round() as f32)
}

/// Gradient start/end in 0..1 coordinates (top-left origin), same math as tacky-borders.
pub fn angle_to_coords(angle: f32) -> ([f32; 2], [f32; 2]) {
    let rad = (angle as f64).to_radians();
    let (x, y) = (rad.cos(), -rad.sin());
    let scalar = 0.5 / x.abs().max(y.abs());
    let (xd, yd) = ((x * scalar) as f32, (y * scalar) as f32);
    ([0.5 - xd, 0.5 - yd], [0.5 + xd, 0.5 + yd])
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = include_str!("../tests/default_config.yaml");

    #[test]
    fn reads_default_config() {
        let s = Settings::from_yaml(DEFAULT).unwrap();
        assert_eq!(s.border_width, 3.0);
        assert_eq!(s.border_offset, Offset::Uniform(-1));
        assert_eq!(s.border_radius, Radius::Auto);
        assert_eq!(
            s.active_color,
            ColorSetting::Gradient {
                from: [0x62, 0x74, 0xe7, 255],
                to: [0x87, 0x52, 0xa3, 255],
                angle: 45.0
            }
        );
        // start [0,1] end [1,0] = bottom-left to top-right = 45deg
        assert!(matches!(s.inactive_color, ColorSetting::Gradient { angle, .. } if angle == 45.0));
        assert!(!s.animations_enabled);
        assert!(!s.effects_enabled);
    }

    #[test]
    fn no_changes_means_identical_file() {
        let s = Settings::from_yaml(DEFAULT).unwrap();
        assert_eq!(s.apply_changes(&s, DEFAULT), DEFAULT);
    }

    #[test]
    fn changes_roundtrip() {
        let before = Settings::from_yaml(DEFAULT).unwrap();
        let mut after = before.clone();
        after.border_width = 6.0;
        after.border_offset = Offset::Uniform(2);
        after.border_radius = Radius::Custom(12.5);
        after.active_color = ColorSetting::Solid([255, 0, 0, 128]);
        after.inactive_color = ColorSetting::Accent;
        after.animations_enabled = true;
        after.effects_enabled = true;

        let text = after.apply_changes(&before, DEFAULT);
        let reread = Settings::from_yaml(&text).unwrap();
        assert_eq!(reread, after);

        // Comments and window rules survive
        assert!(text.contains("# border_width: Width of the border (in pixels)"));
        assert!(text.contains("name: \"seelen-ui\""));
        // Only the animations/effects toggles changed, not the animation list
        assert!(text.contains("type: ReverseSpiral"));
    }

    #[test]
    fn advanced_color_is_left_alone() {
        let yaml = "global:\n  active_color:\n    dark: \"#fff\"\n    light: \"#000\"\n  border_width: 2\n";
        let before = Settings::from_yaml(yaml).unwrap();
        assert!(matches!(before.active_color, ColorSetting::Advanced(_)));
        let mut after = before.clone();
        after.border_width = 5.0;
        let text = after.apply_changes(&before, yaml);
        assert_eq!(text, yaml.replace("border_width: 2", "border_width: 5"));
    }

    #[test]
    fn hex_formats() {
        assert_eq!(parse_hex("#fff"), Some([255, 255, 255, 255]));
        assert_eq!(parse_hex("#0f08"), Some([0, 255, 0, 136]));
        assert_eq!(parse_hex("#6274e7"), Some([0x62, 0x74, 0xe7, 255]));
        assert_eq!(parse_hex("#6274e780"), Some([0x62, 0x74, 0xe7, 0x80]));
        assert_eq!(parse_hex("accent"), None);
        assert_eq!(to_hex([0x62, 0x74, 0xe7, 255]), "#6274e7");
    }

    #[test]
    fn angle_math_matches_tacky_borders() {
        let (s, e) = angle_to_coords(45.0);
        assert!((s[0] - 0.0).abs() < 1e-5 && (s[1] - 1.0).abs() < 1e-5);
        assert!((e[0] - 1.0).abs() < 1e-5 && (e[1] - 0.0).abs() < 1e-5);
    }
}
