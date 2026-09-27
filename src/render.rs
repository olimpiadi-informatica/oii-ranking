//! Rasterization of frames and the video pipeline: frames are rendered in parallel,
//! unchanged frames are reused, and everything is streamed to ffmpeg in order.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use kurbo::Affine;
use rayon::prelude::*;
use resvg::tiny_skia;
use yuvutils_rs::{BufferStoreMut, YuvConversionMode, YuvPlanarImageMut, YuvRange, YuvStandardMatrix};

use crate::encoder::Encoder;
use crate::images::Images;
use crate::timeline::{Drawn, Scene, Segments, Vis};
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

    fn full(&self) -> PxRect {
        PxRect { x0: 0, y0: 0, x1: self.w as i32, y1: self.h as i32 }
    }

    /// Pixels that `d` can touch, with even corners (for the chroma of YUV 4:2:0), or None if
    /// it is out of the frame
    fn bounds(&self, d: &Drawn) -> Option<PxRect> {
        let m = &d.mob;
        let (w, h) = m.size0();
        let k = m.s * d.scale * self.ppu;
        let (mut hw, mut hh) = (w * k / 2.0, h * k / 2.0);
        if m.angle != 0.0 {
            hw = hw.hypot(hh);
            hh = hw;
        }
        // strokes (at most 0.04 units wide), antialiasing and the rounding of images
        let pad = 0.05 * self.ppu + 2.0;
        let (cx, cy) = self.px(m.x, m.y);
        let mut r = PxRect {
            x0: ((cx - hw - pad).floor().max(0.0) as i32) & !1,
            y0: ((cy - hh - pad).floor().max(0.0) as i32) & !1,
            x1: (((cx + hw + pad).ceil().min(self.w as f64) as i32 + 1) & !1).min(self.w as i32),
            y1: (((cy + hh + pad).ceil().min(self.h as f64) as i32 + 1) & !1).min(self.h as i32),
        };
        // a region is converted as whole rows of the frame (see `to_yuv`): on the last row, it
        // must start at the left edge
        if r.y1 == self.h as i32 {
            r.x0 = 0;
        }
        (r.x0 < r.x1 && r.y0 < r.y1).then_some(r)
    }
}

/// A rectangle of pixels, [x0, x1) x [y0, y1)
#[derive(Clone, Copy, Debug, PartialEq)]
struct PxRect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl PxRect {
    fn w(&self) -> u32 {
        (self.x1 - self.x0) as u32
    }
    fn h(&self) -> u32 {
        (self.y1 - self.y0) as u32
    }
    fn area(&self) -> i64 {
        self.w() as i64 * self.h() as i64
    }
    fn intersects(&self, o: &PxRect) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.y0 < o.y1 && o.y0 < self.y1
    }
    fn union(&self, o: &PxRect) -> PxRect {
        PxRect { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }
}

/// Merges the rectangles until none overlap; close ones are merged too, when that adds
/// little area (fewer, bigger regions are cheaper to draw)
fn merge(mut rects: Vec<PxRect>) -> Vec<PxRect> {
    'again: loop {
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                let (a, b) = (rects[i], rects[j]);
                let u = a.union(&b);
                if a.intersects(&b) || u.area() * 4 <= (a.area() + b.area()) * 5 {
                    rects[i] = u;
                    rects.swap_remove(j);
                    continue 'again;
                }
            }
        }
        return rects;
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

/// Converts region r of `rgba` (a whole opaque frame) into the same region of `frame`, in
/// planar YUV 4:2:0 (BT.709, limited range)
fn to_yuv(rgba: &[u8], r: PxRect, frame: &mut [u8], fw: usize, fh: usize) {
    let (cw, ch) = (fw.div_ceil(2), fh.div_ceil(2));
    let (y_plane, uv) = frame.split_at_mut(fw * fh);
    let (u_plane, v_plane) = uv.split_at_mut(cw * ch);
    let (x0, y0, rows) = (r.x0 as usize, r.y0 as usize, r.h() as usize);
    let (c0, cr0, crows) = (x0 / 2, y0 / 2, rows.div_ceil(2));
    // yuvutils converts every row of the slices it gets, whatever the height: they must hold
    // exactly the rows of the region (whole rows of the frame, hence x0 = 0 on the last row)
    let mut planes = YuvPlanarImageMut {
        y_plane: BufferStoreMut::Borrowed(&mut y_plane[y0 * fw + x0..(y0 + rows) * fw + x0]),
        y_stride: fw as u32,
        u_plane: BufferStoreMut::Borrowed(&mut u_plane[cr0 * cw + c0..(cr0 + crows) * cw + c0]),
        u_stride: cw as u32,
        v_plane: BufferStoreMut::Borrowed(&mut v_plane[cr0 * cw + c0..(cr0 + crows) * cw + c0]),
        v_stride: cw as u32,
        width: r.w(),
        height: r.h(),
    };
    yuvutils_rs::rgba_to_yuv420(
        &mut planes,
        &rgba[(y0 * fw + x0) * 4..((y0 + rows) * fw + x0) * 4],
        fw as u32 * 4,
        YuvRange::Limited,
        YuvStandardMatrix::Bt709,
        YuvConversionMode::Balanced,
    )
    .expect("YUV conversion");
}

