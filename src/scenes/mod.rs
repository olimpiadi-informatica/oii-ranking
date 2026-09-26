//! The videos. `standard` is the 16:9 layout (the old ranking.py), `wide` the 64:9 one
//! (ranking_wide.py); this module has what they share.

pub mod overlays;
pub mod standard;
pub mod wide;

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::config::{Color, Config, Medal};
use crate::data::{self, Contest, Data, User};
use crate::images::{self, Images, Img};
use crate::text::Fonts;
use crate::timeline::{Anim, DOWN, Id, Mob, RIGHT, Scene, smooth};
use crate::vector::Shape;

static EMBEDDED_CUP: &str = include_str!("../../images/cup.svg");
static EMBEDDED_MEDAL: &[u8] = include_bytes!("../../images/medal.png");
static EMBEDDED_LOGO: &[u8] = include_bytes!("../../images/logo.png");

/// Everything a scene needs
pub struct Ctx<'a> {
    pub config: &'a Config,
    pub data: &'a Data,
    pub fonts: &'a Fonts,
    pub images: &'a Images,
    pub contest: Contest,
    pub medal: Medal,
    /// Contestants of the medal, from the lowest position to the best one
    pub users: Vec<User>,
    /// No recap pages (when rendering only some contestants)
    pub no_groups: bool,
    no_face: std::sync::OnceLock<Arc<Img>>,
    no_screen: std::sync::OnceLock<Arc<Img>>,
}

impl<'a> Ctx<'a> {
    pub fn new(
        config: &'a Config,
        data: &'a Data,
        fonts: &'a Fonts,
        images: &'a Images,
        medal: Medal,
        only: &[String],
    ) -> Result<Ctx<'a>> {
        let mut users = data.medalists(config, medal);
        if !only.is_empty() {
            users.retain(|u| only.contains(&u.username));
            if users.is_empty() {
                anyhow::bail!("none of {} has the {} medal", only.join(", "), medal.key());
            }
        }
        Ok(Ctx {
            config,
            data,
            fonts,
            images,
            contest: Contest::new(config)?,
            medal,
            users,
            no_groups: !only.is_empty(),
            no_face: Default::default(),
            no_screen: Default::default(),
        })
    }

    pub fn medal_color(&self) -> Option<Color> {
        self.config.medal_color(self.medal)
    }

    pub fn delay(&self) -> f64 {
        *self.config.timing.medal_delay.get(self.medal)
    }

    pub fn groups(&self) -> Vec<crate::config::Group> {
        if self.no_groups { vec![] } else { self.config.groups.get(self.medal).to_vec() }
    }

    fn placeholder(
        &self,
        cell: &std::sync::OnceLock<Arc<Img>>,
        path: &str,
        svg: fn() -> String,
        w: u32,
        h: u32,
    ) -> Arc<Img> {
        cell.get_or_init(|| self.images.file_or(Path::new(path), None, || self.images.svg(svg(), w, h))).clone()
    }

    pub fn face(&self, user: &User) -> Mob {
        let path = Path::new(&self.config.paths.face_dir).join(format!("{}.jpg", user.username));
        let img = if path.exists() {
            self.images.file_or(&path, None, || self.no_face())
        } else {
            eprintln!("!!! Face of {} at {} not found", user.username, path.display());
            self.no_face()
        };
        Mob::image(img)
    }

    fn no_face(&self) -> Arc<Img> {
        self.placeholder(&self.no_face, &self.config.paths.no_face, images::no_face_svg, 1000, 1000)
    }

    pub fn no_screen(&self) -> Arc<Img> {
        self.placeholder(&self.no_screen, &self.config.paths.no_screen, images::no_screen_svg, 960, 540)
    }

    pub fn logo(&self) -> Mob {
        let path = Path::new(&self.config.paths.logo);
        Mob::image(self.images.file_or(path, None, || self.images.bytes("logo", EMBEDDED_LOGO)))
    }

    /// The medal picture in the color of the medal
    pub fn medal_img(&self) -> Mob {
        let color = self.medal_color();
        let path = Path::new(&self.config.paths.medal);
        Mob::image(self.images.file_or(path, color, || self.images.bytes_tinted("medal", EMBEDDED_MEDAL, color)))
    }

    pub fn screenshots(&self, user: &User) -> Vec<(f64, std::path::PathBuf)> {
        let s = data::screenshots(self.config, &self.contest, &user.username);
        if s.is_empty() {
            eprintln!(
                "!!! Screenshots of {} at {} not found",
                user.username,
                Path::new(&self.config.paths.screen_dir).join(&user.username).display()
            );
        }
        s
    }

    pub fn score_text(&self, points: f64) -> String {
        format!("{} / {}", points as i64, self.config.contest.max_score)
    }
}

