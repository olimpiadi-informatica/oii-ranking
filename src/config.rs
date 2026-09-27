//! Settings, read from config.toml on top of the copy built into the binary.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const DEFAULT_CONFIG: &str = include_str!("../config.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Medal {
    Gold,
    Silver,
    Bronze,
    Mention,
}

impl Medal {
    pub const ALL: [Medal; 4] = [Medal::Gold, Medal::Silver, Medal::Bronze, Medal::Mention];

    pub fn key(self) -> &'static str {
        match self {
            Medal::Gold => "gold",
            Medal::Silver => "silver",
            Medal::Bronze => "bronze",
            Medal::Mention => "mention",
        }
    }

    /// Name of the video file
    pub fn title(self) -> &'static str {
        match self {
            Medal::Gold => "Gold",
            Medal::Silver => "Silver",
            Medal::Bronze => "Bronze",
            Medal::Mention => "Mention",
        }
    }

    pub fn parse(s: &str) -> Option<Medal> {
        Medal::ALL.into_iter().find(|m| m.key().eq_ignore_ascii_case(s))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    pub fn hex(s: &str) -> Result<Color> {
        let h = s.trim().trim_start_matches('#');
        let byte = |i: usize| -> Result<f32> {
            let v = u8::from_str_radix(h.get(i..i + 2).context("short color")?, 16)?;
            Ok(v as f32 / 255.0)
        };
        match h.len() {
            6 => Ok(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Ok(Color { a: byte(6)?, ..Color::rgb(byte(0)?, byte(2)?, byte(4)?) }),
            _ => bail!("invalid color {s:?} (expected #RRGGBB)"),
        }
    }

    pub fn with_alpha(self, a: f32) -> Color {
        Color { a, ..self }
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Color, D::Error> {
        let s = String::deserialize(d)?;
        Color::hex(&s).map_err(serde::de::Error::custom)
    }
}

/// A recap page: (columns, rows, scale[, contestants])
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Group {
    pub cols: usize,
    pub rows: usize,
    pub scale: f64,
    pub size: usize,
}

impl<'de> Deserialize<'de> for Group {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Group, D::Error> {
        let v = Vec::<f64>::deserialize(d)?;
        if !(3..=4).contains(&v.len()) {
            return Err(serde::de::Error::custom(
                "a group is [columns, rows, scale] or [columns, rows, scale, contestants]",
            ));
        }
        let (cols, rows) = (v[0] as usize, v[1] as usize);
        Ok(Group { cols, rows, scale: v[2], size: v.get(3).map_or(cols * rows, |&n| n as usize) })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct PerMedal<T> {
    pub gold: T,
    pub silver: T,
    pub bronze: T,
    pub mention: T,
}

impl<T> PerMedal<T> {
    pub fn get(&self, m: Medal) -> &T {
        match m {
            Medal::Gold => &self.gold,
            Medal::Silver => &self.silver,
            Medal::Bronze => &self.bronze,
            Medal::Mention => &self.mention,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Contest {
    pub start: Vec<String>,
    pub end: Vec<String>,
    pub max_score: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Timing {
    pub medal_delay: PerMedal<f64>,
    pub timelapse_duration: f64,
    pub winner_confetti_duration: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Groups {
    pub gold: Vec<Group>,
    pub silver: Vec<Group>,
    pub bronze: Vec<Group>,
    pub mention: Vec<Group>,
    pub row_gap: f64,
}

impl Groups {
    pub fn get(&self, m: Medal) -> &[Group] {
        match m {
            Medal::Gold => &self.gold,
            Medal::Silver => &self.silver,
            Medal::Bronze => &self.bronze,
            Medal::Mention => &self.mention,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Paths {
    pub output_dir: String,
    pub screen_dir: String,
    pub screen_extensions: Vec<String>,
    pub face_dir: String,
    pub logo: String,
    pub cup: String,
    pub medal: String,
    pub no_face: String,
    pub no_screen: String,
    pub videos: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Fonts {
    pub regular: String,
    pub bold: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Colors {
    pub gold: Color,
    pub silver: Color,
    pub bronze: Color,
    pub background: Color,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Video {
    /// "auto", "off" or a device (see config.toml)
    pub hardware: String,
    pub hardware_qp: u32,
    pub encoder: String,
    pub encoder_options: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Zones {
    pub logo_left: f64,
    pub info: f64,
    pub score: f64,
    pub screen: f64,
    pub logo_right: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Wide {
    pub aspect: [u32; 2],
    pub zone_fractions: Zones,
    pub bar_height: f64,
    pub bar_color_no_medal: Color,
    pub bar_speed: f64,
    pub margin: f64,
    pub logo_scale: f64,
    pub score_scale: f64,
    pub name_scale: f64,
    pub info_scale: f64,
    pub medal_scale: f64,
    pub group_tile_width: f64,
    pub row_gap: f64,
    pub groups: PerMedal<Vec<Group>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub max_users: usize,
    pub contest: Contest,
    pub timing: Timing,
    pub groups: Groups,
    pub paths: Paths,
    pub fonts: Fonts,
    pub medal_names: PerMedal<Vec<String>>,
    pub colors: Colors,
    pub class_names: HashMap<String, String>,
    pub video: Video,
    pub wide: Wide,
}

impl Config {
    /// Reads `path` (if it exists) on top of the built-in defaults.
    pub fn load(path: Option<&Path>) -> Result<Config> {
        let mut value: toml::Table = toml::from_str(DEFAULT_CONFIG).expect("built-in config.toml");
        match path {
            Some(p) if p.exists() => {
                let text = std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
                let user: toml::Table = toml::from_str(&text).with_context(|| format!("parsing {}", p.display()))?;
                merge(&mut value, user);
            }
            Some(p) => eprintln!("note: {} not found, using the default settings", p.display()),
            None => {}
        }
        let config: Config = toml::Value::Table(value).try_into().context("invalid configuration")?;
        if config.contest.start.len() != config.contest.end.len() || config.contest.start.is_empty() {
            bail!("contest.start and contest.end must have the same (non-zero) number of days");
        }
        Ok(config)
    }

    /// The configuration of the 64:9 layout: the groups and the row gap come from [wide]
    pub fn widened(&self) -> Config {
        let mut c = self.clone();
        let g = &self.wide.groups;
        c.groups = Groups {
            gold: g.gold.clone(),
            silver: g.silver.clone(),
            bronze: g.bronze.clone(),
            mention: g.mention.clone(),
            row_gap: self.wide.row_gap,
        };
        c
    }

    /// Which medal a ranking entry belongs to, from its `medal` column
    pub fn medal_of(&self, name: &str) -> Option<Medal> {
        let name = name.trim().to_lowercase();
        Medal::ALL.into_iter().find(|&m| self.medal_names.get(m).iter().any(|n| n.to_lowercase() == name))
    }

    /// Color of a medal (the mentions have none)
    pub fn medal_color(&self, m: Medal) -> Option<Color> {
        match m {
            Medal::Gold => Some(self.colors.gold),
            Medal::Silver => Some(self.colors.silver),
            Medal::Bronze => Some(self.colors.bronze),
            Medal::Mention => None,
        }
    }

    pub fn class_name(&self, class: i64) -> String {
        self.class_names.get(&class.to_string()).cloned().unwrap_or_else(|| class.to_string())
    }
}

fn merge(base: &mut toml::Table, over: toml::Table) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let c = Config::load(None).unwrap();
        assert_eq!(c.groups.gold[0], Group { cols: 2, rows: 3, scale: 0.7, size: 5 });
        assert_eq!(c.groups.silver[0].size, 6);
        assert_eq!(c.class_name(12), "IV");
        assert_eq!(c.medal_of("Argento"), Some(Medal::Silver));
        assert_eq!(c.widened().groups.mention[0].cols, 5);
    }

    #[test]
    fn user_values_override_defaults() {
        let mut base: toml::Table = toml::from_str(DEFAULT_CONFIG).unwrap();
        merge(&mut base, toml::from_str("[timing]\ntimelapse_duration = 7").unwrap());
        let c: Config = toml::Value::Table(base).try_into().unwrap();
        assert_eq!(c.timing.timelapse_duration, 7.0);
        assert_eq!(c.timing.winner_confetti_duration, 10.0);
    }
}
