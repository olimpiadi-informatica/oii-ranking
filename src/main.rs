//! Award ceremony videos for the OII.

mod config;
mod data;
mod images;
mod preprocess;
mod render;
mod scenes;
mod text;
mod timeline;
mod tools;
mod vector;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

use config::{Config, Medal};
use render::{Canvas, VideoOptions};
use timeline::Scene;

#[derive(Parser)]
#[command(version, about = "Award ceremony videos for the Italian Olympiad in Informatics")]
struct Cli {
    /// Data directory (with ranking.csv, output/, screenshots/, images/, ...)
    #[arg(short = 'C', long = "dir", global = true, default_value = ".")]
    dir: PathBuf,
    /// Settings file, relative to the data directory
    #[arg(long, global = true, default_value = "config.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Turn the contest data into output/ (history, ranking, cropped faces)
    Preprocess {
        #[arg(long, default_value = "faces")]
        faces_dir: PathBuf,
        #[arg(long, default_value = "ranking")]
        ranking_dir: PathBuf,
        #[arg(long, default_value = "ranking.csv")]
        ranking_csv: PathBuf,
        #[arg(long, default_value = "output")]
        output_dir: PathBuf,
        /// Import data in terry (and/or quizms) format instead of cms
        #[arg(short, long)]
        terry: bool,
    },
    /// Render the video of a medal (or of all of them)
    Render {
        /// gold, silver, bronze, mention or all
        medal: String,
        #[command(flatten)]
        video: VideoArgs,
        /// Output file (default: <paths.videos>/<layout>/<height>p<fps>/<Medal>.mp4)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Start of the part to render, in seconds
        #[arg(long)]
        from: Option<f64>,
        /// End of the part to render, in seconds
        #[arg(long)]
        to: Option<f64>,
    },
    /// Render a single frame to a PNG, to check the layout
    Still {
        /// gold, silver, bronze or mention
        medal: String,
        /// Time of the frame, in seconds
        #[arg(long)]
        at: f64,
        #[command(flatten)]
        video: VideoArgs,
        #[arg(short, long, default_value = "frame.png")]
        output: PathBuf,
    },
    /// Render an overlay with a transparent background (ProRes 4444 .mov)
    Overlay {
        #[command(subcommand)]
        overlay: Overlay,
        #[arg(short, long, value_enum, default_value = "h", global = true)]
        quality: Quality,
        #[arg(short, long, global = true)]
        output: Option<PathBuf>,
    },
    /// Create placeholder data (15 fake contestants) for a quick test
    MakeTestData,
    /// Convert the raw export in orig/ (temp.csv, faces.zip) into ranking.csv and faces/
    ImportOrig,
}

#[derive(Subcommand)]
enum Overlay {
    /// Countdown from N to 0
    Countdown { n: u32 },
    /// Lower third with the name and the title of a person
    NameMarker { name: String, title: String },
}

