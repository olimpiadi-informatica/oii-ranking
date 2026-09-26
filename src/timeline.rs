//! A manim-like scene builder. Scenes are written imperatively (`play`, `wait`), but what
//! gets recorded is a timeline: every frame is a pure function of its time, so frames can
//! be rendered in any order, in parallel, and unchanged frames can be detected.

use std::sync::Arc;

use crate::config::Color;
use crate::images::Img;
use crate::vector::{Reveal, Shape};

#[derive(Clone)]
pub enum Vis {
    Shape(Arc<Shape>),
    Image(Arc<Img>),
    /// Filled rectangle
    Rect {
        w: f64,
        h: f64,
        color: Color,
    },
}

/// A positioned visual (manim's Mobject): its center, uniform scale, and a few extras
#[derive(Clone)]
pub struct Mob {
    pub vis: Vis,
    pub x: f64,
    pub y: f64,
    pub s: f64,
    pub opacity: f64,
    /// Horizontal squash and rotation (radians), for confetti
    pub squash: f64,
    pub angle: f64,
}

/// Directions, as in manim
pub type Dir = (f64, f64);
pub const RIGHT: Dir = (1.0, 0.0);
pub const DOWN: Dir = (0.0, -1.0);
pub const UL: Dir = (-1.0, 1.0);
pub const UR: Dir = (1.0, 1.0);
pub const DL: Dir = (-1.0, -1.0);
pub const DR: Dir = (1.0, -1.0);

/// manim's DEFAULT_MOBJECT_TO_MOBJECT_BUFFER and MED_LARGE_BUFF
pub const BUFF: f64 = 0.25;
pub const EDGE_BUFF: f64 = 0.5;

impl Mob {
    pub fn new(vis: Vis) -> Mob {
        Mob { vis, x: 0.0, y: 0.0, s: 1.0, opacity: 1.0, squash: 1.0, angle: 0.0 }
    }

    pub fn shape(shape: Arc<Shape>) -> Mob {
        Mob::new(Vis::Shape(shape))
    }

    pub fn image(img: Arc<Img>) -> Mob {
        Mob::new(Vis::Image(img))
    }

    pub fn rect(w: f64, h: f64, color: Color) -> Mob {
        Mob::new(Vis::Rect { w, h, color })
    }

    /// Size at scale 1
    pub fn size0(&self) -> (f64, f64) {
        match &self.vis {
            Vis::Shape(s) => (s.w, s.h),
            Vis::Image(i) => i.units(),
            Vis::Rect { w, h, .. } => (*w, *h),
        }
    }

    pub fn width(&self) -> f64 {
        self.size0().0 * self.s
    }
    pub fn height(&self) -> f64 {
        self.size0().1 * self.s
    }
    pub fn left(&self) -> f64 {
        self.x - self.width() / 2.0
    }
    pub fn right(&self) -> f64 {
        self.x + self.width() / 2.0
    }
    pub fn top(&self) -> f64 {
        self.y + self.height() / 2.0
    }
    pub fn bottom(&self) -> f64 {
        self.y - self.height() / 2.0
    }

    pub fn scale(mut self, k: f64) -> Mob {
        self.s *= k;
        self
    }
    pub fn set_width(mut self, w: f64) -> Mob {
        self.s = w / self.size0().0;
        self
    }
    pub fn set_height(mut self, h: f64) -> Mob {
        self.s = h / self.size0().1;
        self
    }
    /// Shrinks to `max` width if wider
    pub fn fit_width(self, max: f64) -> Mob {
        if self.width() > max { self.set_width(max) } else { self }
    }
    pub fn move_to(mut self, x: f64, y: f64) -> Mob {
        self.x = x;
        self.y = y;
        self
    }
    pub fn set_x(mut self, x: f64) -> Mob {
        self.x = x;
        self
    }
    pub fn set_y(mut self, y: f64) -> Mob {
        self.y = y;
        self
    }
    pub fn set_left(mut self, v: f64) -> Mob {
        self.x = v + self.width() / 2.0;
        self
    }
    pub fn set_right(mut self, v: f64) -> Mob {
        self.x = v - self.width() / 2.0;
        self
    }
    pub fn set_top(mut self, v: f64) -> Mob {
        self.y = v - self.height() / 2.0;
        self
    }
    pub fn set_bottom(mut self, v: f64) -> Mob {
        self.y = v + self.height() / 2.0;
        self
    }
    pub fn shift(mut self, dx: f64, dy: f64) -> Mob {
        self.x += dx;
        self.y += dy;
        self
    }

