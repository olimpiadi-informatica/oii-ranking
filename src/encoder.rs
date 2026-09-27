//! The video encoder: a GPU when one can encode the video (NVIDIA through NVENC, AMD and
//! Intel through VAAPI), else the CPU encoder of the settings. Whether a GPU works is found
//! out by encoding a few frames with it: missing drivers, drivers without the codecs (e.g.
//! Fedora's Mesa), frames too big for the hardware and missing devices all show up there.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::config::Config;

#[derive(Clone, Debug)]
pub struct Encoder {
    /// For the messages, e.g. "hevc_vaapi on /dev/dri/renderD128"
    pub name: String,
    /// ffmpeg options before the input (the hardware device)
    pub global: Vec<String>,
    /// ffmpeg options of the output video
    pub output: Vec<String>,
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

impl Encoder {
    /// The CPU encoder of the settings
    pub fn software(config: &Config) -> Encoder {
        let mut output = strings(&["-c:v", &config.video.encoder]);
        output.extend(config.video.encoder_options.iter().cloned());
        output.extend(strings(&["-pix_fmt", "yuv420p"]));
        Encoder { name: config.video.encoder.clone(), global: vec![], output }
    }

    /// Transparent video (for the overlays)
    pub fn prores() -> Encoder {
        let output = strings(&["-c:v", "prores_ks", "-profile:v", "4444", "-pix_fmt", "yuva444p10le"]);
        Encoder { name: "prores_ks".into(), global: vec![], output }
    }

    /// `codec` (h264_vaapi or hevc_vaapi) on a GPU, for w x h frames
    fn vaapi(device: &str, codec: &str, qp: u32, w: u32, h: u32) -> Encoder {
        // Hardware encoders code whole blocks of 16 pixels, and some drivers (AMD's for HEVC)
        // then declare the padded size, e.g. 1088 rows instead of 1080, which shows up as a
        // green line. So the frames are padded here, and the extra pixels are marked as
        // cropped in the stream headers.
        let (pw, ph) = (w.div_ceil(16) * 16, h.div_ceil(16) * 16);
        let mut filters = String::new();
        if (pw, ph) != (w, h) {
            filters = format!("pad={pw}:{ph}:0:0,");
        }
        filters += "format=nv12,hwupload";
        let mut output = strings(&["-vf", &filters, "-c:v", codec, "-rc_mode", "CQP", "-qp", &qp.to_string()]);
        if (pw, ph) != (w, h) {
            let base = codec.trim_end_matches("_vaapi");
            output
                .extend(strings(&["-bsf:v", &format!("{base}_metadata=crop_right={}:crop_bottom={}", pw - w, ph - h)]));
        }
        if codec.starts_with("hevc") {
            // the tag QuickTime and Apple devices need to play HEVC in mp4
            output.extend(strings(&["-tag:v", "hvc1"]));
        }
        Encoder { name: format!("{codec} on {device}"), global: strings(&["-vaapi_device", device]), output }
    }