/// Where a recap card goes (manim's student_badge)
pub struct BadgeLayout {
    /// Left edge of the face, and its top (or its center if `center_y`)
    pub x: f64,
    pub y: f64,
    pub center_y: bool,
    pub scale: f64,
    /// Width of the grid cell
    pub cell_w: f64,
    /// How much less room the name gets for the medal and for the PO circle
    pub medal_room: f64,
    pub po_room: f64,
}

/// Creates the card objects; returns the animations that show it and those that hide it
pub fn badge(scene: &mut Scene, ctx: &Ctx, user: &User, l: BadgeLayout) -> (Vec<Anim>, Vec<Anim>) {
    let fonts = ctx.fonts;
    let scale = l.scale;
    let face = ctx.face(user).set_height(1.5 * scale).set_left(l.x);
    let face = if l.center_y { face.set_y(l.y) } else { face.set_top(l.y) };
    let text_w = l.cell_w - face.width() - 0.2 - 0.3;

    let mut name_w = text_w;
    if ctx.medal_color().is_some() {
        name_w -= l.medal_room;
    }
    if user.po {
        name_w -= l.po_room;
    }
    let name = Mob::shape(fonts.tex(&user.name, 1.1 * scale)).fit_width(name_w);
    let sub = Mob::shape(fonts.tex(&user.class_school(ctx.config), 0.5 * scale)).fit_width(text_w);
    let subsub = Mob::shape(fonts.tex(&user.city_province(), 0.5 * scale)).fit_width(text_w);
    let lines = crate::timeline::stack_left(vec![name, sub, subsub], 0.12 * scale, face.right() + 0.2, face.y);

    let name_mob = lines[0].clone();
    let ids: Vec<Id> = lines.into_iter().map(|m| scene.obj(m)).collect();
    let face_id = scene.obj(face);
    let rt = scene.run_time(&Anim::write(ids[0]));
    let mut play = vec![
        Anim::write(ids[0]),
        Anim::write(ids[1]).run_time(rt),
        Anim::write(ids[2]).run_time(rt),
        Anim::fade_in(face_id).shift(RIGHT),
    ];
    let mut fade: Vec<Anim> = ids.iter().chain([&face_id]).map(|&id| Anim::fade_out(id)).collect();

    let mut last = name_mob;
    if ctx.medal_color().is_some() {
        let medal = ctx.medal_img().scale(0.25 * scale).next_to(&last, RIGHT, crate::timeline::BUFF);
        let medal_id = scene.obj(medal.clone());
        play.push(Anim::fade_in(medal_id).scale(2.0));
        fade.push(Anim::fade_out(medal_id));
        if let Some(p) = user.position {
            let pos = Mob::shape(fonts.tex_bold(&p.to_string(), 0.7 * scale, Color::BLACK)).move_to(medal.x, medal.y);
            let pos_id = scene.obj(pos);
            play.push(Anim::fade_in(pos_id).scale(2.0));
            fade.push(Anim::fade_out(pos_id));
        }
        last = medal;
    }
    if user.po {
        let (circle, text) = po_mark(fonts, &last, 0.4 * scale, 0.8 * scale);
        let (c, t) = (scene.obj(circle), scene.obj(text));
        play.extend([Anim::create(c), Anim::write(t)]);
        fade.extend([Anim::fade_out(c), Anim::fade_out(t)]);
    }
    (play, fade)
}

/// The PO circle right of `next_to`, and its label
pub fn po_mark(fonts: &Fonts, next_to: &Mob, radius: f64, text_scale: f64) -> (Mob, Mob) {
    let circle =
        Mob::shape(Arc::new(Shape::circle(radius, Color::WHITE, 0.04))).next_to(next_to, RIGHT, crate::timeline::BUFF);
    let text = Mob::shape(fonts.tex("PO", text_scale)).move_to(circle.x, circle.y);
    (circle, text)
}