    /// manim's next_to: beside `other` in direction `dir`, centers aligned on the other axis
    pub fn next_to(self, other: &Mob, dir: Dir, buff: f64) -> Mob {
        let (w, h) = (self.width(), self.height());
        let x = if dir.0 != 0.0 { other.x + dir.0 * (other.width() / 2.0 + buff + w / 2.0) } else { other.x };
        let y = if dir.1 != 0.0 { other.y + dir.1 * (other.height() / 2.0 + buff + h / 2.0) } else { other.y };
        self.move_to(x, y)
    }

    /// manim's to_edge / to_corner, on a frame of fw x fh units
    #[allow(clippy::wrong_self_convention)]
    pub fn to_edge(mut self, dir: Dir, frame: (f64, f64), buff: f64) -> Mob {
        if dir.0 != 0.0 {
            self.x = dir.0 * (frame.0 / 2.0 - buff - self.width() / 2.0);
        }
        if dir.1 != 0.0 {
            self.y = dir.1 * (frame.1 / 2.0 - buff - self.height() / 2.0);
        }
        self
    }

    pub fn with_opacity(mut self, o: f64) -> Mob {
        self.opacity = o;
        self
    }

    fn parts(&self) -> usize {
        match &self.vis {
            Vis::Shape(s) => s.parts.len(),
            _ => 1,
        }
    }
}

