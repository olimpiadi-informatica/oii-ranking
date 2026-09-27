//! The video shown in a window while it is rendered, with ffplay (which comes with ffmpeg).
//! The preview never stops a render: without ffplay or a display it is skipped, and closing
//! the window only ends the preview.

use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Mode {
    /// Show the frames as fast as they are rendered (frames are skipped if the window is slower)
    Fast,
    /// Play the video at its real speed: the render waits for the window when it is faster
    Realtime,
    /// No window
    Off,
}

enum Feed {
    /// Frames are written directly: the render waits for the window
    Blocking(ChildStdin),
    /// Frames go through a short queue to a thread writing them: when the queue is full, or
    /// when they come faster than a screen shows them, frames are skipped for the window
    Dropping(SyncSender<Arc<Vec<u8>>>, JoinHandle<()>, Option<Instant>),
}

pub struct Preview {
    child: Child,
    feed: Option<Feed>,
    label: String,
}

impl Preview {
    /// Opens a window for w x h frames (YUV 4:2:0) at `fps`, or None (and why, printed)
    pub fn open(mode: Mode, w: u32, h: u32, fps: f64, label: &str) -> Option<Preview> {
        if mode == Mode::Off {
            return None;
        }
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            println!("Preview: off (no display)");
            return None;
        }
        // the whole 64:9 frame fits in a 1920 pixels wide window, the others in a smaller one
        let width = if w > 2 * h { w.min(1920) } else { w.min(1280) };
        // at the real speed ffplay shows each frame at its time; otherwise, with a very high
        // frame rate, each frame is late when it arrives and is shown at once (or dropped, if a
        // newer one is already there)
        let rate = if mode == Mode::Realtime { format!("{fps}") } else { "1000".into() };
        let spawned = Command::new("ffplay")
            .args(["-hide_banner", "-loglevel", "error", "-autoexit", "-framedrop"])
            .args(["-window_title", &format!("{label} (rendering)"), "-x", &width.to_string()])
            .args(["-f", "rawvideo", "-pixel_format", "yuv420p"])
            .args(["-video_size", &format!("{w}x{h}"), "-framerate", &rate, "-i", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => {
                println!("Preview: off (cannot run ffplay: {e}; it comes with ffmpeg on most systems)");
                return None;
            }
        };
        let stdin = child.stdin.take().expect("ffplay stdin");
        let feed = match mode {
            Mode::Realtime => Feed::Blocking(stdin),
            _ => {
                let (tx, rx) = sync_channel::<Arc<Vec<u8>>>(2);
                let writer = std::thread::spawn(move || {
                    let mut stdin = stdin;
                    for frame in rx {
                        if stdin.write_all(&frame).is_err() {
                            break;
                        }
                    }
                });
                Feed::Dropping(tx, writer, None)
            }
        };
        Some(Preview { child, feed: Some(feed), label: label.to_string() })
    }

    /// Shows a frame (in the realtime mode, waits until the window takes it)
    pub fn show(&mut self, frame: &Arc<Vec<u8>>) {
        let ok = match &mut self.feed {
            None => return,
            Some(Feed::Blocking(stdin)) => stdin.write_all(frame).is_ok(),
            Some(Feed::Dropping(tx, _, last)) => {
                // a screen shows about 60 frames per second: more would only cost time
                if last.is_some_and(|t| t.elapsed() < Duration::from_millis(16)) {
                    return;
                }
                *last = Some(Instant::now());
                !matches!(tx.try_send(frame.clone()), Err(TrySendError::Disconnected(_)))
            }
        };
        if !ok {
            // the window was closed: go on without it
            println!("\nPreview of {} closed: the render goes on", self.label);
            self.feed = None;
        }
    }

    /// Lets the window play the last frames and close
    pub fn finish(mut self) {
        match self.feed.take() {
            Some(Feed::Dropping(tx, writer, _)) => {
                drop(tx);
                let _ = writer.join();
            }
            Some(Feed::Blocking(stdin)) => drop(stdin),
            None => {}
        }
        let _ = self.child.wait();
    }
}

impl Drop for Preview {
    /// A render that fails leaves no window behind
    fn drop(&mut self) {
        if self.feed.is_some() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