    /// `codec` (h264_nvenc or hevc_nvenc) on the NVIDIA GPU of index `gpu`
    fn nvenc(gpu: usize, codec: &str, quality: u32) -> Encoder {
        let q = quality.to_string();
        let mut output = strings(&["-c:v", codec, "-gpu", &gpu.to_string(), "-preset", "p5"]);
        // constant quality: "-cq" with a variable bitrate and no bitrate target
        output.extend(strings(&["-rc", "vbr", "-cq", &q, "-b:v", "0", "-pix_fmt", "yuv420p"]));
        if codec.starts_with("hevc") {
            output.extend(strings(&["-tag:v", "hvc1"]));
        }
        Encoder { name: format!("{codec} on NVIDIA GPU {gpu}"), global: vec![], output }
    }
}

/// A GPU to try
#[derive(Clone, Debug, PartialEq)]
enum Gpu {
    /// NVIDIA, by index (as in /dev/nvidia0)
    Nvidia(usize),
    /// A VAAPI device, e.g. /dev/dri/renderD128
    Vaapi(String),
}

impl Gpu {
    /// The encoders to test on this GPU, best first
    fn encoders(&self, quality: u32, w: u32, h: u32) -> Vec<Encoder> {
        codecs(w, h)
            .iter()
            .map(|codec| match self {
                Gpu::Nvidia(i) => Encoder::nvenc(*i, &format!("{codec}_nvenc"), quality),
                Gpu::Vaapi(device) => Encoder::vaapi(device, &format!("{codec}_vaapi"), quality, w, h),
            })
            .collect()
    }
}

/// The GPUs to try, from `video.hardware` and the devices in `dev` (normally /dev): "auto"
/// (all of them, NVIDIA first: usually the faster), "off" (none) or a device, e.g.
/// "/dev/nvidia0" or "/dev/dri/renderD128"
fn gpus(setting: &str, dev: &Path) -> Vec<Gpu> {
    let nvidia_index = |name: &str| name.strip_prefix("nvidia").and_then(|n| n.parse::<usize>().ok());
    match setting.trim() {
        "off" | "" => vec![],
        "auto" => {
            let names: Vec<String> = std::fs::read_dir(dev)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
                .collect();
            let mut nvidia: Vec<usize> = names.iter().filter_map(|n| nvidia_index(n)).collect();
            nvidia.sort();
            let mut vaapi: Vec<PathBuf> = std::fs::read_dir(dev.join("dri"))
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("renderD")))
                .collect();
            vaapi.sort();
            let vaapi = vaapi.into_iter().map(|p| Gpu::Vaapi(p.to_string_lossy().into_owned()));
            nvidia.into_iter().map(Gpu::Nvidia).chain(vaapi).collect()
        }
        device => match Path::new(device).file_name().and_then(|n| nvidia_index(&n.to_string_lossy())) {
            Some(i) => vec![Gpu::Nvidia(i)],
            None => vec![Gpu::Vaapi(device.to_string())],
        },
    }
}

/// H.264 plays everywhere, but hardware encoders stop at 4096 pixels: HEVC goes further
fn codecs(w: u32, h: u32) -> &'static [&'static str] {
    if w <= 4096 && h <= 4096 { &["h264", "hevc"] } else { &["hevc"] }
}

/// Encodes a few black frames of the video's size, and checks that they decode at that size:
/// Ok if the GPU can do it, else the reason
fn probe(encoder: &Encoder, w: u32, h: u32, fps: f64) -> Result<(), String> {
    static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = std::env::temp_dir().join(format!("oii-ranking-probe-{}-{n}.mkv", std::process::id()));
    let result = encode_test(encoder, w, h, fps, &file).and_then(|()| check_size(&file, w, h));
    let _ = std::fs::remove_file(&file);
    result
}

/// Whether `file` decodes as w x h frames (if this ffmpeg cannot decode it, it is assumed so)
fn check_size(file: &std::path::Path, w: u32, h: u32) -> Result<(), String> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(file)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    let got = out.stdout.len();
    if !out.status.success() || got == 0 || got == (w * h) as usize {
        return Ok(());
    }
    let size = if got % w as usize == 0 { format!("{w}x{}", got / w as usize) } else { format!("{got} pixels") };
    Err(format!("its video decodes at {size} instead of {w}x{h}"))
}

fn encode_test(encoder: &Encoder, w: u32, h: u32, fps: f64, file: &std::path::Path) -> Result<(), String> {
    let mut cmd = Command::new("ffmpeg");
    // at the verbose level ffmpeg says why the driver refuses (e.g. a profile it lacks)
    cmd.args(["-hide_banner", "-v", "verbose"])
        .args(&encoder.global)
        .args(["-f", "lavfi", "-i", &format!("color=black:s={w}x{h}:r={fps}"), "-frames:v", "3"])
        .args(&encoder.output)
        .args(["-y", "-f", "matroska"])
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    let mut stderr = child.stderr.take().unwrap();
    let log = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    // a broken driver can hang: give up after a while
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > Duration::from_secs(20) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let log = log.join().unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    Err(reason(&log))
}

/// The line of ffmpeg's log that best explains a failure, without its "[name @ 0x...]" prefix
fn reason(log: &str) -> String {
    let telling = [
        "not supported",
        "does not support",
        "Cannot load",
        "No capable devices",
        "No such file",
        "Failed to",
        "failed",
        "Error while opening",
    ];
    let line = telling
        .iter()
        .find_map(|t| log.lines().find(|l| l.contains(t)))
        .or_else(|| log.lines().rfind(|l| !l.trim().is_empty()))
        .unwrap_or("failed");
    let line = match (line.starts_with('['), line.find("] ")) {
        (true, Some(i)) => &line[i + 2..],
        _ => line,
    };
    line.trim().to_string()
}

