//! Port of preprocess.py: turns the contest data (cms, terry or quizms) into
//! output/history.json, output/final_scores.json and output/ranking.json, and crops the faces.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::NaiveDateTime;
use rayon::prelude::*;
use serde_json::{Map, Value, json};

use crate::config::Config;
use crate::data::to_timestamp;

pub struct Options {
    pub faces_dir: PathBuf,
    pub ranking_dir: PathBuf,
    pub ranking_csv: PathBuf,
    pub output_dir: PathBuf,
    pub terry: bool,
}

struct Submission {
    user: String,
    task: String,
}

struct SubChange {
    submission: String,
    time: f64,
    extra: Vec<f64>,
}

fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    v.sort();
    Ok(v)
}

fn number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// cms: ranking/submissions/<id>.json and ranking/subchanges/*.json
fn cms_data(dir: &Path) -> Result<(HashMap<String, Submission>, Vec<SubChange>)> {
    let mut subs = HashMap::new();
    for path in json_files(&dir.join("submissions"))? {
        let id = path.file_name().unwrap().to_string_lossy().trim_end_matches(".json").to_string();
        let v = read_json(&path)?;
        subs.insert(id, Submission { user: str_of(&v["user"]), task: str_of(&v["task"]) });
    }
    let mut changes = vec![];
    for path in json_files(&dir.join("subchanges"))? {
        let v = read_json(&path)?;
        changes.push(SubChange {
            submission: str_of(&v["submission"]),
            time: number(&v["time"]),
            extra: v["extra"].as_array().map(|a| a.iter().map(number).collect()).unwrap_or_default(),
        });
    }
    Ok((subs, changes))
}

fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn local_ts(s: &str, fmt: &str) -> Result<f64> {
    Ok(to_timestamp(NaiveDateTime::parse_from_str(s.trim(), fmt).with_context(|| format!("invalid date {s:?}"))?))
}

