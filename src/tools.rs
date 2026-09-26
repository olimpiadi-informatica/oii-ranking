//! Helpers: placeholder data for a smoke test (make_test_data.py) and the import of the
//! raw export in ./orig (import_orig.py).

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local};
use resvg::tiny_skia;
use serde_json::json;

use crate::config::{Color, Config};
use crate::data::local_timestamp;
use crate::scenes::Rng;
use crate::text::Font;

/// Draws `text` on a pixmap, top-left at (x, y) pixels
fn label(pixmap: &mut tiny_skia::Pixmap, font: &Font, text: &str, x: f64, y: f64, size: f64) {
    let shape = font.shape(text, size, Color::WHITE);
    let tf = kurbo::Affine::translate((x + shape.w / 2.0, y + shape.h / 2.0)) * kurbo::Affine::FLIP_Y;
    crate::vector::draw_shape(&mut pixmap.as_mut(), &shape, tf, 1.0, 1.0, crate::vector::Reveal::Full);
}

pub fn make_test_data(config: &Config) -> Result<()> {
    for t in ["ranking.csv", "ranking", "faces", "screenshots"] {
        if Path::new(t).exists() {
            bail!("{t} already exists, refusing to overwrite it");
        }
    }
    let mut rng = Rng::new("test data");
    let font = Font::regular();
    let medals = [("oro", 3), ("argento", 6), ("bronzo", 2), ("menzione", 4)];
    let provinces = ["MI", "RM", "TO", "NA", "PI", "BO"];
    let days: Vec<(f64, f64)> = config
        .contest
        .start
        .iter()
        .zip(&config.contest.end)
        .map(|(s, e)| Ok((local_timestamp(s)?, local_timestamp(e)?)))
        .collect::<Result<_>>()?;

    // users
    struct U {
        position: String,
        username: String,
        medal: &'static str,
        fin: f64,
    }
    let mut users = vec![];
    let mut csv = csv::Writer::from_path("ranking.csv")?;
    csv.write_record(["position", "username", "name", "school", "city", "province", "medal", "class", "po"])?;
    for (medal, n) in medals {
        for _ in 0..n {
            let i = users.len() + 1;
            users.push(U {
                position: i.to_string(),
                username: format!("user{i:02}"),
                medal,
                fin: (config.contest.max_score - 25.0 * i as f64).max(0.0),
            });
        }
    }
    // one unofficial contestant (empty position) with a medal
    users.last_mut().unwrap().position.clear();
    for (k, u) in users.iter().enumerate() {
        let i = k + 1;
        csv.write_record([
            u.position.as_str(),
            &u.username,
            &format!("Nome{i:02} Cognome{i:02}"),
            &format!("Liceo Placeholder {i}"),
            &format!("Citta{i:02}"),
            rng.pick(&provinces),
            u.medal,
            &(1 + (rng.next() * 5.0) as u32).to_string(),
            if i == 1 || i == 5 { "T" } else { "F" },
        ])?;
    }
    csv.flush()?;

    // cms data: the score grows in 4 steps spread over the contest days
    std::fs::create_dir_all("ranking/submissions")?;
    std::fs::create_dir_all("ranking/subchanges")?;
    let mut sid = 0;
    for u in &users {
        let mut steps: Vec<u32> = vec![];
        while steps.len() < 4 {
            let s = 1 + (rng.next() * 99.0) as u32;
            if !steps.contains(&s) {
                steps.push(s);
            }
        }
        steps.sort();
        for (k, s) in steps.iter().enumerate() {
            let (start, end) = days[k * days.len() / steps.len()];
            let t = (start + (end - start) * *s as f64 / 100.0).floor();
            let score = u.fin * (k + 1) as f64 / steps.len() as f64;
            let sub = format!("sub{sid:04}");
            std::fs::write(
                format!("ranking/submissions/{sub}.json"),
                json!({"user": u.username, "task": "task1", "time": t}).to_string(),
            )?;
            std::fs::write(
                format!("ranking/subchanges/{sub}.json"),
                json!({"submission": sub, "time": t, "score": score, "extra": [score]}).to_string(),
            )?;
            sid += 1;
        }
    }

    // faces
    std::fs::create_dir_all("faces")?;
    for u in &users {
        let mut p = tiny_skia::Pixmap::new(400, 300).unwrap();
        let mut c = || (60.0 + rng.next() * 160.0) as u8;
        let (r, g, b) = (c(), c(), c());
        p.fill(tiny_skia::Color::from_rgba8(r, g, b, 255));
        let head =
            tiny_skia::PathBuilder::from_oval(tiny_skia::Rect::from_xywh(130.0, 60.0, 140.0, 140.0).unwrap()).unwrap();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(240, 220, 200, 255);
        p.fill_path(&head, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
        label(&mut p, &font, &u.username, 10.0, 10.0, 20.0);
        let img = image::RgbaImage::from_raw(400, 300, p.take()).unwrap();
        image::DynamicImage::ImageRgba8(img).to_rgb8().save(format!("faces/{}.jpg", u.username))?;
    }

    // screenshots: a few shared frames, linked for every user
    std::fs::create_dir_all("screenshots")?;
    let mut frames = vec![];
    for &(s, e) in &days {
        for frac in [0.0, 0.5, 1.0] {
            let when = DateTime::from_timestamp((s + (e - s) * frac) as i64, 0).unwrap().with_timezone(&Local);
            let name = format!("{}.0.png", when.format("%Y-%m-%dT%H:%M:%S"));
            let mut p = tiny_skia::Pixmap::new(480, 270).unwrap();
            p.fill(tiny_skia::Color::from_rgba8(30, 40, 60, 255));
            label(&mut p, &font, &when.format("%Y-%m-%dT%H:%M:%S").to_string(), 20.0, 120.0, 24.0);
            crate::render::save_png(p, &Path::new("screenshots").join(&name))?;
            frames.push(name);
        }
    }
    for u in &users {
        let d = Path::new("screenshots").join(&u.username);
        std::fs::create_dir_all(&d)?;
        for f in &frames {
            #[cfg(unix)]
            std::os::unix::fs::symlink(Path::new("..").join(f), d.join(f))?;
            #[cfg(not(unix))]
            std::fs::copy(Path::new("screenshots").join(f), d.join(f))?;
        }
    }
    println!("Created placeholder data for {} users. Now run `oii-ranking preprocess`", users.len());
    Ok(())
}

/// orig/temp.csv -> ranking.csv (sorted by position), orig/faces.zip -> faces/<username>.jpg
pub fn import_orig() -> Result<()> {
    let orig = Path::new("orig");
    let mut reader = csv::Reader::from_path(orig.join("temp.csv")).context("reading orig/temp.csv")?;
    let mut rows = vec![];
    for r in reader.deserialize::<std::collections::HashMap<String, String>>() {
        let r = r?;
        let get = |k: &str| r.get(k).cloned().with_context(|| format!("orig/temp.csv: missing column {k}"));
        rows.push([
            get("Posizione")?,
            get("username")?,
            format!("{} {}", get("Nome")?, get("Cognome")?),
            format!("{} {}", get("Tipo_scuola")?, get("Nome_scuola")?),
            get("Città_scuola")?,
            get("Sigla_provincia_scuola")?,
            get("Medaglia")?,
            get("Classe")?,
            if get("PO")?.is_empty() { "F".into() } else { "T".into() },
        ]);
    }
    rows.sort_by_key(|r| r[0].trim().parse::<u64>().unwrap_or(u64::MAX));
    let mut w = csv::Writer::from_path("ranking.csv")?;
    w.write_record(["position", "username", "name", "school", "city", "province", "medal", "class", "po"])?;
    for r in &rows {
        w.write_record(r)?;
    }
    w.flush()?;
    println!("ranking.csv: {} contestants", rows.len());

    std::fs::create_dir_all("faces")?;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(orig.join("faces.zip")).context("opening orig/faces.zip")?)?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let name = Path::new(f.name()).file_name().map(|n| n.to_owned());
        let Some(name) = name else { continue };
        let mut data = vec![];
        f.read_to_end(&mut data)?;
        std::fs::write(Path::new("faces").join(name), data)?;
    }
    let have: std::collections::HashSet<String> = std::fs::read_dir("faces")?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().to_string()))
        .collect();
    let missing: Vec<&String> = rows.iter().map(|r| &r[1]).filter(|u| !have.contains(*u)).collect();
    println!("faces: {} extracted, {} contestants without a face {:?}", have.len(), missing.len(), missing);
    Ok(())
}