/// The encoders chosen for each size and frame rate, or why there are none
type Chosen = HashMap<(u32, u32, u64), Result<Vec<Encoder>, String>>;

/// The encoders this ffmpeg has (None if ffmpeg cannot be run)
fn available() -> Option<&'static HashSet<String>> {
    static ENCODERS: OnceLock<Option<HashSet<String>>> = OnceLock::new();
    ENCODERS
        .get_or_init(|| {
            let out = Command::new("ffmpeg").args(["-hide_banner", "-encoders"]).stderr(Stdio::null()).output().ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            // " V....D libx264   libx264 H.264 ..."
            Some(text.lines().filter_map(|l| l.split_whitespace().nth(1)).map(str::to_string).collect())
        })
        .as_ref()
}

/// A CPU encoder that comes with every ffmpeg build, even Fedora's (which has no libx264):
/// for when the one of the settings is missing
fn openh264(w: u32, h: u32, fps: f64) -> Encoder {
    // OpenH264 has no constant quality mode: about 0.15 bits per pixel is plenty for these videos
    let bitrate = (w as f64 * h as f64 * fps * 0.15) as u64;
    let output = strings(&["-c:v", "libopenh264", "-b:v", &bitrate.to_string(), "-pix_fmt", "yuv420p"]);
    Encoder { name: "libopenh264".into(), global: vec![], output }
}

/// The GPU encoder that passed the test, or why none did
fn hardware(config: &Config, w: u32, h: u32, fps: f64) -> Result<Encoder, String> {
    let setting = &config.video.hardware;
    let gpus = gpus(setting, Path::new("/dev"));
    if gpus.is_empty() {
        return Err(if setting.trim() == "off" { "GPU encoding is off in the settings".into() } else { "no GPU found".into() });
    }
    let mut failures = vec![];
    for gpu in &gpus {
        for encoder in gpu.encoders(config.video.hardware_qp, w, h) {
            match probe(&encoder, w, h, fps) {
                Ok(()) => return Ok(encoder),
                Err(e) => failures.push(format!("{}: {e}", encoder.name)),
            }
        }
    }
    Err(format!("no GPU can encode {w}x{h}; see \"Hardware encoding\" in the README\n  {}", failures.join("\n  ")))
}