/// terry: ranking/<task>/<user>/<submission>/info.txt, and quizms: ranking/submissions.json
fn terry_data(dir: &Path, ranking_csv: &Path) -> Result<(HashMap<String, Submission>, Vec<SubChange>)> {
    let mut subs = HashMap::new();
    let mut changes = vec![];
    for task_dir in json_files(dir)?.into_iter().filter(|p| p.is_dir()) {
        for user_dir in json_files(&task_dir)?.into_iter().filter(|p| p.is_dir()) {
            for sub_dir in json_files(&user_dir)?.into_iter().filter(|p| p.is_dir()) {
                let info = sub_dir.join("info.txt");
                if !info.exists() {
                    continue;
                }
                let text = std::fs::read_to_string(&info)?;
                let mut lines = text.lines();
                let (Some(date), Some(score)) = (lines.next(), lines.next()) else {
                    bail!("{}: expected a date and a score", info.display())
                };
                let time = local_ts(date.trim_start_matches("date:"), "%Y-%m-%d %H:%M:%S")?.floor();
                let score = score.trim_start_matches("score:").trim().parse::<f64>()?.trunc();
                let id = info.to_string_lossy().to_string();
                let name = |p: &Path| p.file_name().unwrap().to_string_lossy().to_string();
                subs.insert(id.clone(), Submission { user: name(&user_dir), task: name(&task_dir) });
                changes.push(SubChange { submission: id, time, extra: vec![score] });
            }
        }
    }
    let qms = dir.join("submissions.json");
    if qms.exists() {
        // contestants are matched by uid, or by "Name Surname" / "Surname Name"
        let mut usernames: HashMap<String, String> = HashMap::new();
        for row in csv::Reader::from_path(ranking_csv)?.deserialize::<HashMap<String, String>>() {
            let row = row?;
            usernames.insert(row["name"].clone(), row["username"].clone());
        }
        let title = |s: &str| {
            s.split(' ')
                .map(|w| {
                    let mut c = w.chars();
                    c.next().map_or(String::new(), |f| f.to_uppercase().chain(c.flat_map(char::to_lowercase)).collect())
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        let data = read_json(&qms)?;
        for d in data.as_object().context("submissions.json: expected an object")?.values() {
            let uid = str_of(&d["uid"]);
            let user = if usernames.values().any(|u| *u == uid) {
                uid.clone()
            } else {
                let (name, surname) = (title(&str_of(&d["name"])), title(&str_of(&d["surname"])));
                match usernames
                    .get(&format!("{name} {surname}"))
                    .or_else(|| usernames.get(&format!("{surname} {name}")))
                {
                    Some(u) => u.clone(),
                    None => continue,
                }
            };
            for (i, s) in d["submissions"].as_array().into_iter().flatten().enumerate() {
                let id = format!("{uid}-{i}");
                let ts = str_of(&s["timestamp"]);
                let ts = &ts[..ts.len().saturating_sub(5)];
                let time = local_ts(ts, "%Y-%m-%dT%H:%M:%S")? + 7200.0;
                let score = number(&s["score"]);
                subs.insert(id.clone(), Submission { user: user.clone(), task: "quizms".into() });
                changes.push(SubChange { submission: id, time, extra: vec![score] });
            }
        }
    }
    Ok((subs, changes))
}

/// Integral floats are written as integers (as Python wrote the times)
fn num(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 { json!(x as i64) } else { json!(x) }
}

pub fn run(config: &Config, o: &Options) -> Result<()> {
    if !o.ranking_dir.exists() {
        bail!("missing ranking directory {}", o.ranking_dir.display());
    }
    std::fs::create_dir_all(&o.output_dir)?;
    let (subs, mut changes) =
        if o.terry { terry_data(&o.ranking_dir, &o.ranking_csv)? } else { cms_data(&o.ranking_dir)? };
    changes.sort_by(|a, b| a.time.total_cmp(&b.time));
    println!("{} submissions", subs.len());
    println!("{} subchanges", changes.len());

    // best score of every subtask, and the history of the total
    let mut task_scores: Map<String, Value> = Map::new();
    let mut scores: HashMap<String, Vec<(String, Vec<f64>)>> = HashMap::new();
    let mut history: Map<String, Value> = Map::new();
    let mut last: HashMap<String, f64> = HashMap::new();
    for ch in &changes {
        let Some(sub) = subs.get(&ch.submission) else { bail!("unknown submission {}", ch.submission) };
        let user = scores.entry(sub.user.clone()).or_default();
        match user.iter_mut().find(|(t, _)| *t == sub.task) {
            Some((_, best)) => {
                for (i, s) in ch.extra.iter().enumerate() {
                    if i < best.len() {
                        best[i] = best[i].max(*s);
                    }
                }
            }
            None => user.push((sub.task.clone(), ch.extra.clone())),
        }
        let total: f64 = user.iter().map(|(_, v)| v.iter().sum::<f64>()).sum();
        // every user who submitted has a history, even if it stays empty
        let h = history.entry(sub.user.clone()).or_insert_with(|| json!([]));
        if total != *last.get(&sub.user).unwrap_or(&0.0) {
            last.insert(sub.user.clone(), total);
            h.as_array_mut().unwrap().push(json!({"time": num(ch.time), "score": total}));
        }
    }
    println!("{} users have submitted", scores.len());
    let mut users: Vec<_> = scores.into_iter().collect();
    users.sort_by(|a, b| a.0.cmp(&b.0));
    for (user, tasks) in users {
        let t: Map<String, Value> = tasks.into_iter().map(|(k, v)| (k, json!(v))).collect();
        task_scores.insert(user, Value::Object(t));
    }
    write_json(&o.output_dir.join("final_scores.json"), &Value::Object(task_scores))?;
    write_json(&o.output_dir.join("history.json"), &Value::Object(history))?;

    // ranking.csv -> ranking.json
    let mut ranking = vec![];
    let mut reader =
        csv::Reader::from_path(&o.ranking_csv).with_context(|| format!("reading {}", o.ranking_csv.display()))?;
    let headers = reader.headers()?.clone();
    for row in reader.records() {
        let row = row?;
        let mut m = Map::new();
        for (k, v) in headers.iter().zip(row.iter()) {
            let value = match k {
                "po" => json!(!v.is_empty() && v != "F"),
                "position" => v.trim().parse::<u32>().map_or(Value::Null, |p| json!(p)),
                "class" => json!(v.trim().parse::<i64>().with_context(|| format!("invalid class {v:?}"))?),
                _ => json!(v),
            };
            m.insert(k.to_string(), value);
        }
        m.entry("po").or_insert(json!(false));
        ranking.push(Value::Object(m));
    }
    write_json(&o.output_dir.join("ranking.json"), &Value::Array(ranking.clone()))?;

    let screens = Path::new(&config.paths.screen_dir);
    if screens.join("background.png").exists() && screens.join("filler.png").exists() {
        let medalists: Vec<String> = ranking
            .iter()
            .filter(|u| !u["medal"].as_str().unwrap_or("").is_empty())
            .map(|u| str_of(&u["username"]))
            .collect();
        fake_screenshots(config, screens, &medalists)?;
    }

    println!("Resizing faces...");
    let faces_out = o.output_dir.join("faces");
    std::fs::create_dir_all(&faces_out)?;
    let faces: Vec<PathBuf> = json_files(&o.faces_dir)?.into_iter().filter(|p| p.is_file()).collect();
    faces.par_iter().try_for_each(|path| -> Result<()> {
        let stem = path.file_stem().unwrap().to_string_lossy();
        let target = faces_out.join(format!("{stem}.jpg"));
        if target.exists() {
            return Ok(());
        }
        crop_face(path, &target).with_context(|| format!("resizing {}", path.display()))
    })?;
    Ok(())
}

/// A 1000x1000 square from the center of the photo (ImageMagick's -resize 1000x1000^ -extent)
fn crop_face(path: &Path, target: &Path) -> Result<()> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(path)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = image::DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    let face = img.resize_to_fill(1000, 1000, image::imageops::FilterType::Lanczos3).to_rgb8();
    let file = std::io::BufWriter::new(std::fs::File::create(target)?);
    image::codecs::jpeg::JpegEncoder::new_with_quality(file, 92).encode_image(&face)?;
    Ok(())
}

fn write_json(path: &Path, v: &Value) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(v)? + "\n").with_context(|| format!("writing {}", path.display()))
}

