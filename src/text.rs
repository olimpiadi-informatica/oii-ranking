//! Text: shaped with rustybuzz, turned into glyph outlines (one part per glyph, so that
//! Write can draw them one after the other).

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use kurbo::BezPath;
use rustybuzz::ttf_parser;

use crate::config::{Color, Config};
use crate::vector::{Part, Shape};

static REGULAR: &[u8] = include_bytes!("../fonts/NewCM10-Regular.otf");
static BOLD: &[u8] = include_bytes!("../fonts/NewCM10-Bold.otf");

/// Size of the em of manim's Tex at scale 1: LaTeX's 10pt, at 0.05 units per point
pub const TEX_EM: f64 = 0.5;

pub struct Font {
    face: rustybuzz::Face<'static>,
}

impl Font {
    fn from_data(data: &'static [u8], what: &str) -> Result<Font> {
        let face = rustybuzz::Face::from_slice(data, 0).with_context(|| format!("invalid font {what}"))?;
        Ok(Font { face })
    }

    fn from_file(path: &str) -> Result<Font> {
        let data = std::fs::read(path).with_context(|| format!("reading font {path}"))?;
        Font::from_data(Box::leak(data.into_boxed_slice()), path)
    }

    /// A font installed on the system, by family name
    pub fn system(family: &str) -> Result<Font> {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        let query =
            resvg::usvg::fontdb::Query { families: &[resvg::usvg::fontdb::Family::Name(family)], ..Default::default() };
        let Some(id) = db.query(&query) else { bail!("font {family:?} is not installed") };
        let Some((data, index)) = db.with_face_data(id, |data, index| (data.to_vec(), index)) else {
            bail!("cannot read font {family:?}")
        };
        let data: &'static [u8] = Box::leak(data.into_boxed_slice());
        let face = rustybuzz::Face::from_slice(data, index).with_context(|| format!("invalid font {family:?}"))?;
        Ok(Font { face })
    }

    pub fn regular() -> Font {
        Font::from_data(REGULAR, "NewCM10-Regular").expect("built-in font")
    }

    /// Glyph outlines of `text` with the given em size in units, centered on their bounding box
    pub fn shape(&self, text: &str, em: f64, color: Color) -> Shape {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let glyphs = rustybuzz::shape(&self.face, &[], buffer);
        let k = em / self.face.units_per_em() as f64;
        let mut parts = vec![];
        let mut pen = 0.0;
        for (info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
            let mut outline =
                Outline { path: BezPath::new(), k, dx: pen + pos.x_offset as f64 * k, dy: pos.y_offset as f64 * k };
            if self.face.outline_glyph(ttf_parser::GlyphId(info.glyph_id as u16), &mut outline).is_some() {
                parts.push(Part { path: outline.path, fill: Some(color), stroke: None });
            }
            pen += pos.x_advance as f64 * k;
        }
        Shape::new(parts)
    }
}

struct Outline {
    path: BezPath,
    k: f64,
    dx: f64,
    dy: f64,
}

impl Outline {
    fn p(&self, x: f32, y: f32) -> (f64, f64) {
        (self.dx + x as f64 * self.k, self.dy + y as f64 * self.k)
    }
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.path.move_to(p);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.path.line_to(p);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (a, b) = (self.p(x1, y1), self.p(x, y));
        self.path.quad_to(a, b);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (a, b, c) = (self.p(x1, y1), self.p(x2, y2), self.p(x, y));
        self.path.curve_to(a, b, c);
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}

/// The fonts of the ranking videos, standing in for LaTeX's \textrm and \textbf
pub struct Fonts {
    pub regular: Font,
    pub bold: Font,
}

impl Fonts {
    pub fn load(config: &Config) -> Result<Fonts> {
        let pick = |path: &str, data: &'static [u8], what| {
            if path.is_empty() { Font::from_data(data, what) } else { Font::from_file(path) }
        };
        Ok(Fonts {
            regular: pick(&config.fonts.regular, REGULAR, "NewCM10-Regular")?,
            bold: pick(&config.fonts.bold, BOLD, "NewCM10-Bold")?,
        })
    }

    /// manim's Tex(text).scale(scale)
    pub fn tex(&self, text: &str, scale: f64) -> Arc<Shape> {
        self.tex_colored(text, scale, Color::WHITE)
    }

    /// manim's Tex(r"\textbf{text}").scale(scale), in a color
    pub fn tex_bold(&self, text: &str, scale: f64, color: Color) -> Arc<Shape> {
        Arc::new(self.bold.shape(text, TEX_EM * scale, color))
    }

    pub fn tex_colored(&self, text: &str, scale: f64, color: Color) -> Arc<Shape> {
        Arc::new(self.regular.shape(&latex_ligatures(text), TEX_EM * scale, color))
    }
}

/// The punctuation LaTeX changes in text: quotes and dashes
fn latex_ligatures(text: &str) -> String {
    text.replace("---", "\u{2014}").replace("--", "\u{2013}").replace('\'', "\u{2019}").replace('`', "\u{2018}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_become_parts() {
        let f = Font::regular();
        let s = f.shape("PO 12", 0.5, Color::WHITE);
        assert_eq!(s.parts.len(), 4);
        // cap height of Computer Modern is about 0.68 em
        let cap = f.shape("H", 1.0, Color::WHITE);
        assert!((cap.h - 0.683).abs() < 0.01, "{}", cap.h);
        assert_eq!(latex_ligatures("D'Amico -- `x'"), "D\u{2019}Amico \u{2013} \u{2018}x\u{2019}");
    }
}
