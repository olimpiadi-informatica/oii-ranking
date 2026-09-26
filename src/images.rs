//! Raster images (logo, medal, faces, screenshots). They are decoded lazily, the first
//! time a frame needs them, then resized once to the size they are drawn at. Memory is
//! released for the images that are no longer used.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use fast_image_resize as fr;
use resvg::tiny_skia;

use crate::config::Color;

/// Pixel height at which manim's ImageMobject is 8 units (the frame height) tall
const MANIM_RESOLUTION: f64 = 1080.0;

pub enum Source {
    File(PathBuf),
    /// SVG document (for placeholders)
    Svg(String),
    /// Encoded image built into the binary
    Bytes(&'static [u8]),
}

/// A resized copy, computed once (None if the image cannot be read)
type Resized = OnceLock<Option<Arc<tiny_skia::Pixmap>>>;

/// Path (or name of a built-in image) and tint
type ImgKey = (PathBuf, Option<[u32; 3]>);

pub struct Img {
    pub source: Source,
    /// Replaces the color of every pixel, keeping its alpha (manim's ImageMobject.set_color)
    pub tint: Option<Color>,
    pub px_w: u32,
    pub px_h: u32,
    decoded: Mutex<Option<Arc<Vec<u8>>>>,
    scaled: Mutex<HashMap<(u32, u32), Arc<Resized>>>,
    last_used: AtomicUsize,
}

impl Img {
    /// Size in units, as manim's ImageMobject before scaling
    pub fn units(&self) -> (f64, f64) {
        let k = 8.0 / MANIM_RESOLUTION;
        (self.px_w as f64 * k, self.px_h as f64 * k)
    }

    /// The image resized to w x h pixels (premultiplied), or None if it cannot be read
    pub fn at_size(&self, w: u32, h: u32, epoch: usize) -> Option<Arc<tiny_skia::Pixmap>> {
        self.last_used.fetch_max(epoch, Ordering::Relaxed);
        if w == 0 || h == 0 {
            return None;
        }
        // other threads needing the same size wait for the first one instead of redoing it
        let cell = self.scaled.lock().unwrap().entry((w, h)).or_default().clone();
        cell.get_or_init(|| match self.render(w, h) {
            Ok(p) => Some(Arc::new(p)),
            Err(e) => {
                static REPORTED: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
                let msg = format!("{e:#}");
                if REPORTED.get_or_init(Default::default).lock().unwrap().insert(msg.clone()) {
                    eprintln!("!!! {msg}");
                }
                None
            }
        })
        .clone()
    }

    fn render(&self, w: u32, h: u32) -> Result<tiny_skia::Pixmap> {
        let mut data = match &self.source {
            Source::Svg(svg) => {
                let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())?;
                let mut pixmap = tiny_skia::Pixmap::new(w, h).context("empty image")?;
                let size = tree.size();
                let ts = tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
                resvg::render(&tree, ts, &mut pixmap.as_mut());
                pixmap.take()
            }
            Source::File(_) | Source::Bytes(_) => {
                let src = self.decoded()?;
                if (w, h) == (self.px_w, self.px_h) {
                    src.as_ref().clone()
                } else {
                    let src = fr::images::Image::from_vec_u8(
                        self.px_w,
                        self.px_h,
                        src.as_ref().clone(),
                        fr::PixelType::U8x4,
                    )?;
                    let mut dst = fr::images::Image::new(w, h, fr::PixelType::U8x4);
                    let options = fr::ResizeOptions::new()
                        .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::CatmullRom))
                        .use_alpha(false); // already premultiplied
                    fr::Resizer::new().resize(&src, &mut dst, &options)?;
                    let mut data = dst.into_vec();
                    // CatmullRom can overshoot: keep the colors valid for premultiplied alpha
                    for px in data.chunks_exact_mut(4) {
                        let a = px[3];
                        px[0] = px[0].min(a);
                        px[1] = px[1].min(a);
                        px[2] = px[2].min(a);
                    }
                    data
                }
            }
        };
        if let Some(t) = self.tint {
            let rgb = [t.r, t.g, t.b].map(|c| c.clamp(0.0, 1.0));
            for px in data.chunks_exact_mut(4) {
                let a = px[3] as f32;
                for i in 0..3 {
                    px[i] = (rgb[i] * a).round() as u8;
                }
            }
        }
        tiny_skia::Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h).context("empty image")?)
            .context("invalid image size")
    }

    /// Premultiplied RGBA pixels at the original size
    fn decoded(&self) -> Result<Arc<Vec<u8>>> {
        let mut slot = self.decoded.lock().unwrap();
        if let Some(d) = &*slot {
            return Ok(d.clone());
        }
        let img = match &self.source {
            Source::File(path) if is_jxl(path) => {
                let data = Arc::new(decode_jxl(path).with_context(|| format!("decoding {}", path.display()))?);
                *slot = Some(data.clone());
                return Ok(data);
            }
            Source::File(path) => image::ImageReader::open(path)
                .and_then(|r| r.with_guessed_format())
                .with_context(|| format!("reading {}", path.display()))?
                .decode()
                .with_context(|| format!("decoding {}", path.display()))?,
            Source::Bytes(data) => image::load_from_memory(data)?,
            Source::Svg(_) => unreachable!(),
        };
        let mut data = img.to_rgba8().into_raw();
        for px in data.chunks_exact_mut(4) {
            let a = px[3] as u16;
            if a < 255 {
                for c in &mut px[..3] {
                    *c = ((*c as u16 * a + 127) / 255) as u8;
                }
            }
        }
        let data = Arc::new(data);
        *slot = Some(data.clone());
        Ok(data)
    }

    fn release(&self) {
        *self.decoded.lock().unwrap() = None;
        self.scaled.lock().unwrap().clear();
    }
}

