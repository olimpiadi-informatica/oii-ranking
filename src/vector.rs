//! Vector shapes (glyph outlines, the cup, circles) and how they are drawn, including the
//! progressive drawing of manim's Write and Create.

use std::path::Path;

use anyhow::{Context, Result};
use kurbo::{Affine, BezPath, ParamCurve, PathEl, PathSeg, Point, Rect};
use resvg::tiny_skia;
use resvg::usvg;

use crate::config::Color;

/// Width of the outline drawn by Write (manim: stroke_width 2 = 0.02 units)
const WRITE_STROKE: f64 = 0.02;

#[derive(Clone, Debug)]
pub struct Part {
    /// In units, y up, relative to the center of the shape
    pub path: BezPath,
    pub fill: Option<Color>,
    /// Color and width in units (strokes do not scale with the shape, as in manim)
    pub stroke: Option<(Color, f64)>,
}

/// A vector graphic made of parts (e.g. one per glyph), centered on its bounding box
#[derive(Clone, Debug)]
pub struct Shape {
    pub parts: Vec<Part>,
    pub w: f64,
    pub h: f64,
}

impl Shape {
    /// Centers the parts on the bounding box of their control points (as manim does)
    pub fn new(mut parts: Vec<Part>) -> Shape {
        parts.retain(|p| p.path.elements().len() > 1);
        let bbox = parts.iter().map(|p| p.path.control_box()).reduce(|a, b| a.union(b)).unwrap_or(Rect::ZERO);
        let shift = Affine::translate(-bbox.center().to_vec2());
        for p in &mut parts {
            p.path.apply_affine(shift);
        }
        Shape { parts, w: bbox.width(), h: bbox.height() }
    }

    /// A circle outline (manim's Circle: starts on the right, counterclockwise)
    pub fn circle(radius: f64, color: Color, stroke_width: f64) -> Shape {
        let mut path = BezPath::new();
        let arcs = 8;
        let k = 4.0 / 3.0 * (std::f64::consts::PI / (2.0 * arcs as f64)).tan();
        let pt = |a: f64| Point::new(radius * a.cos(), radius * a.sin());
        path.move_to(pt(0.0));
        for i in 0..arcs {
            let (a0, a1) =
                (i as f64 * std::f64::consts::TAU / arcs as f64, (i + 1) as f64 * std::f64::consts::TAU / arcs as f64);
            let (p0, p1) = (pt(a0), pt(a1));
            let c0 = p0 + k * kurbo::Vec2::new(-a0.sin(), a0.cos()) * radius;
            let c1 = p1 - k * kurbo::Vec2::new(-a1.sin(), a1.cos()) * radius;
            path.curve_to(c0, c1, p1);
        }
        let mut s = Shape::new(vec![Part { path, fill: None, stroke: Some((color, stroke_width)) }]);
        // the bounding box of a circle is the circle, not its control points
        s.w = 2.0 * radius;
        s.h = 2.0 * radius;
        s
    }

    /// A filled rectangle
    pub fn rect(w: f64, h: f64, fill: Color) -> Shape {
        let mut path = BezPath::new();
        path.move_to((-w / 2.0, h / 2.0));
        path.line_to((w / 2.0, h / 2.0));
        path.line_to((w / 2.0, -h / 2.0));
        path.line_to((-w / 2.0, -h / 2.0));
        path.close_path();
        Shape::new(vec![Part { path, fill: Some(fill), stroke: None }])
    }

