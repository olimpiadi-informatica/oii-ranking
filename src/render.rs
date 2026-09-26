//! Rasterization of frames and the video pipeline: frames are rendered in parallel,
//! unchanged frames are reused, and everything is streamed to ffmpeg in order.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::sync_channel;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use kurbo::Affine;
use rayon::prelude::*;
use resvg::tiny_skia;

use crate::images::Images;
use crate::timeline::{Drawn, Scene, Vis};
use crate::vector::{self, Reveal};

/// Maps scene units to pixels
#[derive(Clone, Copy, Debug)]
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub fw: f64,
    pub fh: f64,
    /// Pixels per unit
    pub ppu: f64,
}

impl Canvas {
    pub fn new(w: u32, h: u32, fh: f64) -> Canvas {
        let ppu = h as f64 / fh;
        Canvas { w, h, fw: w as f64 / ppu, fh, ppu }
    }

    fn px(&self, x: f64, y: f64) -> (f64, f64) {
        ((x + self.fw / 2.0) * self.ppu, (self.fh / 2.0 - y) * self.ppu)
    }
}

/// Renders the frame at time t
pub fn render_frame(scene: &Scene, canvas: &Canvas, images: &Images, t: f64, epoch: usize) -> tiny_skia::Pixmap {
    let mut pixmap = tiny_skia::Pixmap::new(canvas.w, canvas.h).expect("frame size");
    if let Some(bg) = scene.background {
        pixmap.fill(tiny_skia::Color::from_rgba(bg.r, bg.g, bg.b, 1.0).unwrap());
    }
    for d in scene.frame_at(t) {
        draw(&mut pixmap, canvas, images, &d, epoch);
    }
    pixmap
}

fn draw(pixmap: &mut tiny_skia::Pixmap, canvas: &Canvas, _images: &Images, d: &Drawn, epoch: usize) {
    let m = &d.mob;
    let opacity = (d.opacity * m.opacity).clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let (cx, cy) = canvas.px(m.x, m.y);
    let k = canvas.ppu * m.s * d.scale;
    // shape units (y up) -> pixels
    let tf = Affine::translate((cx, cy)) * Affine::rotate(-m.angle) * Affine::scale_non_uniform(k * m.squash, -k);
    match &m.vis {
        Vis::Shape(shape) => vector::draw_shape(pixmap, shape, tf, canvas.ppu, opacity, d.reveal),
        Vis::Rect { w, h, color } => {
            if m.angle == 0.0 {
                let (w, h) = (w * k * m.squash.abs(), h * k);
                if let Some(r) =
                    tiny_skia::Rect::from_xywh((cx - w / 2.0) as f32, (cy - h / 2.0) as f32, w as f32, h as f32)
                {
                    pixmap.fill_rect(r, &vector::paint(*color, opacity), tiny_skia::Transform::identity(), None);
                }
            } else {
                let shape = vector::Shape::rect(*w, *h, *color);
                vector::draw_shape(pixmap, &shape, tf, canvas.ppu, opacity, Reveal::Full);
            }
        }
        Vis::Image(img) => {
            // resized once to the size it has at rest; animations scale that
            let (uw, uh) = img.units();
            let (w, h) = ((uw * m.s * canvas.ppu).round() as u32, (uh * m.s * canvas.ppu).round() as u32);
            let Some(src) = img.at_size(w, h, epoch) else { return };
            let mut paint = tiny_skia::PixmapPaint { opacity: opacity as f32, ..Default::default() };
            if d.scale == 1.0 {
                paint.quality = tiny_skia::FilterQuality::Nearest;
                let (x, y) = ((cx - w as f64 / 2.0).round() as i32, (cy - h as f64 / 2.0).round() as i32);
                pixmap.draw_pixmap(x, y, src.as_ref().as_ref(), &paint, tiny_skia::Transform::identity(), None);
            } else {
                paint.quality = tiny_skia::FilterQuality::Bilinear;
                let s = d.scale as f32;
                let ts = tiny_skia::Transform::from_translate(cx as f32, cy as f32)
                    .pre_scale(s, s)
                    .pre_translate(-(w as f32) / 2.0, -(h as f32) / 2.0);
                pixmap.draw_pixmap(0, 0, src.as_ref().as_ref(), &paint, ts, None);
            }
        }
    }
}

