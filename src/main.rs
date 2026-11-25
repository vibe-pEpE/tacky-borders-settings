// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod model;
mod preview;
mod yaml_edit;

use eframe::egui;
use model::{ColorSetting, Offset, Radius, Settings};
use std::path::PathBuf;

fn config_path() -> Option<PathBuf> {
    // Same lookup as tacky-borders itself.
    let dir = std::env::var("TACKY_BORDERS_CONFIG_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".config").join("tacky-borders")))?;
    Some(dir.join("config.yaml"))
}

enum Status {
    None,
    Ok(String),
    Error(String),
}

struct App {
    path: Option<PathBuf>,
    /// File contents as last loaded/saved
    text: String,
    /// Settings as last loaded/saved
    saved: Settings,
    /// Settings being edited
    edit: Settings,
    status: Status,
    load_error: Option<String>,
}

impl App {
    fn new() -> Self {
        let mut app = App {
            path: config_path(),
            text: String::new(),
            saved: Settings::from_yaml("").unwrap(),
            edit: Settings::from_yaml("").unwrap(),
            status: Status::None,
            load_error: None,
        };
        app.reload();
        app
    }

    fn reload(&mut self) {
        self.load_error = None;
        let Some(path) = &self.path else {
            self.load_error = Some("Could not find your user folder.".into());
            return;
        };
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.load_error = Some(format!(
                    "Could not read {}\n({e})\n\nStart tacky-borders once so it creates the config file.",
                    path.display()
                ));
                return;
            }
        };
        match Settings::from_yaml(&text) {
            Ok(s) => {
                self.text = text;
                self.saved = s.clone();
                self.edit = s;
                self.status = Status::Ok("Loaded config.".into());
            }
            Err(e) => {
                self.load_error = Some(format!(
                    "Your config.yaml has a syntax error, so this app won't touch it:\n\n{e}"
                ));
            }
        }
    }

    fn save(&mut self) {
        let Some(path) = self.path.clone() else { return };

        // If the file changed on disk since we loaded it, apply our changes on top of the new
        // version instead of overwriting someone else's edits.
        let current = std::fs::read_to_string(&path).unwrap_or_else(|_| self.text.clone());
        let base = if current != self.text {
            match Settings::from_yaml(&current) {
                Ok(s) => s,
                Err(e) => {
                    self.status = Status::Error(format!("config.yaml changed and is now invalid: {e}"));
                    return;
                }
            }
        } else {
            self.saved.clone()
        };

        let new_text = self.edit.apply_changes(&base, &current);

        // Safety net: never write something tacky-borders can't read back.
        match Settings::from_yaml(&new_text) {
            Ok(check) if check == self.edit => {}
            Ok(_) | Err(_) => {
                self.status = Status::Error(
                    "Internal error: the new config didn't verify, nothing was saved.".into(),
                );
                return;
            }
        }

        let backup = path.with_extension("yaml.bak");
        if !backup.exists() {
            let _ = std::fs::copy(&path, &backup);
        }

        match std::fs::write(&path, &new_text) {
            Ok(()) => {
                self.text = new_text;
                self.saved = self.edit.clone();
                self.status = Status::Ok("Saved! tacky-borders reloads it automatically.".into());
            }
            Err(e) => self.status = Status::Error(format!("Could not save: {e}")),
        }
    }

    fn open_folder(&self) {
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            #[cfg(windows)]
            let _ = std::process::Command::new("explorer").arg(dir).spawn();
            #[cfg(not(windows))]
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }
}

fn color_button(ui: &mut egui::Ui, rgba: &mut model::Rgba) {
    let mut c = egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
    if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::OnlyBlend)
        .changed()
    {
        *rgba = c.to_srgba_unmultiplied();
    }
}

