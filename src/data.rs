//! Preprocessed data (output/ranking.json, output/history.json) and screenshots.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};

use crate::config::{Config, Medal};

#[derive(Clone, Debug, Deserialize)]
pub struct User {
    /// None for unofficial contestants
    pub position: Option<u32>,
    pub username: String,
    pub name: String,
    pub school: String,
    pub city: String,
    #[serde(default)]
    pub province: String,
    pub medal: String,
    pub class: i64,
    #[serde(default)]
    pub po: bool,
}

impl User {
    /// "Classe IV, <school>"
    pub fn class_school(&self, config: &Config) -> String {
        format!("Classe {}, {}", config.class_name(self.class), self.school)
    }

    /// "<city> (<province>)"
    pub fn city_province(&self) -> String {
        if self.province.is_empty() { self.city.clone() } else { format!("{} ({})", self.city, self.province) }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScoreChange {
    pub time: f64,
    pub score: f64,
}

pub struct Data {
    /// Best first, as in ranking.csv
    pub ranking: Vec<User>,
    pub history: HashMap<String, Vec<ScoreChange>>,
}

impl Data {
    pub fn load(config: &Config) -> Result<Data> {
        let dir = Path::new(&config.paths.output_dir);
        let ranking_path = dir.join("ranking.json");
        let text = std::fs::read_to_string(&ranking_path)
            .with_context(|| format!("reading {} (did you run `oii-ranking preprocess`?)", ranking_path.display()))?;
        // files written by the old Python preprocess.py use the non-standard `Infinity`
        let text = text.replace(": Infinity", ": null");
        let ranking = serde_json::from_str(&text).with_context(|| format!("parsing {}", ranking_path.display()))?;
        let history_path = dir.join("history.json");
        let history = serde_json::from_str(
            &std::fs::read_to_string(&history_path).with_context(|| format!("reading {}", history_path.display()))?,
        )
        .with_context(|| format!("parsing {}", history_path.display()))?;
        Ok(Data { ranking, history })
    }

    /// Contestants with the given medal, from the lowest position to the best one
    pub fn medalists(&self, config: &Config, medal: Medal) -> Vec<User> {
        let mut users: Vec<User> =
            self.ranking.iter().rev().filter(|u| config.medal_of(&u.medal) == Some(medal)).cloned().collect();
        let skip = users.len().saturating_sub(config.max_users);
        users.drain(..skip);
        users
    }

    /// Score history of a user, starting with a 0 at time 0
    pub fn history_of(&self, username: &str) -> Vec<ScoreChange> {
        let mut h = vec![ScoreChange { time: 0.0, score: 0.0 }];
        match self.history.get(username) {
            Some(changes) => h.extend_from_slice(changes),
            None => eprintln!("!!! No submissions of {username}"),
        }
        h
    }

    pub fn final_score(&self, username: &str) -> f64 {
        self.history.get(username).and_then(|h| h.last()).map_or(0.0, |c| c.score)
    }
}

/// Parses a local date like 2026-09-25T09:00:00 into a unix timestamp
pub fn local_timestamp(s: &str) -> Result<f64> {
    let naive = NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%dT%H:%M:%S%.f")
        .with_context(|| format!("invalid date {s:?} (expected YYYY-MM-DDTHH:MM:SS)"))?;
    Ok(to_timestamp(naive))
}

pub fn to_timestamp(naive: NaiveDateTime) -> f64 {
    let local = Local.from_local_datetime(&naive).earliest().unwrap_or_else(|| naive.and_utc().with_timezone(&Local));
    local.timestamp() as f64 + local.timestamp_subsec_micros() as f64 / 1e6
}

/// The contest days, as (start, end) unix timestamps
#[derive(Clone, Debug)]
pub struct Contest {
    pub days: Vec<(f64, f64)>,
}

impl Contest {
    pub fn new(config: &Config) -> Result<Contest> {
        let days = config
            .contest
            .start
            .iter()
            .zip(&config.contest.end)
            .map(|(s, e)| Ok((local_timestamp(s)?, local_timestamp(e)?)))
            .collect::<Result<_>>()?;
        Ok(Contest { days })
    }

    /// Contest time shown after `fraction` (0..1) of the timelapse: the days are shown one
    /// after the other, each taking the same share of the timelapse.
    pub fn time_at(&self, fraction: f64) -> f64 {
        let n = self.days.len();
        let scaled = fraction * n as f64;
        let i = (scaled.max(0.0) as usize).min(n - 1);
        let (s, e) = self.days[i];
        s + (e - s) * (scaled - i as f64)
    }

    pub fn contains(&self, t: f64) -> bool {
        self.days.iter().any(|&(s, e)| s <= t && t <= e)
    }
}

/// Screenshots of a user taken during the contest, sorted by time
pub fn screenshots(config: &Config, contest: &Contest, username: &str) -> Vec<(f64, PathBuf)> {
    let dir = Path::new(&config.paths.screen_dir).join(username);
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![] };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| {
            let ext = format!(".{}", p.extension()?.to_str()?.to_lowercase());
            if !config.paths.screen_extensions.iter().any(|e| e.to_lowercase() == ext) {
                return None;
            }
            let naive = NaiveDateTime::parse_from_str(p.file_stem()?.to_str()?, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
            let t = to_timestamp(naive);
            contest.contains(t).then_some((t, p))
        })
        .collect()
}

/// Index of the last entry at or before `t` (0 if none), for lists sorted by time
pub fn index_at<T>(items: &[T], time: impl Fn(&T) -> f64, t: f64) -> usize {
    items.partition_point(|x| time(x) <= t).saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timelapse_covers_every_day() {
        let c = Contest { days: vec![(100.0, 200.0), (1000.0, 1100.0)] };
        assert_eq!(c.time_at(0.0), 100.0);
        assert_eq!(c.time_at(0.25), 150.0);
        assert_eq!(c.time_at(0.5), 1000.0);
        assert_eq!(c.time_at(1.0), 1100.0);
    }

    #[test]
    fn index_at_finds_last_before() {
        let v = [0.0, 10.0, 20.0];
        assert_eq!(index_at(&v, |x| *x, -5.0), 0);
        assert_eq!(index_at(&v, |x| *x, 10.0), 1);
        assert_eq!(index_at(&v, |x| *x, 99.0), 2);
    }
}