/// The cup with the position, before the top 3
pub fn cup(scene: &mut Scene, ctx: &Ctx, position: u32) {
    let gold = ctx.config.colors.gold;
    let path = Path::new(&ctx.config.paths.cup);
    let shape = Shape::svg(path, 4.0, gold).or_else(|e| {
        if path.exists() {
            eprintln!("!!! {e:#}");
        }
        Shape::svg_str(EMBEDDED_CUP, 4.0, gold)
    });
    let cup = scene.obj(Mob::shape(Arc::new(shape.expect("built-in cup"))));
    let pos =
        Mob::shape(ctx.fonts.tex_colored(&position.to_string(), 3.0, ctx.config.colors.background)).shift(0.0, 1.3);
    let pos = scene.obj(pos);
    scene.play(vec![Anim::write(cup).run_time(2.0), Anim::write(pos).run_time(0.001)]);
    scene.wait(2.0);
    scene.play(vec![Anim::fade_out(cup).shift(DOWN), Anim::fade_out(pos).shift(DOWN)]);
}

/// The timelapse lasts `frames` frames from `start`. Like the manim updaters, frame k shows
/// the contest as it was at timelapse time (k - 1) / fps; after the end, the last frame stays.
#[derive(Clone, Copy)]
pub struct Timelapse {
    pub start: f64,
    pub frames: usize,
    pub fps: f64,
    pub duration: f64,
}

impl Timelapse {
    pub fn new(scene: &Scene, duration: f64) -> Timelapse {
        let frames = ((duration * scene.fps - 1e-6).ceil() as usize).max(1);
        Timelapse { start: scene.now(), frames, fps: scene.fps, duration }
    }

    pub fn frame(&self, t: f64) -> usize {
        (((t - self.start) * self.fps).round().max(0.0) as usize).min(self.frames - 1)
    }

    /// Seconds of timelapse shown at frame k
    pub fn elapsed(&self, k: usize) -> f64 {
        k.saturating_sub(1) as f64 / self.fps
    }

    /// Contest time shown at frame k
    pub fn contest_time(&self, contest: &Contest, k: usize) -> f64 {
        contest.time_at(self.elapsed(k) / self.duration)
    }

    /// For every frame, the index of the last item (sorted by time) before the time shown
    pub fn indices(&self, contest: &Contest, times: &[f64]) -> Vec<usize> {
        (0..self.frames).map(|k| data::index_at(times, |t| *t, self.contest_time(contest, k))).collect()
    }
}

/// The screenshots shown during a timelapse: one mob per frame (shared between frames)
pub fn screen_frames(
    ctx: &Ctx,
    tl: &Timelapse,
    screenshots: &[(f64, std::path::PathBuf)],
    place: impl Fn(Mob) -> Mob,
) -> (Vec<Arc<Mob>>, Arc<Mob>) {
    if screenshots.is_empty() {
        let m = Arc::new(place(Mob::image(ctx.no_screen())));
        return (vec![m.clone(); tl.frames], m);
    }
    let times: Vec<f64> = screenshots.iter().map(|s| s.0).collect();
    let idx = tl.indices(&ctx.contest, &times);
    let mut by_index: std::collections::HashMap<usize, Arc<Mob>> = Default::default();
    let mut get = |i: usize| {
        by_index
            .entry(i)
            .or_insert_with(|| {
                Arc::new(place(Mob::image(ctx.images.file_or(&screenshots[i].1, None, || ctx.no_screen()))))
            })
            .clone()
    };
    let frames = idx.iter().map(|&i| get(i)).collect();
    (frames, get(screenshots.len() - 1))
}

/// Small deterministic random generator (SplitMix64)
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: &str) -> Rng {
        Rng(seed.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3)))
    }
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[((self.next() * v.len() as f64) as usize).min(v.len() - 1)]
    }
}

/// manim's RED, YELLOW, GREEN, BLUE, PURPLE, RED
const CONFETTI_COLORS: [Color; 6] = [
    Color::rgb(0.988, 0.384, 0.333),
    Color::rgb(1.0, 1.0, 0.0),
    Color::rgb(0.514, 0.757, 0.404),
    Color::rgb(0.345, 0.769, 0.867),
    Color::rgb(0.604, 0.447, 0.675),
    Color::rgb(0.988, 0.384, 0.333),
];
const CONFETTI_SIZE: f64 = 0.2;
const CONFETTI_OPACITY: f64 = 0.75;