fn color_editor(ui: &mut egui::Ui, id: &str, c: &mut ColorSetting) {
    let mode = match c {
        ColorSetting::Accent => "Windows accent color",
        ColorSetting::Solid(_) => "Single color",
        ColorSetting::Gradient { .. } => "Gradient",
        ColorSetting::Advanced(_) => "Advanced (kept as is)",
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(mode)
        .width(190.0)
        .show_ui(ui, |ui| {
            let current = c.clone();
            let base = match &current {
                ColorSetting::Solid(x) => *x,
                ColorSetting::Gradient { from, .. } => *from,
                _ => [0x62, 0x74, 0xe7, 255],
            };
            let to = match &current {
                ColorSetting::Gradient { to, .. } => *to,
                _ => [0x87, 0x52, 0xa3, 255],
            };
            if ui
                .selectable_label(matches!(current, ColorSetting::Accent), "Windows accent color")
                .clicked()
            {
                *c = ColorSetting::Accent;
            }
            if ui
                .selectable_label(matches!(current, ColorSetting::Solid(_)), "Single color")
                .clicked()
                && !matches!(current, ColorSetting::Solid(_))
            {
                *c = ColorSetting::Solid(base);
            }
            if ui
                .selectable_label(matches!(current, ColorSetting::Gradient { .. }), "Gradient")
                .clicked()
                && !matches!(current, ColorSetting::Gradient { .. })
            {
                *c = ColorSetting::Gradient { from: base, to, angle: 45.0 };
            }
        });

    match c {
        ColorSetting::Accent => {
            ui.label("Uses the accent color from Windows settings.");
        }
        ColorSetting::Solid(rgba) => {
            ui.horizontal(|ui| {
                color_button(ui, rgba);
                ui.monospace(model::to_hex(*rgba));
            });
        }
        ColorSetting::Gradient { from, to, angle } => {
            ui.horizontal(|ui| {
                ui.label("From");
                color_button(ui, from);
                ui.label("to");
                color_button(ui, to);
                if ui.small_button("⇄").on_hover_text("Swap colors").clicked() {
                    std::mem::swap(from, to);
                }
            });
            ui.add(egui::Slider::new(angle, -180.0..=180.0).text("angle °").step_by(1.0));
        }
        ColorSetting::Advanced(yaml) => {
            ui.label(
                "This color uses a format this app can't edit yet (for example separate \
                 dark/light theme colors). It will be left exactly as it is, unless you \
                 pick another option above.",
            );
            ui.add(
                egui::TextEdit::multiline(&mut yaml.as_str())
                    .code_editor()
                    .desired_rows(2),
            );
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let dirty = self.edit != self.saved;

        egui::Panel::bottom("bottom").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let save = ui.add_enabled(
                    dirty && self.load_error.is_none(),
                    egui::Button::new(egui::RichText::new("💾 Save").strong())
                        .min_size(egui::vec2(90.0, 28.0)),
                );
                if save.clicked() {
                    self.save();
                }
                if ui
                    .add_enabled(dirty, egui::Button::new("Undo changes"))
                    .clicked()
                {
                    self.edit = self.saved.clone();
                }
                if ui.button("⟳ Reload file").clicked() {
                    self.reload();
                }
                if ui.button("📁 Open config folder").clicked() {
                    self.open_folder();
                }
            });
            match &self.status {
                Status::None => {}
                Status::Ok(m) if !dirty => {
                    ui.colored_label(egui::Color32::from_rgb(80, 200, 120), m);
                }
                Status::Ok(_) => {
                    ui.label("You have unsaved changes.");
                }
                Status::Error(m) => {
                    ui.colored_label(egui::Color32::from_rgb(230, 90, 90), m);
                }
            }
            ui.add_space(4.0);
        });

        egui::CentralPanel::default_margins().show(ui, |ui| {
            if let Some(err) = &self.load_error {
                ui.heading("Can't load config");
                ui.label(err);
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Preview");
                preview::draw(ui, &self.edit);
                ui.add_space(8.0);

                ui.heading("Border");
                egui::Grid::new("border")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Width");
                        ui.add(
                            egui::Slider::new(&mut self.edit.border_width, 0.0..=20.0)
                                .step_by(1.0)
                                .suffix(" px"),
                        );
                        ui.end_row();

                        ui.label("Offset");
                        match &mut self.edit.border_offset {
                            Offset::Uniform(n) => {
                                ui.add(egui::Slider::new(n, -10..=10).suffix(" px"))
                                    .on_hover_text(
                                        "Negative = border moves inside the window, \
                                         positive = outside.",
                                    );
                            }
                            Offset::PerSide(_) => {
                                ui.horizontal(|ui| {
                                    ui.label("Set per side in config.");
                                    if ui.small_button("Use one value").clicked() {
                                        self.edit.border_offset = Offset::Uniform(-1);
                                    }
                                });
                            }
                        }
                        ui.end_row();

                        ui.label("Corners");
                        ui.horizontal(|ui| {
                            let r = &mut self.edit.border_radius;
                            let text = match r {
                                Radius::Custom(_) => "Custom",
                                _ => Radius::PRESETS
                                    .iter()
                                    .find(|(p, _)| p == r)
                                    .map(|(_, n)| *n)
                                    .unwrap_or("Auto"),
                            };
                            egui::ComboBox::from_id_salt("radius")
                                .selected_text(text)
                                .width(190.0)
                                .show_ui(ui, |ui| {
                                    for (preset, name) in Radius::PRESETS {
                                        ui.selectable_value(r, preset, name);
                                    }
                                    if ui
                                        .selectable_label(matches!(r, Radius::Custom(_)), "Custom")
                                        .clicked()
                                        && !matches!(r, Radius::Custom(_))
                                    {
                                        *r = Radius::Custom(r.preview_px());
                                    }
                                });
                            if let Radius::Custom(px) = r {
                                ui.add(egui::Slider::new(px, 0.0..=30.0).step_by(1.0).suffix(" px"));
                            }
                        });
                        ui.end_row();
                    });

                ui.add_space(8.0);
                ui.heading("Active window color");
                color_editor(ui, "active", &mut self.edit.active_color);

                ui.add_space(8.0);
                ui.heading("Inactive window color");
                color_editor(ui, "inactive", &mut self.edit.inactive_color);

                ui.add_space(8.0);
                ui.heading("Extras");
                ui.checkbox(&mut self.edit.animations_enabled, "Animations")
                    .on_hover_text("Spiral / fade animations as defined in the config.");
                ui.checkbox(&mut self.edit.effects_enabled, "Effects (shadow / glow)")
                    .on_hover_text("Needs rendering_backend: V2. May use more CPU/GPU.");

                ui.add_space(10.0);
                if let Some(p) = &self.path {
                    ui.small(format!("Config: {}", p.display()));
                }
            });
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Tacky Borders Settings")
            .with_inner_size([520.0, 760.0])
            .with_min_inner_size([420.0, 400.0]),
        // Windows: wgpu (DirectX 12). Elsewhere: OpenGL, which also works in VMs without a GPU.
        #[cfg(not(windows))]
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Tacky Borders Settings",
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    )
}
