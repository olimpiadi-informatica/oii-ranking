//! Rasterization of frames and the video pipeline: frames are rendered in parallel,
//! unchanged frames are reused, and everything is streamed to ffmpeg in order.

use std::cell::RefCell;
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
use yuvutils_rs::{BufferStoreMut, YuvConversionMode, YuvPlanarImageMut, YuvRange, YuvStandardMatrix};

use crate::encoder::Encoder;
use crate::images::Images;
use crate::interrupt;
use crate::preview::{self, Preview};
use crate::timeline::{Drawn, Scene, Vis};
use crate::vector::{self, Reveal};

/// Maps scene units to the pixels of the frame, or of a region of it
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
pub fn render_frame(scene: &Scene, canvas: &Canvas, _images: &Images, t: f64, epoch: usize) -> tiny_skia::Pixmap {
    let mut pixmap = tiny_skia::Pixmap::new(canvas.w, canvas.h).expect("frame size");
    if let Some(bg) = scene.background {
        pixmap.fill(tiny_skia::Color::from_rgba(bg.r, bg.g, bg.b, 1.0).unwrap());
    }
    let mut pm = pixmap.as_mut();
    for d in scene.frame_at(t) {
        draw(&mut pm, canvas, &d, epoch);
    }
    pixmap
}