/// The encoders to use for a w x h video, best first: a GPU that passed the test (if any),
/// then a CPU encoder, also used if the GPU fails during the render. The choice is made once
/// per size and frame rate, and printed.
pub fn choose(config: &Config, w: u32, h: u32, fps: f64) -> Result<Vec<Encoder>> {
    static CHOSEN: Mutex<Option<Chosen>> = Mutex::new(None);
    let mut chosen = CHOSEN.lock().unwrap();
    let entry = chosen.get_or_insert_with(HashMap::new).entry((w, h, fps.to_bits()));
    let encoders = entry.or_insert_with(|| {
        let Some(available) = available() else { return Err("cannot run ffmpeg: is it installed?".into()) };
        let gpu = hardware(config, w, h, fps);
        let mut notes = vec![];
        let mut cpu = vec![];
        if available.contains(&config.video.encoder) {
            cpu.push(Encoder::software(config));
        } else {
            notes.push(format!(
                "this ffmpeg has no {} encoder (on Fedora it comes with RPM Fusion's ffmpeg)",
                config.video.encoder
            ));
            if available.contains("libopenh264") && w <= 4096 && h <= 4096 {
                cpu.push(openh264(w, h, fps));
            }
        }
        let encoders: Vec<Encoder> = gpu.iter().cloned().chain(cpu).collect();
        let Some(first) = encoders.first() else {
            return Err(format!(
                "no encoder can make a {w}x{h} video: {}; {}",
                notes.join("; "),
                gpu.err().unwrap_or_default()
            ));
        };
        match &gpu {
            Ok(_) => println!("Encoder: {} (GPU)", first.name),
            Err(why) => println!("Encoder: {} (CPU): {why}", first.name),
        }
        for note in &notes {
            println!("  note: {note}");
        }
        Ok(encoders)
    });
    encoders.clone().map_err(|e| anyhow::anyhow!(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_videos_need_hevc() {
        let names = |gpu: Gpu, w| gpu.encoders(20, w, 1080).into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(names(Gpu::Nvidia(0), 1920), ["h264_nvenc on NVIDIA GPU 0", "hevc_nvenc on NVIDIA GPU 0"]);
        assert_eq!(names(Gpu::Vaapi("/dev/dri/renderD128".into()), 7680), ["hevc_vaapi on /dev/dri/renderD128"]);
    }

    #[test]
    fn gpus_from_the_setting() {
        let dev = Path::new("/nonexistent");
        assert!(gpus("off", dev).is_empty());
        assert!(gpus("auto", dev).is_empty());
        assert_eq!(gpus("/dev/dri/renderD129", dev), [Gpu::Vaapi("/dev/dri/renderD129".into())]);
        assert_eq!(gpus("/dev/nvidia1", dev), [Gpu::Nvidia(1)]);
    }

    #[test]
    fn auto_finds_nvidia_first() {
        let dev = std::env::temp_dir().join(format!("oii-ranking-dev-{}", std::process::id()));
        std::fs::create_dir_all(dev.join("dri")).unwrap();
        for f in ["nvidia1", "nvidia0", "nvidiactl", "nvidia-uvm", "dri/renderD128", "dri/card0"] {
            std::fs::write(dev.join(f), "").unwrap();
        }
        let found = gpus("auto", &dev);
        std::fs::remove_dir_all(&dev).unwrap();
        let render = dev.join("dri/renderD128").to_string_lossy().into_owned();
        assert_eq!(found, [Gpu::Nvidia(0), Gpu::Nvidia(1), Gpu::Vaapi(render)]);
    }

    #[test]
    fn nvenc_uses_constant_quality() {
        let args = Encoder::nvenc(1, "hevc_nvenc", 20).output.join(" ");
        assert!(args.contains("-c:v hevc_nvenc -gpu 1") && args.contains("-rc vbr -cq 20 -b:v 0"), "{args}");
    }

    #[test]
    fn reasons_are_readable() {
        let log = "[h264_vaapi @ 0x55] Compatible profile VAProfileH264High (7) is not supported by driver.\n\
                   [vost#0:0/h264_vaapi @ 0x55] Error while opening encoder - maybe incorrect parameters";
        assert_eq!(reason(log), "Compatible profile VAProfileH264High (7) is not supported by driver.");
        let log = "[vost#0:0/h264_nvenc @ 0x56] Starting thread...\n[h264_nvenc @ 0x56] Cannot load libcuda.so.1\n\
                   [vost#0:0/h264_nvenc @ 0x56] Error while opening encoder";
        assert_eq!(reason(log), "Cannot load libcuda.so.1");
    }

    #[test]
    fn frames_are_padded_to_blocks_and_cropped() {
        let e = Encoder::vaapi("/dev/dri/renderD128", "hevc_vaapi", 20, 7680, 1080);
        let args = e.output.join(" ");
        assert!(args.contains("pad=7680:1088:0:0,format=nv12,hwupload"), "{args}");
        assert!(args.contains("hevc_metadata=crop_right=0:crop_bottom=8"), "{args}");
        let e = Encoder::vaapi("/dev/dri/renderD128", "h264_vaapi", 20, 1920, 1088);
        assert!(!e.output.join(" ").contains("pad="));
    }

    #[test]
    fn missing_device_fails_the_probe() {
        let encoder = Encoder::vaapi("/dev/dri/does-not-exist", "h264_vaapi", 20, 320, 240);
        assert!(probe(&encoder, 320, 240, 30.0).is_err());
    }

    /// Needs a VAAPI GPU: without the padding, AMD's HEVC encoder makes 1088 rows of 1080
    #[test]
    #[ignore]
    fn probe_rejects_a_wrong_size() {
        let mut e = Encoder::vaapi("/dev/dri/renderD128", "hevc_vaapi", 20, 7680, 1080);
        assert_eq!(probe(&e, 7680, 1080, 60.0), Ok(()));
        e.output = strings(&["-vf", "format=nv12,hwupload", "-c:v", "hevc_vaapi"]);
        let err = probe(&e, 7680, 1080, 60.0).unwrap_err();
        assert!(err.contains("7680x1088"), "{err}");
    }
}
