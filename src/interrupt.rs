//! Ctrl+C. During a render, the first one stops it cleanly: no more frames are drawn, the ones
//! already drawn are encoded, and ffmpeg closes the video, which stays as the partial output.
//! A second one, or one outside a render, quits at once.
//!
//! ffmpeg and ffplay are started in their own process group (see [`spawn_detached`]), so that the
//! terminal's Ctrl+C does not reach them: otherwise they would stop in the middle of a frame.

use std::process::{Child, Command};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);
static RENDERING: AtomicBool = AtomicBool::new(false);
/// The detached processes still running, killed when quitting at once
static CHILDREN: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// The error of a render stopped with Ctrl+C
#[derive(Debug)]
pub struct Interrupted;

impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("interrupted")
    }
}

impl std::error::Error for Interrupted {}

pub fn install() {
    let result = ctrlc::set_handler(|| {
        if !RENDERING.load(Ordering::SeqCst) || REQUESTED.swap(true, Ordering::SeqCst) {
            eprintln!("\nInterrupted");
            for &pid in CHILDREN.lock().unwrap().iter() {
                #[cfg(unix)]
                // SAFETY: kill only sends a signal
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGKILL);
                }
            }
            std::process::exit(130);
        }
        eprintln!("\nStopping: encoding the frames drawn so far (Ctrl+C again to quit at once)");
    });
    if let Err(e) = result {
        eprintln!("note: Ctrl+C will not stop renders cleanly ({e})");
    }
}

/// Whether Ctrl+C was pressed during a render
pub fn requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}

/// While it lives, Ctrl+C stops the render cleanly instead of quitting
pub struct Rendering;

impl Rendering {
    pub fn start() -> Rendering {
        RENDERING.store(true, Ordering::SeqCst);
        Rendering
    }
}

impl Drop for Rendering {
    fn drop(&mut self) {
        RENDERING.store(false, Ordering::SeqCst);
    }
}

/// Starts `cmd` in its own process group, out of reach of the terminal's Ctrl+C; it is
/// killed if the program quits at once
pub fn spawn_detached(cmd: &mut Command) -> std::io::Result<Child> {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(cmd, 0);
    let child = cmd.spawn()?;
    CHILDREN.lock().unwrap().push(child.id());
    Ok(child)
}

/// `child` (started with [`spawn_detached`]) has ended
pub fn forget(child: &Child) {
    CHILDREN.lock().unwrap().retain(|&pid| pid != child.id());
}