    /// Loads an SVG file, scaled to the given height, with all its parts in one color
    /// (manim's SVGMobject + set_color)
    pub fn svg(path: &Path, height: f64, color: Color) -> Result<Shape> {
        let data = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Shape::svg_str(&data, height, color).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn svg_str(data: &str, height: f64, color: Color) -> Result<Shape> {
        let tree = usvg::Tree::from_str(data, &usvg::Options::default())?;
        let mut parts = vec![];
        collect_svg(tree.root(), &mut parts, color);
        // svg is y down
        for p in &mut parts {
            p.path.apply_affine(Affine::FLIP_Y);
        }
        let shape = Shape::new(parts);
        let k = if shape.h > 0.0 { height / shape.h } else { 1.0 };
        Ok(shape.scaled(k))
    }

    pub fn scaled(mut self, k: f64) -> Shape {
        for p in &mut self.parts {
            p.path.apply_affine(Affine::scale(k));
        }
        self.w *= k;
        self.h *= k;
        self
    }
}

fn collect_svg(group: &usvg::Group, out: &mut Vec<Part>, color: Color) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect_svg(g, out, color),
            usvg::Node::Path(p) => {
                let ts = p.abs_transform();
                let tf = Affine::new([ts.sx, ts.ky, ts.kx, ts.sy, ts.tx, ts.ty].map(f64::from));
                let mut path = BezPath::new();
                for seg in p.data().segments() {
                    use tiny_skia::PathSegment as S;
                    let pt = |p: tiny_skia::Point| Point::new(p.x as f64, p.y as f64);
                    match seg {
                        S::MoveTo(a) => path.move_to(pt(a)),
                        S::LineTo(a) => path.line_to(pt(a)),
                        S::QuadTo(a, b) => path.quad_to(pt(a), pt(b)),
                        S::CubicTo(a, b, c) => path.curve_to(pt(a), pt(b), pt(c)),
                        S::Close => path.close_path(),
                    }
                }
                path.apply_affine(tf);
                let stroke = p.stroke().map(|s| (color, s.width().get() as f64 * 0.01));
                out.push(Part { path, fill: p.fill().map(|_| color), stroke });
            }
            _ => {}
        }
    }
}

/// How much of a shape is drawn
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reveal {
    Full,
    /// manim's Write: each part draws its outline, then fills in; parts start one after the other
    Write(f64),
    /// manim's Create: parts appear one after the other, following their path
    Create(f64),
}

/// Draws `shape` with `tf` mapping shape units to pixels; `ppu` is pixels per unit (for strokes)
pub fn draw_shape(pixmap: &mut tiny_skia::Pixmap, shape: &Shape, tf: Affine, ppu: f64, opacity: f64, reveal: Reveal) {
    let n = shape.parts.len();
    for (i, part) in shape.parts.iter().enumerate() {
        match reveal {
            Reveal::Full => draw_part(pixmap, &part.path, tf, part.fill, part.stroke, ppu, opacity),
            Reveal::Create(alpha) => {
                let sub = (alpha * n as f64 - i as f64).clamp(0.0, 1.0);
                if sub > 0.0 {
                    let path = if sub < 1.0 { partial_path(&part.path, sub) } else { part.path.clone() };
                    draw_part(pixmap, &path, tf, part.fill, part.stroke, ppu, opacity);
                }
            }
            Reveal::Write(alpha) => {
                let lag = (4.0 / n.max(1) as f64).min(0.2);
                let full = (n as f64 - 1.0) * lag + 1.0;
                let sub = (alpha * full - i as f64 * lag).clamp(0.0, 1.0);
                let outline = part.stroke.map(|s| s.0).or(part.fill).unwrap_or(Color::WHITE);
                if sub <= 0.0 {
                } else if sub < 0.5 {
                    let path = partial_path(&part.path, 2.0 * sub);
                    draw_part(pixmap, &path, tf, None, Some((outline, WRITE_STROKE)), ppu, opacity);
                } else {
                    // from the outline to the final style
                    let t = 2.0 * sub - 1.0;
                    let fill = part.fill.map(|c| c.with_alpha(c.a * t as f32));
                    let (target, target_w) = part.stroke.unwrap_or((outline, 0.0));
                    let stroke_color = lerp_color(outline, target, t);
                    let width = WRITE_STROKE + (target_w - WRITE_STROKE) * t;
                    draw_part(pixmap, &part.path, tf, fill, Some((stroke_color, width)), ppu, opacity);
                }
            }
        }
    }
}

fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    let t = t as f32;
    Color { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: a.a + (b.a - a.a) * t }
}

pub fn paint(color: Color, opacity: f64) -> tiny_skia::Paint<'static> {
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(
        tiny_skia::Color::from_rgba(
            color.r.clamp(0.0, 1.0),
            color.g.clamp(0.0, 1.0),
            color.b.clamp(0.0, 1.0),
            (color.a * opacity as f32).clamp(0.0, 1.0),
        )
        .unwrap_or(tiny_skia::Color::TRANSPARENT),
    );
    paint.anti_alias = true;
    paint
}

fn draw_part(
    pixmap: &mut tiny_skia::Pixmap,
    path: &BezPath,
    tf: Affine,
    fill: Option<Color>,
    stroke: Option<(Color, f64)>,
    ppu: f64,
    opacity: f64,
) {
    let Some(sk) = to_skia(path, tf) else { return };
    if let Some(c) = fill
        && c.a > 0.0
    {
        pixmap.fill_path(&sk, &paint(c, opacity), tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
    }
    if let Some((c, w)) = stroke {
        let width = (w * ppu) as f32;
        if width > 0.01 && c.a > 0.0 {
            let stroke = tiny_skia::Stroke {
                width,
                line_join: tiny_skia::LineJoin::Round,
                line_cap: tiny_skia::LineCap::Round,
                ..Default::default()
            };
            pixmap.stroke_path(&sk, &paint(c, opacity), &stroke, tiny_skia::Transform::identity(), None);
        }
    }
}

pub fn to_skia(path: &BezPath, tf: Affine) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    let p = |pt: Point| {
        let q = tf * pt;
        (q.x as f32, q.y as f32)
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(a) => {
                let a = p(a);
                pb.move_to(a.0, a.1)
            }
            PathEl::LineTo(a) => {
                let a = p(a);
                pb.line_to(a.0, a.1)
            }
            PathEl::QuadTo(a, b) => {
                let (a, b) = (p(a), p(b));
                pb.quad_to(a.0, a.1, b.0, b.1)
            }
            PathEl::CurveTo(a, b, c) => {
                let (a, b, c) = (p(a), p(b), p(c));
                pb.cubic_to(a.0, a.1, b.0, b.1, c.0, c.1)
            }
            PathEl::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

/// The first `frac` of a path, counted in curves (manim's pointwise_become_partial)
pub fn partial_path(path: &BezPath, frac: f64) -> BezPath {
    let segs: Vec<PathSeg> = path.segments().collect();
    let x = frac.clamp(0.0, 1.0) * segs.len() as f64;
    let whole = x.floor() as usize;
    let mut out = BezPath::new();
    let mut last: Option<Point> = None;
    let mut push = |seg: PathSeg, out: &mut BezPath| {
        if last.is_none_or(|l| (l - seg.start()).hypot() > 1e-9) {
            out.move_to(seg.start());
        }
        match seg {
            PathSeg::Line(l) => out.line_to(l.p1),
            PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
            PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
        }
        last = Some(seg.end());
    };
    for seg in &segs[..whole.min(segs.len())] {
        push(*seg, &mut out);
    }
    if whole < segs.len() && x > whole as f64 {
        push(segs[whole].subsegment(0.0..x - whole as f64), &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_path_counts_curves() {
        let s = Shape::rect(2.0, 2.0, Color::WHITE);
        let path = &s.parts[0].path;
        assert_eq!(path.segments().count(), 4);
        let half = partial_path(path, 0.5);
        assert_eq!(half.segments().count(), 2);
        let bit = partial_path(path, 0.125);
        let end = bit.segments().last().unwrap().end();
        assert!((end.x - 0.0).abs() < 1e-9 && (end.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn circle_size() {
        let c = Shape::circle(0.5, Color::WHITE, 0.04);
        assert_eq!((c.w, c.h), (1.0, 1.0));
    }
}
