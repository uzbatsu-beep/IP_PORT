use egui::{Color32, Stroke, Visuals};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const ACCENT_DARK: Color32 = Color32::from_rgb(0x3d, 0x9b, 0xff);
const SUCCESS: Color32 = Color32::from_rgb(0x4c, 0xc3, 0x8a);
const WARNING: Color32 = Color32::from_rgb(0xf0, 0xb4, 0x4c);
pub const ERROR: Color32 = Color32::from_rgb(0xf2, 0x6d, 0x6d);

#[cfg(test)]
pub const ACCENT: Color32 = ACCENT_DARK;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Light,
    #[default]
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Palette {
    #[default]
    Blue,
    Slate,
    Teal,
    Emerald,
    Violet,
    Amber,
    Monochrome,
    Indigo,
}

impl Palette {
    pub const ALL: [Self; 8] = [
        Self::Blue,
        Self::Slate,
        Self::Teal,
        Self::Emerald,
        Self::Violet,
        Self::Amber,
        Self::Monochrome,
        Self::Indigo,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Blue => "Classic Blue",
            Self::Slate => "Industrial Slate",
            Self::Teal => "Teal",
            Self::Emerald => "Emerald",
            Self::Violet => "Violet",
            Self::Amber => "Amber",
            Self::Monochrome => "Monochrome",
            Self::Indigo => "Indigo",
        }
    }

    fn color(self, dark: bool) -> Color32 {
        let (r, g, b) = match (self, dark) {
            (Self::Blue, true) => return ACCENT_DARK,
            (Self::Blue, false) => (21, 75, 157),
            (Self::Slate, true) => (148, 163, 184),
            (Self::Slate, false) => (71, 85, 105),
            (Self::Teal, true) => (45, 212, 191),
            (Self::Teal, false) => (15, 103, 94),
            (Self::Emerald, true) => (52, 211, 153),
            (Self::Emerald, false) => (18, 103, 65),
            (Self::Violet, true) => (196, 181, 253),
            (Self::Violet, false) => (107, 59, 170),
            (Self::Amber, true) => (251, 191, 36),
            (Self::Amber, false) => (137, 75, 10),
            (Self::Monochrome, true) => (190, 190, 190),
            (Self::Monochrome, false) => (75, 75, 75),
            (Self::Indigo, true) => (165, 180, 252),
            (Self::Indigo, false) => (67, 56, 166),
        };
        Color32::from_rgb(r, g, b)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Theme,
    pub palette: Palette,
}

#[derive(Serialize, Deserialize)]
struct Settings {
    schema_version: u32,
    appearance: Appearance,
}

pub fn settings_path() -> Option<PathBuf> {
    let dir = crate::oui_store::default_dir()?;
    Some(dir.parent()?.join("appearance.json"))
}

pub fn load(path: &Path) -> io::Result<Appearance> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Appearance::default());
        }
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Appearance settings exceed 4 KiB",
        ));
    }
    let settings: Settings = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if settings.schema_version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unsupported appearance settings version",
        ));
    }
    Ok(settings.appearance)
}