fn yuv_size(w: u32, h: u32) -> usize {
    let (w, h) = (w as usize, h as usize);
    w * h + 2 * w.div_ceil(2) * h.div_ceil(2)
}

thread_local! {
    /// The frame being drawn: never cleared as a whole, only the regions drawn again are
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Renders frames in YUV 4:2:0, drawing as little as possible. Within a segment of the
/// timeline (see [`Segments`]) most objects do not change: they are drawn (and converted)
/// once into a static frame. A frame is a copy of it, where only the regions around the
/// dynamic objects are drawn again, with everything in them, and converted.
///
/// The regions are drawn at their place in a frame-sized pixmap, not in a pixmap of their
/// size: tiny-skia's antialiasing depends a little on the position of the paths, and this
/// way the pixels are exactly those of the whole frame drawn at once.
/// The static frame of a segment, in YUV, drawn by the first thread that needs it
type StaticFrame = OnceLock<Arc<Vec<u8>>>;

struct YuvFrames<'a> {
    scene: &'a Scene,
    canvas: &'a Canvas,
    segments: Segments,
    statics: Mutex<HashMap<usize, Arc<StaticFrame>>>,
}

impl<'a> YuvFrames<'a> {
    fn new(scene: &'a Scene, canvas: &'a Canvas) -> YuvFrames<'a> {
        YuvFrames { scene, canvas, segments: scene.segments(), statics: Mutex::new(HashMap::new()) }
    }

    fn frame(&self, j: usize, epoch: usize) -> Arc<Vec<u8>> {
        let (scene, canvas) = (self.scene, self.canvas);
        let t = j as f64 / scene.fps;
        let seg = self.segments.index(t);
        let drawn = scene.frame_in_segment(t, self.segments.mid(seg));
        // objects in the frame, with the pixels they can touch
        let placed: Vec<(PxRect, &Drawn)> = drawn.iter().filter_map(|d| Some((canvas.bounds(d)?, d))).collect();
        let dirty = merge(placed.iter().filter(|(_, d)| d.dynamic).map(|(r, _)| *r).collect());
        let full = canvas.full();
        let dirty_area: i64 = dirty.iter().map(PxRect::area).sum();

        // a short segment, or a frame that changes almost everywhere: draw it all
        if self.segments.frames(seg, scene.fps) < 3 || dirty_area * 10 > full.area() * 7 {
            let mut out = vec![0u8; yuv_size(canvas.w, canvas.h)];
            self.draw_region(full, &placed, epoch, |rgba| {
                to_yuv(rgba, full, &mut out, canvas.w as usize, canvas.h as usize)
            });
            return Arc::new(out);
        }

        let cell = self.statics.lock().unwrap().entry(seg).or_default().clone();
        let base = cell
            .get_or_init(|| {
                let still: Vec<_> = placed.iter().filter(|(_, d)| !d.dynamic).copied().collect();
                let mut out = vec![0u8; yuv_size(canvas.w, canvas.h)];
                self.draw_region(full, &still, epoch, |rgba| {
                    to_yuv(rgba, full, &mut out, canvas.w as usize, canvas.h as usize)
                });
                Arc::new(out)
            })
            .clone();
        if dirty.is_empty() {
            return base;
        }
        let mut out = base.as_ref().clone();
        for r in dirty {
            self.draw_region(r, &placed, epoch, |rgba| to_yuv(rgba, r, &mut out, canvas.w as usize, canvas.h as usize));
        }
        Arc::new(out)
    }

    /// Draws region r of the frame (the objects in `placed` that touch it) and passes the
    /// whole frame to `f`, where only region r is valid
    fn draw_region(&self, r: PxRect, placed: &[(PxRect, &Drawn)], epoch: usize, f: impl FnOnce(&[u8])) {
        SCRATCH.with_borrow_mut(|buf| {
            let (w, h) = (self.canvas.w, self.canvas.h);
            buf.resize(w as usize * h as usize * 4, 0);
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(buf, w, h).expect("frame size");
            let bg = self.scene.background.expect("YUV frames are opaque");
            let rect = tiny_skia::Rect::from_ltrb(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32).unwrap();
            let mut paint = vector::paint(bg, 1.0);
            paint.blend_mode = tiny_skia::BlendMode::Source;
            paint.anti_alias = false;
            pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
            // what the objects draw out of r is left there: it is not read
            for (b, d) in placed {
                if b.intersects(&r) {
                    draw(&mut pixmap, self.canvas, d, epoch);
                }
            }
            f(buf);
        })
    }

    /// Frees the static frames of the segments before time t
    fn release_before(&self, t: f64) {
        let seg = self.segments.index(t);
        self.statics.lock().unwrap().retain(|&s, _| s >= seg);
    }
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
    let mut child = cmd.spawn().context("starting ffmpeg (is it installed?)")?;
    let mut stdin = child.stdin.take().unwrap();

    let changed = scene.changed_frames();
    let yuv = YuvFrames::new(scene, canvas);
    let threads = rayon::current_num_threads();
    // frames rendered together: enough to keep every thread busy, within about 1 GB
    let frame_bytes = canvas.w as usize * canvas.h as usize * 4;
    let chunk = ((1 << 30) / frame_bytes).clamp(threads, 2 * threads);
    // room for two chunks: the next chunk is rendered while ffmpeg encodes the last one
    let (tx, rx) = sync_channel::<Arc<Vec<u8>>>(2 * chunk);
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for frame in rx {
            stdin.write_all(&frame)?;
        }
        Ok(())
    });

    let started = Instant::now();
    let mut previous: Option<Arc<Vec<u8>>> = None;
    let mut rendered = 0usize;
    let mut result = Ok(());
    'outer: for (epoch, start) in (first..last).step_by(chunk).enumerate() {
        let end = (start + chunk).min(last);
        let epoch = epoch + 1;
        let frames: Vec<Option<Arc<Vec<u8>>>> = (start..end)
            .into_par_iter()
            .map(|j| {
                (j == first || changed[j]).then(|| {
                    if opts.transparent {
                        Arc::new(straight_rgba(render_frame(scene, canvas, images, j as f64 / scene.fps, epoch)))
                    } else {
                        yuv.frame(j, epoch)
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
        yuv.release_before(end as f64 / scene.fps);
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
    use crate::config::Color;
    use crate::timeline::{Anim, DOWN, Mob, RIGHT};

    #[test]
    fn yuv_matches_the_reference() {
        let (w, h) = (64u32, 32u32);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i * 7) as u8, (i * 13) as u8, (i * 29) as u8, 255]).collect();
        let reference = rgba_to_yuv420_reference(&rgba, w as usize, h as usize);
        let mut out = vec![0u8; yuv_size(w, h)];
        let full = PxRect { x0: 0, y0: 0, x1: w as i32, y1: h as i32 };
        to_yuv(&rgba, full, &mut out, w as usize, h as usize);
        let max = reference.iter().zip(&out).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        assert!(max <= 2, "max difference {max}");
    }

    #[test]
    fn rects_merge_until_disjoint() {
        let r = |x0, y0, x1, y1| PxRect { x0, y0, x1, y1 };
        let m = merge(vec![r(0, 0, 10, 10), r(100, 100, 110, 110), r(8, 8, 20, 20), r(18, 0, 30, 4)]);
        assert_eq!(m.len(), 2);
        for (i, a) in m.iter().enumerate() {
            for b in &m[i + 1..] {
                assert!(!a.intersects(b));
            }
        }
    }

    /// Every frame drawn incrementally equals the frame drawn from scratch
    #[test]
    fn incremental_frames_are_exact() {
        let bg = Color::hex("#222222").unwrap();
        let mut scene = Scene::new(30.0, 16.0, 9.0, Some(bg));
        let fixed = scene.obj(Mob::rect(3.0, 2.0, Color::rgb(0.2, 0.6, 0.9)).move_to(-4.0, 1.0));
        let below = scene.obj(Mob::rect(1.0, 1.0, Color::rgb(1.0, 0.2, 0.2)).move_to(-6.0, 1.0));
        let above = scene.obj(Mob::rect(1.5, 4.0, Color::rgb(0.9, 0.9, 0.1)).move_to(-3.0, 0.0));
        let circle = scene.obj(Mob::shape(Arc::new(vector::Shape::circle(0.7, Color::WHITE, 0.04))).move_to(4.0, -2.0));
        scene.add(&[fixed, below, above]);
        scene.wait(0.5);
        // moves under `above`, while the circle is drawn
        scene.play(vec![Anim::shift_by(below, 4.3, -0.4).run_time(2.0), Anim::create(circle)]);
        let spin = scene.func(Arc::new(|t: f64| {
            let mut m = Mob::rect(0.6, 0.6, Color::rgb(0.3, 1.0, 0.3)).move_to(5.0 * (t - 3.0) - 4.0, 2.0);
            m.angle = t * 3.0;
            m.squash = (t * 5.0).cos();
            Some(m)
        }));
        scene.add(&[spin]);
        scene.play(vec![Anim::fade_out(fixed).shift(DOWN), Anim::fade_in(circle).shift(RIGHT).run_time(0.1)]);
        scene.wait(1.0);

        let canvas = Canvas::new(320, 180, 9.0);
        let images = Images::new();
        let frames = YuvFrames::new(&scene, &canvas);

        let (mut partial, mut differ) = (0, 0);
        for j in 0..scene.total_frames() {
            let t = j as f64 / scene.fps;
            let full = render_frame(&scene, &canvas, &images, t, 1);
            let mut expected = vec![0u8; yuv_size(canvas.w, canvas.h)];
            to_yuv(full.data(), canvas.full(), &mut expected, 320, 180);
            // anything read from outside the regions drawn would show
            SCRATCH.with_borrow_mut(|buf| *buf = vec![0xAB; 320 * 180 * 4]);
            let got = frames.frame(j, 1);
            let max = expected.iter().zip(got.iter()).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            differ += (max > 0) as usize;
            assert!(max <= 1, "frame {j}: difference {max}");
            let seg = frames.segments.index(t);
            let drawn = scene.frame_in_segment(t, frames.segments.mid(seg));
            partial += drawn.iter().any(|d| d.dynamic) as usize;
        }
        assert!(partial > 30, "only {partial} frames used the static frames");
        assert!(differ * 20 <= scene.total_frames(), "{differ} frames differ by rounding");
    }

    /// On the real data in $DATA_DIR (gold, $WIDE=1 for the 64:9 layout, 1080p): windows of
    /// frames drawn from scratch and incrementally, compared, with the time of both. Nothing is
    /// encoded or written.
    #[test]
    #[ignore]
    fn real_frames_incremental_vs_full() {
        use crate::config::{Config, Medal};
        std::env::set_current_dir(std::env::var("DATA_DIR").unwrap()).unwrap();
        let wide = std::env::var("WIDE").is_ok();
        let config = Config::load(Some(Path::new("config.toml"))).unwrap();
        let config = if wide { config.widened() } else { config };
        let data = crate::data::Data::load(&config).unwrap();
        let fonts = crate::text::Fonts::load(&config).unwrap();
        let images = Images::new();
        let ctx = crate::scenes::Ctx::new(&config, &data, &fonts, &images, Medal::Gold, &[]).unwrap();
        let (w, h) = if wide { (7680, 1080) } else { (1920, 1080) };
        let aspect = w as f64 / h as f64;
        let scene = if wide {
            crate::scenes::wide::build(&ctx, 60.0, aspect)
        } else {
            crate::scenes::standard::build(&ctx, 60.0, aspect)
        };
        let canvas = Canvas::new(w, h, scene.fh);
        let frames = YuvFrames::new(&scene, &canvas);
        let total = scene.total_frames();
        // 12 windows of 30 frames spread over the video
        let js: Vec<usize> = (0..12).flat_map(|k| (0..30).map(move |i| k * total / 12 + i)).collect();
        for &j in &js[..30] {
            render_frame(&scene, &canvas, &images, j as f64 / 60.0, 1); // load the images
        }
        let t0 = Instant::now();
        let expected: Vec<Vec<u8>> = js
            .par_iter()
            .map(|&j| {
                let full = render_frame(&scene, &canvas, &images, j as f64 / 60.0, 1);
                let mut out = vec![0u8; yuv_size(w, h)];
                to_yuv(full.data(), canvas.full(), &mut out, w as usize, h as usize);
                out
            })
            .collect();
        let full_time = t0.elapsed().as_secs_f64();
        let t0 = Instant::now();
        let got: Vec<Arc<Vec<u8>>> = js.par_iter().map(|&j| frames.frame(j, 1)).collect();
        let incremental_time = t0.elapsed().as_secs_f64();
        let mut worst = 0;
        for (j, (a, b)) in js.iter().zip(expected.iter().zip(&got)) {
            let max = a.iter().zip(b.iter()).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
            let n = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
            if max > 1 {
                println!("frame {j}: {n} bytes differ, by up to {max}");
            }
            worst = worst.max(max);
        }
        println!(
            "{} frames: from scratch {:.1} ms/frame, incremental {:.1} ms/frame; worst difference {worst}",
            js.len(),
            full_time * 1000.0 / js.len() as f64,
            incremental_time * 1000.0 / js.len() as f64
        );
        assert!(worst <= 1);
    }
}