/// manim's VGroup(...).arrange(DOWN, aligned_edge=LEFT, buff=buff): stacks the mobjects,
/// left aligned, and returns them with the group's left edge at `left` and its vertical
/// center at `cy`
pub fn stack_left(mobs: Vec<Mob>, buff: f64, left: f64, cy: f64) -> Vec<Mob> {
    let total: f64 = mobs.iter().map(Mob::height).sum::<f64>() + buff * (mobs.len().saturating_sub(1)) as f64;
    let mut top = cy + total / 2.0;
    mobs.into_iter()
        .map(|m| {
            let m = m.set_left(left).set_top(top);
            top -= m.height() + buff;
            m
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rate {
    Linear,
    Smooth,
}

impl Rate {
    pub fn apply(self, t: f64) -> f64 {
        match self {
            Rate::Linear => t,
            Rate::Smooth => smooth(t),
        }
    }
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// manim's `smooth` rate function (inflection 10)
pub fn smooth(t: f64) -> f64 {
    let error = sigmoid(-5.0);
    ((sigmoid(10.0 * (t - 0.5)) - error) / (1.0 - 2.0 * error)).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Write,
    Unwrite,
    Create,
    FadeIn {
        shift: Dir,
        scale: f64,
    },
    FadeOut {
        shift: Dir,
        scale: f64,
    },
    /// Moves by (dx, dy)
    Shift(f64, f64),
    /// Nothing happens (the content changes on its own): keeps the play going
    Hold,
}

impl Kind {
    fn introduces(self) -> bool {
        matches!(self, Kind::Write | Kind::Create | Kind::FadeIn { .. })
    }
    fn removes(self) -> bool {
        matches!(self, Kind::Unwrite | Kind::FadeOut { .. })
    }
}

/// An animation to be played
#[derive(Clone, Copy, Debug)]
pub struct Anim {
    pub id: Id,
    pub kind: Kind,
    pub run_time: Option<f64>,
    pub delay: f64,
    pub rate: Option<Rate>,
}

impl Anim {
    fn new(id: Id, kind: Kind) -> Anim {
        Anim { id, kind, run_time: None, delay: 0.0, rate: None }
    }
    pub fn write(id: Id) -> Anim {
        Anim::new(id, Kind::Write)
    }
    pub fn unwrite(id: Id) -> Anim {
        Anim::new(id, Kind::Unwrite)
    }
    pub fn create(id: Id) -> Anim {
        Anim::new(id, Kind::Create)
    }
    pub fn fade_in(id: Id) -> Anim {
        Anim::new(id, Kind::FadeIn { shift: (0.0, 0.0), scale: 1.0 })
    }
    pub fn fade_out(id: Id) -> Anim {
        Anim::new(id, Kind::FadeOut { shift: (0.0, 0.0), scale: 1.0 })
    }
    pub fn shift_by(id: Id, dx: f64, dy: f64) -> Anim {
        Anim::new(id, Kind::Shift(dx, dy))
    }
    pub fn hold(id: Id, run_time: f64) -> Anim {
        Anim::new(id, Kind::Hold).run_time(run_time)
    }
    /// FadeIn(shift=...) / FadeOut(shift=...)
    pub fn shift(mut self, d: Dir) -> Anim {
        match &mut self.kind {
            Kind::FadeIn { shift, .. } | Kind::FadeOut { shift, .. } => *shift = d,
            _ => panic!("shift only applies to fades"),
        }
        self
    }
    /// FadeIn(scale=...) / FadeOut(scale=...)
    pub fn scale(mut self, k: f64) -> Anim {
        match &mut self.kind {
            Kind::FadeIn { scale, .. } | Kind::FadeOut { scale, .. } => *scale = k,
            _ => panic!("scale only applies to fades"),
        }
        self
    }
    pub fn run_time(mut self, t: f64) -> Anim {
        self.run_time = Some(t);
        self
    }
    pub fn delay(mut self, d: f64) -> Anim {
        self.delay = d;
        self
    }
}

pub type Id = usize;
pub type Func = Arc<dyn Fn(f64) -> Option<Mob> + Send + Sync>;

#[derive(Clone)]
enum Content {
    Static(Mob),
    /// Computed from the time of the frame
    Func(Func),
}

#[derive(Clone, Copy)]
struct Placed {
    t0: f64,
    dur: f64,
    kind: Kind,
    rate: Rate,
}

struct Obj {
    /// (from time, content), sorted
    contents: Vec<(f64, Content)>,
    start: f64,
    end: f64,
    z: usize,
    anims: Vec<Placed>,
}

/// What to draw for an object in a frame
pub struct Drawn {
    pub mob: Mob,
    /// Scale factor from the animations (about the center of the mob)
    pub scale: f64,
    pub opacity: f64,
    pub reveal: Reveal,
    /// Whether it changes during its segment (see [`Segments`]); the other objects look the
    /// same in every frame of the segment
    pub dynamic: bool,
}

/// The timeline cut at every event (an object appearing or disappearing, an animation
/// starting or ending, a change of content). Within a segment, every object either keeps the
/// same look or is dynamic for the whole segment.
pub struct Segments {
    starts: Vec<f64>,
}

impl Segments {
    /// Index of the segment containing time t
    pub fn index(&self, t: f64) -> usize {
        self.starts.partition_point(|&s| s <= t).saturating_sub(1)
    }

    /// A time inside segment i, away from its ends
    pub fn mid(&self, i: usize) -> f64 {
        match self.starts.get(i + 1) {
            Some(&next) => (self.starts[i] + next) / 2.0,
            None => self.starts[i] + 1.0,
        }
    }

    /// Number of frames in segment i
    pub fn frames(&self, i: usize, fps: f64) -> usize {
        let first = |t: f64| (t * fps - 1e-6).ceil().max(0.0) as usize;
        match self.starts.get(i + 1) {
            Some(&next) => first(next) - first(self.starts[i]),
            None => usize::MAX,
        }
    }
}

pub struct Scene {
    pub fps: f64,
    /// Frame size in units
    pub fw: f64,
    pub fh: f64,
    /// None = transparent
    pub background: Option<Color>,
    frame: usize,
    objs: Vec<Obj>,
    next_z: usize,
}

impl Scene {
    pub fn new(fps: f64, fw: f64, fh: f64, background: Option<Color>) -> Scene {
        Scene { fps, fw, fh, background, frame: 0, objs: vec![], next_z: 0 }
    }

    pub fn frame_size(&self) -> (f64, f64) {
        (self.fw, self.fh)
    }

    /// Time of the next frame to be recorded
    pub fn now(&self) -> f64 {
        self.frame as f64 / self.fps
    }

    pub fn total_frames(&self) -> usize {
        self.frame
    }

    /// A new mobject, not yet in the scene
    pub fn obj(&mut self, mob: Mob) -> Id {
        self.objs.push(Obj {
            contents: vec![(f64::NEG_INFINITY, Content::Static(mob))],
            start: f64::INFINITY,
            end: f64::INFINITY,
            z: 0,
            anims: vec![],
        });
        self.objs.len() - 1
    }

    /// A new object whose look is computed from the time
    pub fn func(&mut self, f: Func) -> Id {
        let id = self.obj(Mob::rect(0.0, 0.0, Color::WHITE));
        self.objs[id].contents = vec![(f64::NEG_INFINITY, Content::Func(f))];
        id
    }

    /// The current look of a mobject (the last static one)
    pub fn mob(&self, id: Id) -> &Mob {
        self.objs[id]
            .contents
            .iter()
            .rev()
            .find_map(|(_, c)| match c {
                Content::Static(m) => Some(m),
                Content::Func(_) => None,
            })
            .expect("object without a static look")
    }

    /// Adds objects to the scene, from now
    pub fn add(&mut self, ids: &[Id]) {
        let now = self.now();
        for &id in ids {
            self.introduce(id, now);
        }
    }

    fn introduce(&mut self, id: Id, t: f64) {
        let o = &mut self.objs[id];
        if o.start.is_infinite() {
            o.start = t;
            o.z = self.next_z;
            self.next_z += 1;
        } else {
            assert!(o.end > t, "object {id} was removed: create a new one to show it again");
        }
    }

    /// Removes an object at time t
    pub fn end_at(&mut self, id: Id, t: f64) {
        let o = &mut self.objs[id];
        o.end = o.end.min(t);
    }

    /// Changes the look of an object from now on (manim's `become`)
    pub fn become_mob(&mut self, id: Id, mob: Mob) {
        let now = self.now();
        self.objs[id].contents.push((now, Content::Static(mob)));
    }

    /// Makes the look of an object computed from the time, from now on
    pub fn become_func(&mut self, id: Id, f: Func) {
        let now = self.now();
        self.objs[id].contents.push((now, Content::Func(f)));
    }

    /// Default run time of an animation (Write takes longer for long texts)
    pub fn run_time(&self, a: &Anim) -> f64 {
        a.run_time.unwrap_or_else(|| match a.kind {
            Kind::Write | Kind::Unwrite => {
                if self.mob(a.id).parts() < 15 {
                    1.0
                } else {
                    2.0
                }
            }
            _ => 1.0,
        })
    }

    /// Plays animations together; each lasts its own run time, the play lasts the longest
    pub fn play(&mut self, anims: Vec<Anim>) {
        let t0 = self.now();
        let mut longest: f64 = 0.0;
        for a in anims {
            let dur = self.run_time(&a);
            let start = t0 + a.delay;
            longest = longest.max(a.delay + dur);
            let rate = a.rate.unwrap_or(match a.kind {
                Kind::Write | Kind::Unwrite | Kind::Hold => Rate::Linear,
                _ => Rate::Smooth,
            });
            // the object joins the scene when the play starts (invisible until its animation starts)
            self.introduce(a.id, t0);
            let o = &mut self.objs[a.id];
            if a.kind.removes() {
                o.end = o.end.min(start + dur);
            }
            o.anims.push(Placed { t0: start, dur, kind: a.kind, rate });
        }
        self.advance(longest);
    }

    pub fn wait(&mut self, d: f64) {
        self.advance(d);
    }

    /// Like manim, a play of `d` seconds lasts ceil(d * fps) frames (at least one)
    fn advance(&mut self, d: f64) {
        let frames = ((d * self.fps - 1e-6).ceil() as usize).max(1);
        self.frame += frames;
    }

    pub fn segments(&self) -> Segments {
        let mut starts = vec![0.0];
        for o in self.objs.iter().filter(|o| o.start.is_finite()) {
            starts.push(o.start);
            if o.end.is_finite() {
                starts.push(o.end);
            }
            for a in o.anims.iter().filter(|a| !matches!(a.kind, Kind::Hold)) {
                starts.extend([a.t0, a.t0 + a.dur]);
            }
            starts.extend(o.contents.iter().map(|(from, _)| *from).filter(|t| t.is_finite()));
        }
        starts.retain(|&t| t >= 0.0);
        starts.sort_by(f64::total_cmp);
        starts.dedup();
        Segments { starts }
    }

    /// What is on screen at time t, bottom to top
    pub fn frame_at(&self, t: f64) -> Vec<Drawn> {
        self.frame_in_segment(t, t)
    }

    /// What is on screen at time t, bottom to top, with `dynamic` set for the objects that
    /// change during the segment containing t (`mid` is a time inside it: see [`Segments::mid`])
    pub fn frame_in_segment(&self, t: f64, mid: f64) -> Vec<Drawn> {
        let mut out: Vec<(usize, Drawn)> = vec![];
        for o in &self.objs {
            if !(o.start <= t && t < o.end) {
                continue;
            }
            let content_at =
                |t: f64| &o.contents[o.contents.partition_point(|(from, _)| *from <= t).saturating_sub(1)].1;
            let dynamic = matches!(content_at(mid), Content::Func(_))
                || o.anims.iter().any(|a| !matches!(a.kind, Kind::Hold) && a.t0 < mid && mid < a.t0 + a.dur);
            let Some(mut mob) = (match content_at(t) {
                Content::Static(m) => Some(m.clone()),
                Content::Func(f) => f(t),
            }) else {
                continue;
            };
            let (mut dx, mut dy, mut scale, mut opacity) = (0.0, 0.0, 1.0, 1.0);
            let mut reveal = Reveal::Full;
            for a in &o.anims {
                if t < a.t0 {
                    // not started: an introduced object is still invisible
                    if a.kind.introduces() {
                        opacity = 0.0;
                    }
                    continue;
                }
                let raw = if a.dur > 0.0 { ((t - a.t0) / a.dur).min(1.0) } else { 1.0 };
                let alpha = a.rate.apply(raw);
                match a.kind {
                    Kind::Write if raw < 1.0 => reveal = Reveal::Write(alpha),
                    Kind::Unwrite => reveal = Reveal::Write(1.0 - alpha),
                    Kind::Create if raw < 1.0 => reveal = Reveal::Create(alpha),
                    Kind::FadeIn { shift, scale: k } => {
                        opacity *= alpha;
                        dx -= shift.0 * (1.0 - alpha);
                        dy -= shift.1 * (1.0 - alpha);
                        scale *= k + (1.0 - k) * alpha;
                    }
                    Kind::FadeOut { shift, scale: k } => {
                        opacity *= 1.0 - alpha;
                        dx += shift.0 * alpha;
                        dy += shift.1 * alpha;
                        scale *= 1.0 + (k - 1.0) * alpha;
                    }
                    Kind::Shift(sx, sy) => {
                        dx += sx * alpha;
                        dy += sy * alpha;
                    }
                    _ => {}
                }
            }
            if opacity <= 0.0 || reveal == Reveal::Write(0.0) || reveal == Reveal::Create(0.0) {
                continue;
            }
            mob.x += dx;
            mob.y += dy;
            out.push((o.z, Drawn { mob, scale, opacity, reveal, dynamic }));
        }
        out.sort_by_key(|(z, _)| *z);
        out.into_iter().map(|(_, d)| d).collect()
    }

    /// For every frame, whether it can differ from the previous one
    pub fn changed_frames(&self) -> Vec<bool> {
        let n = self.frame;
        let mut changed = vec![false; n];
        if n == 0 {
            return changed;
        }
        changed[0] = true;
        let fps = self.fps;
        // frames j with j/fps > a and (j-1)/fps < b, i.e. whose step (t_{j-1}, t_j] meets (a, b]
        let mut mark = |a: f64, b: f64| {
            if a >= b {
                return;
            }
            let lo = ((a * fps + 1e-6).floor() as i64 + 1).max(0) as usize;
            let hi = (((b * fps - 1e-6).ceil() as i64) + 1).clamp(0, n as i64) as usize;
            for c in changed.iter_mut().take(hi).skip(lo) {
                *c = true;
            }
        };
        let eps = 0.5 / fps;
        for o in &self.objs {
            if o.start.is_infinite() {
                continue;
            }
            // appearing and disappearing
            mark(o.start - eps, o.start);
            if o.end.is_finite() {
                mark(o.end - eps, o.end);
            }
            for a in &o.anims {
                if !matches!(a.kind, Kind::Hold) {
                    mark(a.t0, a.t0 + a.dur);
                }
            }
            for (i, (from, c)) in o.contents.iter().enumerate() {
                let until = o.contents.get(i + 1).map_or(o.end, |(t, _)| *t);
                let from = from.max(o.start);
                match c {
                    Content::Static(_) => mark(from - eps, from),
                    Content::Func(_) => mark(from - eps, until.min(n as f64 / fps)),
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_functions_match_manim() {
        assert_eq!(smooth(0.0), 0.0);
        assert!((smooth(0.5) - 0.5).abs() < 1e-12);
        assert_eq!(smooth(1.0), 1.0);
        assert!((smooth(0.25) - 0.070103716545).abs() < 1e-9);
        assert!((smooth(0.1) - 0.011446579539).abs() < 1e-9);
    }

    #[test]
    fn frames_like_numpy_arange() {
        for (fps, d, frames) in [(15.0, 1.5, 23), (30.0, 0.3, 9), (60.0, 0.3, 18), (60.0, 0.001, 1), (15.0, 0.25, 4)] {
            let mut s = Scene::new(fps, 16.0, 9.0, None);
            s.wait(d);
            assert_eq!(s.total_frames(), frames, "{fps} {d}");
        }
    }

    #[test]
    fn fades_and_static_frames() {
        let mut s = Scene::new(10.0, 16.0, 9.0, None);
        let a = s.obj(Mob::rect(1.0, 1.0, Color::WHITE));
        s.wait(1.0);
        s.play(vec![Anim::fade_in(a).shift(RIGHT)]);
        s.wait(1.0);
        s.play(vec![Anim::fade_out(a)]);
        assert_eq!(s.total_frames(), 40);
        assert!(s.frame_at(0.5).is_empty());
        let mid = s.frame_at(1.5);
        assert!((mid[0].opacity - 0.5).abs() < 1e-9 && (mid[0].mob.x + 0.5).abs() < 1e-9);
        assert_eq!(s.frame_at(2.5)[0].opacity, 1.0);
        assert!(s.frame_at(4.0).is_empty());
        let changed = s.changed_frames();
        assert!(changed[0] && !changed[5] && changed[10] && changed[15] && changed[20]);
        assert!(!changed[21] && !changed[29] && changed[31] && changed[39]);
    }
}