/// The burst of confetti around the medal: animations for the next play (1 second)
pub fn confetti_boom(scene: &mut Scene, rng: &mut Rng, x: f64, y: f64, n: usize) -> Vec<Anim> {
    let t0 = scene.now();
    let fps = scene.fps;
    (0..n)
        .map(|_| {
            let color = rng.pick(&CONFETTI_COLORS);
            let direction = rng.next() * std::f64::consts::TAU;
            let speed = rng.next() * 3.0;
            let phase = rng.next() * std::f64::consts::TAU;
            let square = Mob::rect(CONFETTI_SIZE, CONFETTI_SIZE, color).with_opacity(CONFETTI_OPACITY);
            let id = scene.func(Arc::new(move |t| {
                // frame k of the second-long animation; the fading compounds frame by frame
                let k = ((t - t0) * fps).round().max(0.0) as usize;
                let alpha = (k as f64 / fps).min(1.0);
                let mut opacity = CONFETTI_OPACITY;
                for i in 0..=k {
                    let a = (i as f64 / fps).min(1.0);
                    if a > 0.85 {
                        opacity *= (1.0 - a) / 0.15;
                    }
                }
                if opacity <= 0.0 {
                    return None;
                }
                let angle = alpha * 2.0 * std::f64::consts::TAU + phase;
                let dist = alpha * speed;
                let mut m = square.clone().move_to(x + dist * direction.cos(), y + dist * direction.sin());
                m.squash = angle.cos();
                m.angle = angle;
                m.opacity = opacity;
                Some(m)
            }));
            scene.end_at(id, t0 + 1.0);
            Anim::hold(id, 1.0)
        })
        .collect()
}

/// Confetti falling from the top for `duration` seconds, starting now; returns the squares
pub fn confetti_rain(scene: &mut Scene, rng: &mut Rng, n: usize, duration: f64) -> Vec<Id> {
    let t0 = scene.now();
    let (fw, fh) = scene.frame_size();
    const FALL: f64 = 5.0;
    const STEP: f64 = 1.0 / 60.0; // the drift of manim's updater, at 60 fps
    let ids: Vec<Id> = (0..n)
        .map(|i| {
            let delay = if n > 1 { duration * i as f64 / (n - 1) as f64 } else { 0.0 };
            let x0 = fw * rng.next() - fw / 2.0;
            let color = rng.pick(&CONFETTI_COLORS);
            let turns = 2.0 * rng.next();
            let turn_width = 0.03 * rng.next();
            let square = Mob::rect(CONFETTI_SIZE, CONFETTI_SIZE, color).with_opacity(CONFETTI_OPACITY);
            let y0 = fh / 2.0 + BUFF_UP + CONFETTI_SIZE / 2.0;
            scene.func(Arc::new(move |t| {
                let time = t - t0;
                if time < delay {
                    return Some(square.clone().move_to(x0, y0));
                }
                let (mut x, mut opacity) = (x0, CONFETTI_OPACITY);
                let mut s = (delay / STEP).ceil();
                while s * STEP <= time {
                    let tt = (s * STEP - delay) / FALL;
                    x += turn_width * (turns * tt * std::f64::consts::TAU).cos();
                    if s * STEP > delay + FALL * 0.85 && tt <= 1.0 {
                        opacity *= ((1.0 - tt) / 0.15).max(0.0);
                    }
                    s += 1.0;
                }
                let tt = (time - delay) / FALL;
                if opacity <= 0.0 || tt > 1.2 {
                    return None;
                }
                let mut m = square.clone().move_to(x, fh / 2.0 - fh * tt);
                m.squash = (std::f64::consts::TAU * (time - delay)).cos();
                m.opacity = opacity;
                Some(m)
            }))
        })
        .collect();
    scene.add(&ids);
    ids
}

const BUFF_UP: f64 = crate::timeline::BUFF;

/// The top-bar value during a timelapse: follows the score with an exponential approach,
/// updated frame by frame (as the manim updater did)
pub fn smoothed(tl: &Timelapse, targets: &[f64], speed: f64) -> Vec<f64> {
    let mut v = 0.0;
    (0..tl.frames)
        .map(|k| {
            let dt = if k == 0 { 0.0 } else { 1.0 / tl.fps };
            v += (targets[k] - v) * (1.0 - (-speed * dt).exp());
            v
        })
        .collect()
}

/// Value at time t of an animation from a to b that starts at t0 and lasts 1 second
pub fn tween(a: f64, b: f64, t0: f64, t: f64) -> f64 {
    a + (b - a) * smooth((t - t0).clamp(0.0, 1.0))
}