fn draw(pixmap: &mut tiny_skia::PixmapMut, canvas: &Canvas, d: &Drawn, epoch: usize) {
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

/// Opaque RGBA to planar YUV 4:2:0 (hand-written: kept as the reference of the tests)
#[cfg(test)]
fn rgba_to_yuv420_reference(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
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

/// An opaque RGBA frame to planar YUV 4:2:0 (BT.709, limited range)
fn to_yuv(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let (cw, ch) = ((w as usize).div_ceil(2), (h as usize).div_ceil(2));
    let mut frame = vec![0u8; yuv_size(w, h)];
    let (y_plane, uv) = frame.split_at_mut(w as usize * h as usize);
    let (u_plane, v_plane) = uv.split_at_mut(cw * ch);
    let mut planes = YuvPlanarImageMut {
        y_plane: BufferStoreMut::Borrowed(y_plane),
        y_stride: w,
        u_plane: BufferStoreMut::Borrowed(u_plane),
        u_stride: cw as u32,
        v_plane: BufferStoreMut::Borrowed(v_plane),
        v_stride: cw as u32,
        width: w,
        height: h,
    };
    yuvutils_rs::rgba_to_yuv420(
        &mut planes,
        rgba,
        w * 4,
        YuvRange::Limited,
        YuvStandardMatrix::Bt709,
        YuvConversionMode::Balanced,
    )
    .expect("YUV conversion");
    frame
}

fn yuv_size(w: u32, h: u32) -> usize {
    let (w, h) = (w as usize, h as usize);
    w * h + 2 * w.div_ceil(2) * h.div_ceil(2)
}

thread_local! {
    /// The pixels of the frame being drawn, reused from frame to frame
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// The frame at time t in YUV 4:2:0 (the scene must have a background)
fn yuv_frame(scene: &Scene, canvas: &Canvas, t: f64, epoch: usize) -> Vec<u8> {
    SCRATCH.with_borrow_mut(|buf| {
        let (w, h) = (canvas.w, canvas.h);
        buf.resize(w as usize * h as usize * 4, 0);
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(buf, w, h).expect("frame size");
        let bg = scene.background.expect("YUV frames are opaque");
        pixmap.fill(tiny_skia::Color::from_rgba(bg.r, bg.g, bg.b, 1.0).unwrap());
        for d in scene.frame_at(t) {
            draw(&mut pixmap, canvas, &d, epoch);
        }
        to_yuv(buf, w, h)
    })
}

pub fn save_png(pixmap: tiny_skia::Pixmap, path: &Path) -> Result<()> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let img = image::RgbaImage::from_raw(w, h, straight_rgba(pixmap)).context("frame")?;
    img.save(path).with_context(|| format!("writing {}", path.display()))
}

pub struct VideoOptions {
    pub path: PathBuf,
    pub transparent: bool,
    /// Encoders to try in order: if one fails, the video is rendered again with the next
    pub encoders: Vec<Encoder>,
    /// Frames to render: [first, last)
    pub frames: (usize, usize),
    pub label: String,
    /// The window showing the video while it is rendered
    pub preview: preview::Mode,
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

    for (i, encoder) in opts.encoders.iter().enumerate() {
        match encode(scene, canvas, images, opts, (first, last), &partial, encoder) {
            Ok(elapsed) => {
                std::fs::rename(&partial, &opts.path)
                    .with_context(|| format!("renaming to {}", opts.path.display()))?;
                eprintln!(
                    "{}: {} ({:.1}s of video in {:.1}s)",
                    opts.label,
                    opts.path.display(),
                    (last - first) as f64 / scene.fps,
                    elapsed
                );
                return Ok(());
            }
            Err(e) if e.is::<interrupt::Interrupted>() => {
                eprintln!("{}: stopped; the video rendered so far is in {}", opts.label, partial.display());
                return Err(e);
            }
            Err(e) => match opts.encoders.get(i + 1) {
                Some(next) => {
                    eprintln!(
                        "!!! {} failed: {e:#}\n!!! rendering {} again with {}",
                        encoder.name, opts.label, next.name
                    )
                }
                None => return Err(e.context(format!("the partial output is in {}", partial.display()))),
            },
        }
    }
    bail!("no video encoder")
}

/// Renders the frames and encodes them with `encoder` into `partial`; returns the seconds taken
fn encode(
    scene: &Scene,
    canvas: &Canvas,
    images: &Images,
    opts: &VideoOptions,
    (first, last): (usize, usize),
    partial: &Path,
    encoder: &Encoder,
) -> Result<f64> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-y"]).args(&encoder.global).args(["-f", "rawvideo"]);
    if opts.transparent {
        cmd.args(["-pix_fmt", "rgba"]);
    } else {
        cmd.args(["-pix_fmt", "yuv420p", "-colorspace", "bt709", "-color_range", "tv"]);
    }
    cmd.args(["-s", &format!("{}x{}", canvas.w, canvas.h), "-framerate", &format!("{}", scene.fps), "-i", "-"]);
    cmd.args(&encoder.output);
    if !opts.transparent {
        cmd.args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"]);
        cmd.args(["-movflags", "+faststart"]);
    }
    cmd.arg(partial).stdin(Stdio::piped());
    // from now on, Ctrl+C stops the render cleanly
    let _rendering = interrupt::Rendering::start();
    let mut child = interrupt::spawn_detached(&mut cmd).context("starting ffmpeg (is it installed?)")?;
    let mut stdin = child.stdin.take().unwrap();

    let changed = scene.changed_frames();
    let threads = rayon::current_num_threads();
    // frames rendered together: enough to keep every thread busy, within about 1 GB
    let frame_bytes = canvas.w as usize * canvas.h as usize * 4;
    let chunk = ((1 << 30) / frame_bytes).clamp(threads, 2 * threads);
    // room for two chunks: the next chunk is rendered while ffmpeg encodes the last one
    let (tx, rx) = sync_channel::<Arc<Vec<u8>>>(2 * chunk);
    // the window showing the video (not for the transparent overlays)
    let preview = match opts.transparent {
        true => None,
        false => Preview::open(opts.preview, canvas.w, canvas.h, scene.fps, &opts.label),
    };
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        let mut preview = preview;
        for frame in rx {
            // in the realtime mode this waits for the window, and so the render does
            if let Some(p) = &mut preview {
                p.show(&frame);
            }
            stdin.write_all(&frame)?;
        }
        if let Some(p) = preview {
            p.finish();
        }
        Ok(())
    });

    let started = Instant::now();
    let mut previous: Option<Arc<Vec<u8>>> = None;
    let mut rendered = 0usize;
    let mut result = Ok(());
    'outer: for (epoch, start) in (first..last).step_by(chunk).enumerate() {
        if interrupt::requested() {
            break;
        }
        let end = (start + chunk).min(last);
        let epoch = epoch + 1;
        let frames: Vec<Option<Arc<Vec<u8>>>> = (start..end)
            .into_par_iter()
            .map(|j| {
                (j == first || changed[j]).then(|| {
                    if opts.transparent {
                        Arc::new(straight_rgba(render_frame(scene, canvas, images, j as f64 / scene.fps, epoch)))
                    } else {
                        Arc::new(yuv_frame(scene, canvas, j as f64 / scene.fps, epoch))
                    }
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
    let status = child.wait();
    interrupt::forget(&child);
    let status = status.context("waiting for ffmpeg")?;
    if interrupt::requested() {
        return Err(interrupt::Interrupted.into());
    }
    if !status.success() {
        bail!("ffmpeg failed ({status})");
    }
    result?;
    write_result.context("sending frames to ffmpeg")?;
    Ok(started.elapsed().as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yuv_matches_the_reference() {
        let (w, h) = (64u32, 32u32);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i * 7) as u8, (i * 13) as u8, (i * 29) as u8, 255]).collect();
        let reference = rgba_to_yuv420_reference(&rgba, w as usize, h as usize);
        let out = to_yuv(&rgba, w, h);
        let max = reference.iter().zip(&out).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        assert!(max <= 2, "max difference {max}");
    }

    /// The reused scratch buffer never leaks into a frame
    #[test]
    fn yuv_frames_are_drawn_from_scratch() {
        use crate::config::Color;
        use crate::timeline::{Anim, Mob};
        let mut scene = Scene::new(10.0, 16.0, 9.0, Some(Color::hex("#222222").unwrap()));
        let a = scene.obj(Mob::rect(3.0, 2.0, Color::rgb(0.2, 0.6, 0.9)).move_to(-4.0, 1.0));
        scene.add(&[a]);
        scene.play(vec![Anim::shift_by(a, 6.0, -2.0)]);
        let canvas = Canvas::new(320, 180, 9.0);
        let images = Images::new();
        for j in 0..scene.total_frames() {
            let t = j as f64 / scene.fps;
            SCRATCH.with_borrow_mut(|buf| *buf = vec![0xAB; 320 * 180 * 4]);
            let full = render_frame(&scene, &canvas, &images, t, 1);
            assert_eq!(yuv_frame(&scene, &canvas, t, 1), to_yuv(full.data(), 320, 180), "frame {j}");
        }
    }
}