fn is_jxl(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("jxl"))
}

/// JPEG XL (with jxl-rs) straight to premultiplied RGBA, without going through `image`
fn decode_jxl(path: &Path) -> Result<Vec<u8>> {
    use jxl::api::{JxlColorType, JxlDataFormat, JxlDecoder, JxlDecoderOptions, JxlOutputBuffer, JxlPixelFormat};
    use jxl::api::{ProcessingResult, states};
    use jxl::headers::extra_channels::ExtraChannel;

    fn complete<T, F>(r: jxl::error::Result<ProcessingResult<T, F>>) -> Result<T> {
        match r.map_err(|e| anyhow::anyhow!("{e}"))? {
            ProcessingResult::Complete { result } => Ok(result),
            ProcessingResult::NeedsMoreInput { .. } => anyhow::bail!("truncated JPEG XL file"),
        }
    }

    let data = std::fs::read(path)?;
    let mut input: &[u8] = &data;
    // the decoder runs on the calling thread: frames are already decoded in parallel
    let mut options = JxlDecoderOptions::default();
    options.premultiply_output = true;
    let mut decoder = complete(JxlDecoder::<states::Initialized>::new(options).process(&mut input, None))?;
    let info = decoder.basic_info().clone();
    let (w, h) = info.size;
    let alpha = info.extra_channels.iter().any(|c| c.ec_type == ExtraChannel::Alpha);
    let gray = decoder.current_pixel_format().color_type.is_grayscale();
    let (color_type, channels) = match (gray, alpha) {
        (true, false) => (JxlColorType::Grayscale, 1),
        (true, true) => (JxlColorType::GrayscaleAlpha, 2),
        (false, false) => (JxlColorType::Rgb, 3),
        (false, true) => (JxlColorType::Rgba, 4),
    };
    decoder
        .set_pixel_format(JxlPixelFormat {
            color_type,
            color_data_format: Some(JxlDataFormat::U8 { bit_depth: 8 }),
            extra_channel_format: vec![None; info.extra_channels.len()],
        })
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let frame = complete(decoder.process(&mut input, None))?;
    let mut buf = vec![0u8; w * h * channels];
    let mut output = JxlOutputBuffer::new(&mut buf, h, w * channels);
    complete(frame.process(&mut input, std::slice::from_mut(&mut output), None))?;
    if channels == 4 {
        return Ok(buf);
    }
    let mut out = vec![255u8; w * h * 4];
    for (px, o) in buf.chunks_exact(channels).zip(out.chunks_exact_mut(4)) {
        match channels {
            1 => o[..3].fill(px[0]),
            2 => o.copy_from_slice(&[px[0], px[0], px[0], px[1]]),
            _ => o[..3].copy_from_slice(px),
        }
    }
    Ok(out)
}

/// All the images of a video, shared between the frames
#[derive(Default)]
pub struct Images {
    by_key: Mutex<HashMap<ImgKey, Arc<Img>>>,
    all: Mutex<Vec<Arc<Img>>>,
}

impl Images {
    pub fn new() -> Images {
        jxl_image_rs_integration::register_image_decoding_hook();
        Images::default()
    }

    /// An image file (only its header is read now)
    pub fn file(&self, path: &Path, tint: Option<Color>) -> Result<Arc<Img>> {
        let key = (path.to_path_buf(), tint.map(|c| [c.r, c.g, c.b].map(f32::to_bits)));
        if let Some(img) = self.by_key.lock().unwrap().get(&key) {
            return Ok(img.clone());
        }
        let (px_w, px_h) = image::ImageReader::open(path)
            .and_then(|r| r.with_guessed_format())
            .with_context(|| format!("reading {}", path.display()))?
            .into_dimensions()
            .with_context(|| format!("reading {}", path.display()))?;
        let img = self.push(Source::File(path.to_path_buf()), tint, px_w, px_h);
        self.by_key.lock().unwrap().insert(key, img.clone());
        Ok(img)
    }

