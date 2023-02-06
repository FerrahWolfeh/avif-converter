use std::{
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use bytesize::ByteSize;
use clap::{Parser, ValueEnum};
use color_eyre::eyre::Result;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use owo_colors::OwoColorize;
use ravif::{Encoder, Img};
use rayon::{prelude::*, ThreadPoolBuilder};
use rgb::FromSlice;
use sha2::{Digest, Sha256};
use spinoff::{spinners, Color, Spinner, Streams};

#[derive(Debug, ValueEnum, Copy, Clone)]
#[repr(u8)]
enum Name {
    MD5,
    SHA256,
    Random,
}

#[derive(Debug, Clone, Parser)]
struct Args {
    /// Dir where all images are located
    dir: PathBuf,
    #[clap(short, long, default_value_t = 70)]
    quality: u8,

    #[clap(short, long, default_value_t = 4)]
    speed: u8,

    #[clap(short, long, value_enum, default_value_t = Name::MD5)]
    name_type: Name,

    /// Defaults to number of CPU cores
    #[clap(short, long, default_value_t = 0)]
    threads: usize,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args: Args = Args::parse();

    let thread_num = if args.threads > 0 {
        args.threads
    } else {
        num_cpus::get()
    };

    ThreadPoolBuilder::new()
        .num_threads(thread_num)
        .build_global()
        .unwrap();

    if args.dir.is_dir() {
        let spin_collect = Spinner::new_with_stream(
            spinners::Dots,
            "Searching for files...",
            Color::Green,
            Streams::Stderr,
        );

        let paths = search_dir(&args.dir);
        let psize = paths.len();

        spin_collect.success(&format!("Found {psize} files."));

        let mut final_stats = Vec::with_capacity(psize);

        let mut global_ctr = 0;

        let initial_size: u64 = paths.iter().map(|(_, num)| num).sum();

        let progress_bar = ProgressBar::new(paths.len() as u64).with_style(bar_style());

        progress_bar.enable_steady_tick(Duration::from_millis(100));

        let start = Instant::now();

        paths
            .par_iter()
            .with_max_len(1)
            .map(|(item, _)| {
                process_image(
                    item,
                    args.quality,
                    args.speed,
                    args.name_type,
                    Some(progress_bar.clone()),
                )
            })
            .collect_into_vec(&mut final_stats);

        let elapsed = start.elapsed();

        progress_bar.finish();

        let simd_stats = Vec::from_iter(final_stats.into_iter().filter_map(|result| {
            if let Ok(new_size) = result {
                Some(new_size)
            } else {
                global_ctr += 1;
                None
            }
        }));

        let f_len = simd_stats.len();
        let sum: u64 = simd_stats.iter().sum();

        let texts = ["Original folder size".bold().0, "New folder size".bold().0];

        let delta = ((sum as f32 / initial_size as f32) * 100.) - 100.;

        let percentage = if delta < 0. {
            let st1 = format!("{delta:.2}%");
            format!("{}", st1.green())
        } else {
            let st1 = format!("+{delta:.2}%");
            format!("{}", st1.red())
        };

        println!(
            "Encoded {f_len} files in {elapsed:.2?}.\n{} {} | {} {} ({})",
            texts[0],
            ByteSize::b(initial_size).to_string_as(true).blue().bold(),
            texts[1],
            ByteSize::b(sum).to_string_as(true),
            percentage
        );
    } else if args.dir.is_file() {
        println!(
            "Encoding single file {} ({})",
            args.dir.file_name().unwrap().to_string_lossy().bold(),
            ByteSize::b(args.dir.metadata()?.len())
                .to_string_as(true)
                .bold()
                .blue()
        );

        let spin_collect = Spinner::new_with_stream(
            spinners::Dots,
            "Processing...",
            Color::Green,
            Streams::Stderr,
        );

        let fsz = process_image(&args.dir, args.quality, args.speed, args.name_type, None)?;

        spin_collect.success(&format!(
            "Encoding finished ({})",
            ByteSize::b(fsz).to_string_as(true).bold().green()
        ));
    }

    Ok(())
}

fn process_image(
    image: &Path,
    quality: u8,
    speed: u8,
    name: Name,
    progress: Option<ProgressBar>,
) -> Result<u64> {
    let raw_img = image::open(image)?;

    let (width, height) = (raw_img.width(), raw_img.height());

    let binding = raw_img.to_rgba8();

    let encodable_img = Img::new(binding.as_rgba(), width as usize, height as usize);

    let encoder = Encoder::new()
        .with_num_threads(Some(1))
        .with_alpha_quality(100.)
        .with_quality(quality as f32)
        .with_speed(speed);

    let encoded_img = encoder.encode_rgba(encodable_img)?;

    let avif = encoded_img.avif_file;

    let fname = match name {
        Name::MD5 => {
            let digest = md5::compute(&avif);

            format!("{digest:x}")
        }
        Name::SHA256 => {
            let mut hasher = Sha256::new();

            hasher.update(&avif);

            hex::encode(hasher.finalize())
        }
        Name::Random => todo!(),
    };

    let binding = image.canonicalize()?;
    let fpath = binding.parent().unwrap();

    fs::write(fpath.join(format!("{fname}.avif")), &avif)?;
    fs::remove_file(image)?;

    if let Some(pb) = progress {
        pb.inc(1);
    }

    Ok(avif.len() as u64)
}

fn search_dir(dir: &Path) -> Vec<(PathBuf, u64)> {
    let paths = fs::read_dir(dir).unwrap();

    Vec::from_iter(paths.filter_map(|entry| {
        let entry = entry.unwrap();
        let path = entry.path();
        let ext = path.extension();
        if let Some(ext) = ext {
            if ext == "jpg" || ext == "png" || ext == "jpeg" || ext == "jfif" || ext == "webp" {
                let size = entry.metadata().unwrap().len();
                return Some((path, size));
            }
        }
        None
    }))
}

fn bar_style() -> ProgressStyle {
    let template = "{spinner:.red.bold} {elapsed_precise:.bold} [{wide_bar:.blue.bold}] {percent:.bold} {pos:.bold} (eta. {eta})";

    ProgressStyle::default_bar()
        .template(template)
        .unwrap()
        .with_key("pos", |state: &ProgressState, w: &mut dyn Write| {
            write!(w, "{}/{}", state.pos(), state.len().unwrap()).unwrap();
        })
        .with_key("percent", |state: &ProgressState, w: &mut dyn Write| {
            write!(w, "{:>3.0}%", state.fraction() * 100_f32).unwrap();
        })
        .progress_chars("# ")
}