pub fn save(path: &Path, appearance: Appearance) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing settings directory"))?;
    fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec_pretty(&Settings {
        schema_version: 1,
        appearance,
    })
    .map_err(io::Error::other)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let pending = dir.join(format!(".appearance-{}-{nonce}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    let result = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = result.and_then(|_| fs::rename(&pending, path));
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

pub fn accent(ui: &egui::Ui) -> Color32 {
    ui.visuals().hyperlink_color
}

pub fn success(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        SUCCESS
    } else {
        Color32::from_rgb(19, 107, 61)
    }
}

pub fn warning(ui: &egui::Ui) -> Color32 {
    ui.visuals().warn_fg_color
}

pub fn error(ui: &egui::Ui) -> Color32 {
    ui.visuals().error_fg_color
}

pub fn tint(background: Color32, foreground: Color32, percent: u16) -> Color32 {
    let mix =
        |a: u8, b: u8| ((u16::from(a) * (100 - percent) + u16::from(b) * percent) / 100) as u8;
    Color32::from_rgb(
        mix(background.r(), foreground.r()),
        mix(background.g(), foreground.g()),
        mix(background.b(), foreground.b()),
    )
}

fn visuals(appearance: Appearance) -> Visuals {
    let dark = appearance.theme == Theme::Dark;
    let mut visuals = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    if dark {
        visuals.panel_fill = Color32::from_rgb(0x16, 0x18, 0x1d);
        visuals.window_fill = Color32::from_rgb(0x1c, 0x1f, 0x26);
        visuals.extreme_bg_color = Color32::from_rgb(0x11, 0x13, 0x17);
        visuals.faint_bg_color = Color32::from_rgb(0x20, 0x24, 0x2c);
        visuals.override_text_color = Some(Color32::from_rgb(0xd8, 0xdd, 0xe6));
        visuals.widgets.noninteractive.bg_stroke =
            Stroke::new(1.0, Color32::from_rgb(0x2c, 0x31, 0x3b));
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(0x25, 0x2a, 0x33);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x25, 0x2a, 0x33);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x2f, 0x36, 0x42);
        visuals.warn_fg_color = WARNING;
        visuals.error_fg_color = ERROR;
    } else {
        visuals.panel_fill = Color32::from_rgb(245, 247, 250);
        visuals.window_fill = Color32::WHITE;
        visuals.extreme_bg_color = Color32::from_rgb(255, 255, 255);
        visuals.faint_bg_color = Color32::from_rgb(234, 238, 244);
        visuals.override_text_color = Some(Color32::from_rgb(31, 41, 55));
        visuals.warn_fg_color = Color32::from_rgb(133, 77, 14);
        visuals.error_fg_color = Color32::from_rgb(159, 29, 35);
    }
    let accent = appearance.palette.color(dark);
    visuals.selection.bg_fill = tint(visuals.panel_fill, accent, 12);
    visuals.selection.stroke = Stroke::new(1.0, accent);
    visuals.hyperlink_color = accent;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, accent);
    visuals.widgets.active.bg_fill = tint(visuals.panel_fill, accent, 15);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    visuals
}

pub fn apply_appearance(ctx: &egui::Context, appearance: Appearance) {
    let (theme, preference) = match appearance.theme {
        Theme::Dark => (egui::Theme::Dark, egui::ThemePreference::Dark),
        Theme::Light => (egui::Theme::Light, egui::ThemePreference::Light),
    };
    ctx.set_visuals_of(theme, visuals(appearance));
    ctx.set_theme(preference);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 26.0;
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
    });
    ctx.request_repaint();
}

#[cfg(test)]
pub fn apply(ctx: &egui::Context) {
    apply_appearance(ctx, Appearance::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(a: Color32, b: Color32) -> f64 {
        fn luminance(c: Color32) -> f64 {
            let linear = |v: u8| {
                let x = f64::from(v) / 255.0;
                if x <= 0.04045 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
        }
        let a = luminance(a);
        let b = luminance(b);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn all_appearances_have_readable_text_and_accents() {
        for theme in [Theme::Dark, Theme::Light] {
            for palette in Palette::ALL {
                let v = visuals(Appearance { theme, palette });
                assert_eq!(v.dark_mode, theme == Theme::Dark);
                for background in [v.panel_fill, v.window_fill, v.selection.bg_fill] {
                    assert!(contrast(v.text_color(), background) >= 4.5);
                    assert!(contrast(v.hyperlink_color, background) >= 4.5);
                }
                let default = visuals(Appearance {
                    theme,
                    palette: Palette::Blue,
                });
                assert_eq!(v.error_fg_color, default.error_fg_color);
                assert_eq!(v.warn_fg_color, default.warn_fg_color);
            }
        }
    }

    #[test]
    fn settings_round_trip_overwrite_and_corruption() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ip-port-appearance-{nonce}"));
        let path = dir.join("appearance.json");
        assert_eq!(load(&path).unwrap(), Appearance::default());
        for theme in [Theme::Dark, Theme::Light] {
            for palette in Palette::ALL {
                let expected = Appearance { theme, palette };
                save(&path, expected).unwrap();
                assert_eq!(load(&path).unwrap(), expected);
            }
        }
        fs::write(&path, b"broken").unwrap();
        assert!(load(&path).is_err());
        save(&path, Appearance::default()).unwrap();
        assert_eq!(load(&path).unwrap(), Appearance::default());
        fs::write(&path, b"{\"schema_version\":2,\"appearance\":{}}").unwrap();
        assert!(load(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