    /// An image file, or the SVG placeholder if the file is missing or unreadable
    pub fn file_or(&self, path: &Path, tint: Option<Color>, placeholder: impl FnOnce() -> Arc<Img>) -> Arc<Img> {
        match self.file(path, tint) {
            Ok(img) => img,
            Err(e) => {
                if path.exists() {
                    eprintln!("!!! {e:#}");
                }
                placeholder()
            }
        }
    }

    /// An image built into the binary (used when a file of the repository is missing)
    pub fn bytes(&self, key: &str, data: &'static [u8]) -> Arc<Img> {
        self.bytes_tinted(key, data, None)
    }

    pub fn bytes_tinted(&self, key: &str, data: &'static [u8], tint: Option<Color>) -> Arc<Img> {
        let key = (PathBuf::from(format!("<built-in {key}>")), tint.map(|c| [c.r, c.g, c.b].map(f32::to_bits)));
        if let Some(img) = self.by_key.lock().unwrap().get(&key) {
            return img.clone();
        }
        let (w, h) = image::ImageReader::new(std::io::Cursor::new(data))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok())
            .expect("built-in image");
        let img = self.push(Source::Bytes(data), tint, w, h);
        self.by_key.lock().unwrap().insert(key, img.clone());
        img
    }

    /// An SVG drawing with a nominal size in pixels (used for the layout, as for files)
    pub fn svg(&self, svg: String, px_w: u32, px_h: u32) -> Arc<Img> {
        self.push(Source::Svg(svg), None, px_w, px_h)
    }

    fn push(&self, source: Source, tint: Option<Color>, px_w: u32, px_h: u32) -> Arc<Img> {
        let img = Arc::new(Img {
            source,
            tint,
            px_w,
            px_h,
            decoded: Mutex::new(None),
            scaled: Mutex::new(HashMap::new()),
            last_used: AtomicUsize::new(0),
        });
        self.all.lock().unwrap().push(img.clone());
        img
    }

    /// Frees the pixels of the images not used since `epoch`
    pub fn release_unused(&self, epoch: usize) {
        for img in self.all.lock().unwrap().iter() {
            if img.last_used.load(Ordering::Relaxed) < epoch {
                img.release();
            }
        }
    }
}

/// Placeholder for a missing face: a grey silhouette
pub fn no_face_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000" viewBox="0 0 100 100">
<rect width="100" height="100" fill="#3a3a3a"/>
<circle cx="50" cy="38" r="19" fill="#8a8a8a"/>
<path d="M12 100 C12 72 30 62 50 62 C70 62 88 72 88 100 Z" fill="#8a8a8a"/>
</svg>"##
        .to_string()
}

/// Placeholder for missing screenshots
pub fn no_screen_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="960" height="540" viewBox="0 0 960 540">
<rect width="960" height="540" fill="#1b2a3a"/>
<g stroke="#3f6a8f" stroke-width="6" fill="none" stroke-linecap="round">
<path d="M0 120 H220 L280 180 H520"/><path d="M960 400 H700 L640 340 H420"/>
<path d="M120 540 V430 L180 370 H330"/><path d="M820 0 V110 L760 170 H600"/>
<path d="M0 300 H140 L200 240 H300"/><path d="M960 220 H820 L780 260"/>
</g>
<g fill="#5b8fbf"><circle cx="520" cy="180" r="12"/><circle cx="420" cy="340" r="12"/>
<circle cx="330" cy="370" r="12"/><circle cx="600" cy="170" r="12"/><circle cx="300" cy="240" r="12"/>
<circle cx="780" cy="260" r="12"/></g>
<rect x="380" y="200" width="200" height="140" rx="10" fill="#27415a" stroke="#5b8fbf" stroke-width="6"/>
</svg>"##
        .to_string()
}

#[cfg(test)]
mod tests {
    /// Decodes the file in $JXL_FILE to $JXL_OUT (raw RGBA), to compare with another decoder
    #[test]
    #[ignore]
    fn decode_jxl_to_raw() {
        let path = std::env::var("JXL_FILE").unwrap();
        let data = super::decode_jxl(std::path::Path::new(&path)).unwrap();
        std::fs::write(std::env::var("JXL_OUT").unwrap(), data).unwrap();
    }

    /// Decoding speed of the first 64 .jxl files in $JXL_DIR, in parallel as when rendering
    #[test]
    #[ignore]
    fn decode_jxl_speed() {
        use rayon::prelude::*;
        let mut files: Vec<_> = std::fs::read_dir(std::env::var("JXL_DIR").unwrap())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| super::is_jxl(p))
            .collect();
        files.sort();
        files.truncate(64);
        let t = std::time::Instant::now();
        files.par_iter().for_each(|f| drop(super::decode_jxl(f).unwrap()));
        println!("{:.1} ms/image", t.elapsed().as_secs_f64() * 1000.0 / files.len() as f64);
    }
}