/// Straight (not premultiplied) RGBA bytes
fn straight_rgba(pixmap: tiny_skia::Pixmap) -> Vec<u8> {
    let mut data = pixmap.take();
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    data
}

/// Opaque RGBA to planar YUV 4:2:0 (BT.709, limited range): done here, in parallel, it is
/// much faster than in ffmpeg, and the frames sent to ffmpeg are 2.7 times smaller
fn rgba_to_yuv420(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut out = vec![0u8; w * h + 2 * cw * ch];
    let (y_plane, uv) = out.split_at_mut(w * h);
    let (u_plane, v_plane) = uv.split_at_mut(cw * ch);
    // coefficients scaled by 2^16, including the limited range (219/255 and 224/255)
    const KR: i32 = (0.2126 * 219.0 / 255.0 * 65536.0) as i32;
    const KG: i32 = (0.7152 * 219.0 / 255.0 * 65536.0) as i32;
    const KB: i32 = (0.0722 * 219.0 / 255.0 * 65536.0) as i32;
    const UR: i32 = (-0.1146 * 224.0 / 255.0 * 65536.0) as i32;
    const UG: i32 = (-0.3854 * 224.0 / 255.0 * 65536.0) as i32;
    const UB: i32 = (0.5 * 224.0 / 255.0 * 65536.0) as i32;
    const VR: i32 = (0.5 * 224.0 / 255.0 * 65536.0) as i32;
    const VG: i32 = (-0.4542 * 224.0 / 255.0 * 65536.0) as i32;
    const VB: i32 = (-0.0458 * 224.0 / 255.0 * 65536.0) as i32;
    for (y, row) in rgba.chunks_exact(w * 4).enumerate() {
        let yr = &mut y_plane[y * w..(y + 1) * w];
        for (x, px) in row.chunks_exact(4).enumerate() {
            let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
            yr[x] = ((16 << 16) + KR * r + KG * g + KB * b + (1 << 15)).clamp(0, 255 << 16).wrapping_shr(16) as u8;
        }
    }
    for cy in 0..ch {
        let (r0, r1) = (cy * 2, (cy * 2 + 1).min(h - 1));
        for cx in 0..cw {
            let (x0, x1) = (cx * 2, (cx * 2 + 1).min(w - 1));
            let (mut r, mut g, mut b) = (0, 0, 0);
            for (yy, xx) in [(r0, x0), (r0, x1), (r1, x0), (r1, x1)] {
                let i = (yy * w + xx) * 4;
                r += rgba[i] as i32;
                g += rgba[i + 1] as i32;
                b += rgba[i + 2] as i32;
            }
            // sums of 4 pixels: divide by 4 with the shift
            let u = (128 << 18) + UR * r + UG * g + UB * b + (1 << 17);
            let v = (128 << 18) + VR * r + VG * g + VB * b + (1 << 17);
            u_plane[cy * cw + cx] = (u >> 18).clamp(0, 255) as u8;
            v_plane[cy * cw + cx] = (v >> 18).clamp(0, 255) as u8;
        }
    }
    out
}

pub fn save_png(pixmap: tiny_skia::Pixmap, path: &Path) -> Result<()> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let img = image::RgbaImage::from_raw(w, h, straight_rgba(pixmap)).context("frame")?;
    img.save(path).with_context(|| format!("writing {}", path.display()))
}

pub struct VideoOptions {
    pub path: PathBuf,
    pub transparent: bool,
    pub encoder: String,
    pub encoder_options: Vec<String>,
    /// Frames to render: [first, last)
    pub frames: (usize, usize),
    pub label: String,
}