#[derive(clap::Args)]
struct VideoArgs {
    /// l = 854x480 15fps (preview), m = 720p30, h = 1080p60 (final), p = 1440p60, k = 2160p60
    #[arg(short, long, value_enum, default_value = "h")]
    quality: Quality,
    /// 64:9 layout (settings in [wide] of config.toml)
    #[arg(short, long)]
    wide: bool,
    /// Only these contestants (by username, comma separated; no recap pages)
    #[arg(long, value_delimiter = ',')]
    only: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Quality {
    L,
    M,
    H,
    P,
    K,
}

impl Quality {
    /// (height, fps), as manim's -ql .. -qk
    fn params(self) -> (u32, f64) {
        match self {
            Quality::L => (480, 15.0),
            Quality::M => (720, 30.0),
            Quality::H => (1080, 60.0),
            Quality::P => (1440, 60.0),
            Quality::K => (2160, 60.0),
        }
    }
}

/// Pixel size of the video (the width must be even for the encoder)
fn video_size(config: &Config, quality: Quality, wide: bool) -> Result<(u32, u32, f64)> {
    let (h, fps) = quality.params();
    if !wide {
        let w = if h == 480 { 854 } else { h * 16 / 9 };
        return Ok((w, h, fps));
    }
    if matches!(quality, Quality::P | Quality::K) {
        bail!("the wide layout supports only the l, m and h qualities (wider videos are not supported by h264)");
    }
    let [aw, ah] = config.wide.aspect;
    let w = 2 * ((h as f64 * aw as f64 / ah as f64 / 2.0).round() as u32);
    Ok((w, h, fps))
}

fn parse_medals(s: &str) -> Result<Vec<Medal>> {
    if s.eq_ignore_ascii_case("all") {
        return Ok(Medal::ALL.to_vec());
    }
    Medal::parse(s)
        .map(|m| vec![m])
        .with_context(|| format!("unknown medal {s:?}: use gold, silver, bronze, mention or all"))
}

fn build(
    config: &Config,
    data: &data::Data,
    fonts: &text::Fonts,
    images: &images::Images,
    medal: Medal,
    v: &VideoArgs,
) -> Result<(Scene, Canvas)> {
    let (w, h, fps) = video_size(config, v.quality, v.wide)?;
    let aspect = w as f64 / h as f64;
    let wide_config;
    let config = if v.wide {
        wide_config = config.widened();
        &wide_config
    } else {
        config
    };
    let ctx = scenes::Ctx::new(config, data, fonts, images, medal, &v.only)?;
    println!("Rendering ranking of {} students with medal \"{}\" at {w}x{h}...\n", ctx.users.len(), medal.key());
    let scene =
        if v.wide { scenes::wide::build(&ctx, fps, aspect) } else { scenes::standard::build(&ctx, fps, aspect) };
    let canvas = Canvas::new(w, h, scene.fh);
    Ok((scene, canvas))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    std::env::set_current_dir(&cli.dir).with_context(|| format!("entering {}", cli.dir.display()))?;
    let config = Config::load(Some(&cli.config))?;

    match cli.command {
        Command::Preprocess { faces_dir, ranking_dir, ranking_csv, output_dir, terry } => {
            let opts = preprocess::Options { faces_dir, ranking_dir, ranking_csv, output_dir, terry };
            preprocess::run(&config, &opts)
        }
        Command::Render { medal, video, output, from, to } => {
            let medals = parse_medals(&medal)?;
            if output.is_some() && medals.len() > 1 {
                bail!("--output needs a single medal");
            }
            let data = data::Data::load(&config)?;
            let fonts = text::Fonts::load(&config)?;
            for m in medals {
                let images = images::Images::new();
                let (scene, canvas) = build(&config, &data, &fonts, &images, m, &video)?;
                let total = scene.total_frames();
                let first = from.map_or(0, |s| (s * scene.fps).round() as usize);
                let last = to.map_or(total, |s| (s * scene.fps).round() as usize);
                let path = output.clone().unwrap_or_else(|| {
                    let layout = if video.wide { "wide" } else { "standard" };
                    let mut name = m.title().to_string();
                    if !video.only.is_empty() {
                        name += &format!("_{}", video.only.join("_"));
                    }
                    if from.is_some() || to.is_some() {
                        name += &format!("_{}-{}s", from.unwrap_or(0.0), to.map_or("end".into(), |t| t.to_string()));
                    }
                    Path::new(&config.paths.videos)
                        .join(layout)
                        .join(format!("{}p{}", canvas.h, scene.fps))
                        .join(name + ".mp4")
                });
                println!("{}: {:.1}s, {} frames", m.title(), total as f64 / scene.fps, total);
                let opts = VideoOptions {
                    path,
                    transparent: false,
                    encoder: config.video.encoder.clone(),
                    encoder_options: config.video.encoder_options.clone(),
                    frames: (first, last),
                    label: m.title().to_string(),
                };
                render::render_video(&scene, &canvas, &images, &opts)?;
                let (used, decoded, decodes) = images.decode_stats(Path::new(&config.paths.screen_dir));
                println!("{}: screenshots: {used} in the video, {decoded} decoded, {decodes} decodes", m.title());
            }
            Ok(())
        }
        Command::Still { medal, at, video, output } => {
            let m = Medal::parse(&medal).with_context(|| format!("unknown medal {medal:?}"))?;
            let data = data::Data::load(&config)?;
            let fonts = text::Fonts::load(&config)?;
            let images = images::Images::new();
            let (scene, canvas) = build(&config, &data, &fonts, &images, m, &video)?;
            let frame = render::render_frame(&scene, &canvas, &images, at, 1);
            render::save_png(frame, &output)?;
            println!("{} (the video lasts {:.1}s)", output.display(), scene.total_frames() as f64 / scene.fps);
            Ok(())
        }
        Command::Overlay { overlay, quality, output } => {
            let (w, h, fps) = video_size(&config, quality, false)?;
            let aspect = w as f64 / h as f64;
            let (scene, name) = match overlay {
                Overlay::Countdown { n } => (scenes::overlays::countdown(fps, aspect, n), "Countdown"),
                Overlay::NameMarker { name, title } => {
                    (scenes::overlays::name_marker(fps, aspect, &name, &title), "NameMarker")
                }
            };
            let canvas = Canvas::new(w, h, scene.fh);
            let path = output.unwrap_or_else(|| {
                Path::new(&config.paths.videos).join("overlays").join(format!("{h}p{fps}")).join(format!("{name}.mov"))
            });
            let opts = VideoOptions {
                path,
                transparent: true,
                encoder: String::new(),
                encoder_options: vec![],
                frames: (0, scene.total_frames()),
                label: name.to_string(),
            };
            render::render_video(&scene, &canvas, &images::Images::new(), &opts)
        }
        Command::MakeTestData => tools::make_test_data(&config),
        Command::ImportOrig => tools::import_orig(),
    }
}
