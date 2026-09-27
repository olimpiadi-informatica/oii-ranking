//! The video encoder: a GPU through VAAPI (AMD and Intel on Linux) when one can encode the
//! video, else the CPU encoder of the settings. Whether a GPU works is found out by encoding a
//! few frames with it: drivers without the codecs (e.g. Fedora's Mesa), frames too big for
//! the hardware and missing devices all show up there.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::PathBuf;
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

    fn vaapi(device: &str, codec: &str, qp: u32) -> Encoder {
        let mut output = strings(&["-vf", "format=nv12,hwupload", "-c:v", codec, "-rc_mode", "CQP"]);
        output.extend(["-qp".to_string(), qp.to_string()]);
        if codec.starts_with("hevc") {
            // the tag QuickTime and Apple devices need to play HEVC in mp4
            output.extend(strings(&["-tag:v", "hvc1"]));
        }
        Encoder { name: format!("{codec} on {device}"), global: strings(&["-vaapi_device", device]), output }
    }
}

/// The GPUs to try, from `video.hardware`: "auto" (all of them), "off" (none) or a device
fn devices(setting: &str) -> Vec<String> {
    match setting.trim() {
        "off" | "" => vec![],
        "auto" => {
            let mut v: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("renderD")))
                .collect();
            v.sort();
            v.into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
        }
        device => vec![device.to_string()],
    }
}

/// H.264 plays everywhere, but hardware encoders stop at 4096 pixels: HEVC goes further
fn codecs(w: u32, h: u32) -> &'static [&'static str] {
    if w <= 4096 && h <= 4096 { &["h264_vaapi", "hevc_vaapi"] } else { &["hevc_vaapi"] }
}

/// Encodes a few black frames of the video's size: Ok if the GPU can do it, else the reason
fn probe(encoder: &Encoder, w: u32, h: u32, fps: f64) -> Result<(), String> {
    let mut cmd = Command::new("ffmpeg");
    // at the verbose level ffmpeg says why the driver refuses (e.g. a profile it lacks)
    cmd.args(["-hide_banner", "-v", "verbose"])
        .args(&encoder.global)
        .args(["-f", "lavfi", "-i", &format!("color=black:s={w}x{h}:r={fps}"), "-frames:v", "3"])
        .args(&encoder.output)
        .args(["-f", "null", "-"])
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
    let telling = ["not supported", "does not support", "No such file", "Failed to", "failed", "Error while opening"];
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
    let devices = devices(setting);
    if devices.is_empty() {
        return Err(if setting.trim() == "off" { "off in the settings".into() } else { "no GPU found".into() });
    }
    let mut failures = vec![];
    for device in &devices {
        for codec in codecs(w, h) {
            let encoder = Encoder::vaapi(device, codec, config.video.hardware_qp);
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
            Err(why) => println!("Encoder: {} (CPU: {why})", first.name),
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
        assert_eq!(codecs(1920, 1080), ["h264_vaapi", "hevc_vaapi"]);
        assert_eq!(codecs(7680, 1080), ["hevc_vaapi"]);
    }

    #[test]
    fn device_setting() {
        assert!(devices("off").is_empty());
        assert_eq!(devices("/dev/dri/renderD129"), ["/dev/dri/renderD129"]);
    }

    #[test]
    fn reasons_are_readable() {
        let log = "[h264_vaapi @ 0x55] Compatible profile VAProfileH264High (7) is not supported by driver.\n\
                   [vost#0:0/h264_vaapi @ 0x55] Error while opening encoder - maybe incorrect parameters";
        assert_eq!(reason(log), "Compatible profile VAProfileH264High (7) is not supported by driver.");
    }

    #[test]
    fn missing_device_fails_the_probe() {
        let encoder = Encoder::vaapi("/dev/dri/does-not-exist", "h264_vaapi", 20);
        assert!(probe(&encoder, 320, 240, 30.0).is_err());
    }
}