pub fn render_video(scene: &Scene, canvas: &Canvas, images: &Images, opts: &VideoOptions) -> Result<()> {
    let total = scene.total_frames();
    let (first, last) = (opts.frames.0.min(total), opts.frames.1.min(total));
    if first >= last {
        bail!("nothing to render: the scene has {total} frames");
    }
    if let Some(dir) = opts.path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    // write next to the target and rename at the end, so that a failed or interrupted
    // render never replaces a good video
    let ext = opts.path.extension().and_then(|e| e.to_str()).unwrap_or("mp4").to_string();
    let partial = opts.path.with_extension(format!("partial.{ext}"));

    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo"]);
    if opts.transparent {
        cmd.args(["-pix_fmt", "rgba"]);
    } else {
        cmd.args(["-pix_fmt", "yuv420p", "-colorspace", "bt709", "-color_range", "tv"]);
    }
    cmd.args(["-s", &format!("{}x{}", canvas.w, canvas.h), "-framerate", &format!("{}", scene.fps), "-i", "-"]);
    if opts.transparent {
        cmd.args(["-c:v", "prores_ks", "-profile:v", "4444", "-pix_fmt", "yuva444p10le"]);
    } else {
        cmd.args(["-c:v", &opts.encoder]).args(&opts.encoder_options).args(["-pix_fmt", "yuv420p"]);
        cmd.args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"]);
        cmd.args(["-movflags", "+faststart"]);
    }
    cmd.arg(&partial).stdin(Stdio::piped());
    let mut child = cmd.spawn().context("starting ffmpeg (is it installed?)")?;
    let mut stdin = child.stdin.take().unwrap();

    let changed = scene.changed_frames();
    let threads = rayon::current_num_threads();
    let (tx, rx) = sync_channel::<Arc<Vec<u8>>>(threads);
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for frame in rx {
            stdin.write_all(&frame)?;
        }
        Ok(())
    });

    let started = Instant::now();
    let mut previous: Option<Arc<Vec<u8>>> = None;
    let mut rendered = 0usize;
    // frames rendered together: enough to keep every thread busy, within about 1 GB
    let frame_bytes = canvas.w as usize * canvas.h as usize * 4;
    let chunk = ((1 << 30) / frame_bytes).clamp(threads, 2 * threads);
    let mut result = Ok(());
    'outer: for (epoch, start) in (first..last).step_by(chunk).enumerate() {
        let end = (start + chunk).min(last);
        let epoch = epoch + 1;
        let frames: Vec<Option<Arc<Vec<u8>>>> = (start..end)
            .into_par_iter()
            .map(|j| {
                (j == first || changed[j]).then(|| {
                    let pixmap = render_frame(scene, canvas, images, j as f64 / scene.fps, epoch);
                    Arc::new(if opts.transparent {
                        straight_rgba(pixmap)
                    } else {
                        rgba_to_yuv420(pixmap.data(), canvas.w as usize, canvas.h as usize)
                    })
                })
            })
            .collect();
        for frame in frames {
            if frame.is_some() {
                rendered += 1;
                previous = frame;
            }
            if tx.send(previous.clone().unwrap()).is_err() {
                result = Err(anyhow::anyhow!("ffmpeg stopped"));
                break 'outer;
            }
        }
        images.release_unused(epoch.saturating_sub(1));
        let done = end - first;
        let elapsed = started.elapsed().as_secs_f64();
        let eta = elapsed / done as f64 * (last - end) as f64;
        eprint!(
            "\r{}: frame {done}/{} ({:.0} fps, {} drawn, {:.0}s left)   ",
            opts.label,
            last - first,
            done as f64 / elapsed,
            rendered,
            eta
        );
    }
    drop(tx);
    eprintln!();
    let write_result = writer.join().expect("writer thread");
    let status = child.wait().context("waiting for ffmpeg")?;
    result?;
    write_result.context("sending frames to ffmpeg")?;
    if !status.success() {
        bail!("ffmpeg failed ({status}); the partial output is in {}", partial.display());
    }
    std::fs::rename(&partial, &opts.path).with_context(|| format!("renaming to {}", opts.path.display()))?;
    eprintln!(
        "{}: {} ({:.1}s of video in {:.1}s)",
        opts.label,
        opts.path.display(),
        (last - first) as f64 / scene.fps,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn yuv_of_known_colors() {
        let px = |r, g, b| [r, g, b, 255u8].repeat(4);
        for ((r, g, b), (y, u, v)) in
            [((255, 255, 255), (235, 128, 128)), ((0, 0, 0), (16, 128, 128)), ((255, 0, 0), (63, 102, 240))]
        {
            let out = super::rgba_to_yuv420(&px(r, g, b), 2, 2);
            assert_eq!(&out[..4], &[y; 4]);
            assert!((out[4] as i32 - u).abs() <= 1 && (out[5] as i32 - v).abs() <= 1, "{out:?}");
        }
    }
}
