//! Transparent overlays for the live stream (port of overlays.py)

use std::sync::Arc;

use crate::config::Color;
use crate::text::Font;
use crate::timeline::{Anim, DOWN, Mob, RIGHT, Scene};
use crate::vector::Shape;

/// manim's Text: an em of font_size / 96 units
fn text(font: &Font, s: &str, font_size: f64, color: Color) -> Mob {
    Mob::shape(Arc::new(font.shape(s, font_size / 96.0, color)))
}

fn font(family: &str) -> Font {
    Font::system(family).unwrap_or_else(|e| {
        eprintln!("!!! {e:#}: using the built-in font");
        Font::regular()
    })
}

/// Counts down from n to 0, a number per second
pub fn countdown(fps: f64, aspect: f64, n: u32) -> Scene {
    let mut scene = Scene::new(fps, 8.0 * aspect, 8.0, None);
    let font = font("OCR A Extended");
    for i in (0..=n).rev() {
        let t = scene.obj(text(&font, &format!("{i:02}"), 144.0, Color::WHITE));
        scene.play(vec![Anim::fade_in(t).scale(0.0).run_time(0.5)]);
        scene.play(vec![Anim::fade_out(t).scale(5.0).run_time(0.5)]);
    }
    scene
}

/// A lower third with the name and the title of a person
pub fn name_marker(fps: f64, aspect: f64, name: &str, title: &str) -> Scene {
    let mut scene = Scene::new(fps, 8.0 * aspect, 8.0, None);
    let frame = scene.frame_size();
    let font = font("Futura");
    let rect = Mob::shape(Arc::new(Shape::rect(8.0, 1.0, Color::hex("#C0C0C0").unwrap().with_alpha(0.8))))
        .to_edge(RIGHT, frame, 0.0)
        .to_edge(DOWN, frame, 0.5);
    let n = text(&font, name, if name.chars().count() < 33 { 36.0 } else { 32.0 }, Color::BLACK);
    let t = text(&font, title, 24.0, Color::BLACK);
    // stacked, left aligned, 6.4 units from the left edge and 0.55 from the bottom
    let left = -frame.0 / 2.0 + 6.4;
    let bottom = -frame.1 / 2.0 + 0.55;
    let t = t.set_left(left).set_bottom(bottom);
    let n = n.set_left(left).set_bottom(t.top());
    let (r, n, t) = (scene.obj(rect), scene.obj(n), scene.obj(t));
    let lag = |a: Anim, b: Anim, s: &Scene| {
        let d = 0.3 * s.run_time(&a);
        vec![a, b.delay(d)]
    };
    scene.wait(0.5);
    scene.play(vec![Anim::write(r)]);
    let anims = lag(Anim::write(n), Anim::write(t), &scene);
    scene.play(anims);
    scene.wait(3.0);
    let anims = lag(Anim::unwrite(t), Anim::unwrite(n), &scene);
    scene.play(anims);
    scene.play(vec![Anim::unwrite(r)]);
    scene.wait(0.5);
    scene
}