/// Placeholder screenshots: a clock for every minute of the contest (screenshots/background.png
/// with screenshots/filler.png inside the dial), linked into the folder of every medalist.
/// Skipped if they were already generated.
fn fake_screenshots(config: &Config, dir: &Path, users: &[String]) -> Result<()> {
    let existing = |d: &Path| -> Vec<String> {
        std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("20") && n.ends_with("00.0.png"))
            .collect()
    };
    let mut frames = existing(dir);
    if frames.is_empty() {
        println!("Generating placeholder screenshots...");
        let mut minutes = vec![];
        for (s, e) in config.contest.start.iter().zip(&config.contest.end) {
            let fmt = "%Y-%m-%dT%H:%M:%S";
            let start = NaiveDateTime::parse_from_str(s, fmt)?;
            let end = NaiveDateTime::parse_from_str(e, fmt)?;
            let mut t = start
                .date()
                .and_hms_opt(start.format("%H").to_string().parse()?, start.format("%M").to_string().parse()?, 0)
                .unwrap();
            while t <= end {
                minutes.push(t);
                t += chrono::Duration::minutes(1);
            }
        }
        let bg = std::fs::canonicalize(dir.join("background.png"))?;
        let filler = std::fs::canonicalize(dir.join("filler.png"))?;
        minutes.par_iter().try_for_each(|t| -> Result<()> {
            let (h, m): (u32, u32) = (t.format("%H").to_string().parse()?, t.format("%M").to_string().parse()?);
            let svg = clock_svg(&bg, &filler, h, m);
            let opts = resvg::usvg::Options { resources_dir: Some(dir.to_path_buf()), ..Default::default() };
            let tree = resvg::usvg::Tree::from_str(&svg, &opts)?;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(960, 540).unwrap();
            resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
            crate::render::save_png(pixmap, &dir.join(format!("{}.0.png", t.format("%Y-%m-%dT%H:%M:%S"))))
        })?;
        frames = existing(dir);
    }
    for user in users {
        let d = dir.join(user);
        std::fs::create_dir_all(&d)?;
        for f in &frames {
            let link = d.join(f);
            if !link.exists() {
                #[cfg(unix)]
                std::os::unix::fs::symlink(Path::new("..").join(f), &link)?;
                #[cfg(not(unix))]
                std::fs::copy(dir.join(f), &link)?;
            }
        }
    }
    Ok(())
}

/// The clock of images/clock.asy
fn clock_svg(bg: &Path, filler: &Path, h: u32, m: u32) -> String {
    let mut s = String::new();
    s.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="960" height="540" viewBox="-480 -270 960 540">"#);
    s.push_str(&format!(
        r#"<defs><clipPath id="dial"><circle r="230"/></clipPath></defs>
<image x="-480" y="-270" width="960" height="540" preserveAspectRatio="xMidYMid slice" xlink:href="{}"/>
<image x="-480" y="-270" width="960" height="540" preserveAspectRatio="xMidYMid slice" clip-path="url(#dial)" xlink:href="{}"/>
<circle r="230" fill="none" stroke="black" stroke-width="24"/>"#,
        bg.display(),
        filler.display()
    ));
    for i in 0..60 {
        let a = (6 * i) as f64 * std::f64::consts::PI / 180.0;
        s.push_str(&format!(r#"<circle cx="{:.2}" cy="{:.2}" r="4"/>"#, 200.0 * a.cos(), 200.0 * a.sin()));
    }
    for i in 0..12 {
        let a = (30 * i) as f64 * std::f64::consts::PI / 180.0;
        let l = if i % 3 == 0 { 32.0 } else { 16.0 };
        let (c, sn) = (a.cos(), a.sin());
        s.push_str(&format!(
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="black" stroke-width="12"/>"#,
            (200.0 - l) * c,
            (200.0 - l) * sn,
            200.0 * c,
            200.0 * sn
        ));
    }
    // hands (y points down in svg: rotate clockwise)
    let hand = |angle: f64, len: f64| {
        format!(
            r#"<path transform="rotate({angle}) scale(2)" d="M0 0 L-5 {:.1} L0 {:.1} L5 {:.1} Z"/>"#,
            -len * 0.5 - (len - 60.0) * 0.5,
            -len,
            -len * 0.5 - (len - 60.0) * 0.5
        )
    };
    s.push_str(&hand(30.0 * (h % 12) as f64, 60.0));
    s.push_str(&hand(6.0 * m as f64, 90.0));
    s.push_str(r#"<circle r="6"/><circle r="3" fill="white"/></svg>"#);
    s
}
