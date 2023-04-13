use bytesize::ByteSize;
use clap::Parser;
use color_eyre::eyre::Result;
use image_avif::ImageFile;
use indicatif::ProgressBar;
use once_cell::sync::Lazy;
use owo_colors::OwoColorize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::spawn,
    time::{Duration, Instant},
};
use utils::{bar_style, search_dir, ConsoleMsg};

mod image_avif;
mod name_fun;

mod utils;

use crate::name_fun::Name;

#[derive(Debug, Clone, Parser)]
struct Args {
    /// File or directory containing images to convert
    #[clap(value_name = "PATH")]
    path: PathBuf,

    #[clap(short, long, default_value_t = 70, value_name = "QUALITY")]
    quality: u8,

    #[clap(short, long, default_value_t = 4, value_name = "SPEED")]
    speed: u8,

    #[clap(short, long, value_enum, default_value_t = Name::MD5)]
    name_type: Name,

    /// Defaults to number of CPU cores. Use 0 for all cores
    #[clap(short, long, default_value_t = 0, value_name = "THREADS")]
    threads: usize,

    /// How many images to keep in memory at once
    #[clap(short, long)]
    batch_size: Option<usize>,

    /// Supress console messages
    #[clap(long, default_value_t = false)]
    quiet: bool,

    /// Keep original file
    #[clap(short, long, default_value_t = false)]
    keep: bool,
}

static GLOBAL_COUNTER: AtomicU64 = AtomicU64::new(0);
static SUCCESS_COUNT: AtomicU64 = AtomicU64::new(0);
static FINAL_STATS: AtomicU64 = AtomicU64::new(0);
static PROGRESS_BAR: Lazy<ProgressBar> = Lazy::new(|| ProgressBar::new(0).with_style(bar_style()));

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::builder().format_timestamp(None).init();
    let args: Args = Args::parse();

    let thread_num = if args.threads > 0 {
        args.threads
    } else {
        num_cpus::get()
    };

    let mut console = ConsoleMsg::new(args.quiet);

    if args.path.is_dir() {
        console.set_spinner("Searching for files...");

        let paths = search_dir(&args.path);
        let psize = paths.len();

        let con = console.finish_spinner(&format!("Found {psize} files."));

        let initial_size: u64 = paths.iter().map(|item| item.size).sum();

        PROGRESS_BAR.set_length(paths.len() as u64);

        PROGRESS_BAR.enable_steady_tick(Duration::from_millis(100));

        let threads = if paths.len() >= thread_num {
            1
        } else {
            thread_num / paths.len()
        };

        let (tx, rx) = mpsc::sync_channel(thread_num);

        let rx = Arc::new(Mutex::new(rx));

        let mut handles = Vec::with_capacity(thread_num);

        for _ in 0..thread_num {
            let rx = rx.clone();
            let handle = spawn(move || {
                while let Ok(item) = rx.lock().unwrap().recv() {
                    let mut item: ImageFile = item;
                    if let Ok(results) = item.full_convert(
                        args.quality,
                        args.speed,
                        threads,
                        Some(PROGRESS_BAR.clone()),
                        args.name_type,
                        args.keep,
                    ) {
                        SUCCESS_COUNT.fetch_add(results.size, Ordering::SeqCst);
                        FINAL_STATS.fetch_add(1, Ordering::SeqCst);
                    } else {
                        GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
                    }
                }
            });
            handles.push(handle);
        }

        let start = Instant::now();

        for item in paths {
            tx.send(item)?;
        }

        for handle in handles {
            handle.join().expect("Failed to join thread");
        }

        let elapsed = start.elapsed();

        PROGRESS_BAR.finish();

        let texts = [
            *"Original folder size".bold().0,
            *"New folder size".bold().0,
        ];

        let delta =
            ((FINAL_STATS.load(Ordering::SeqCst) as f32 / initial_size as f32) * 100.) - 100.;

        let percentage = if delta < 0. {
            let st1 = format!("{delta:.2}%");
            format!("{}", st1.green())
        } else {
            let st1 = format!("+{delta:.2}%");
            format!("{}", st1.red())
        };

        let times = {
            let ratio = FINAL_STATS.load(Ordering::Relaxed) as f32 / initial_size as f32;
            if ratio > 0. {
                let st1 = format!("~{ratio:.2}X smaller");
                format!("{}", st1.green())
            } else {
                let st1 = format!("~{ratio:.2}X bigger");
                format!("{}", st1.red())
            }
        };

        con.print_message(format!(
            "Encoded {} files in {elapsed:.2?}.\n{} {} | {} {} ({} or {})",
            SUCCESS_COUNT.load(Ordering::SeqCst),
            texts[0],
            ByteSize::b(initial_size).to_string_as(true).blue().bold(),
            texts[1],
            ByteSize::b(FINAL_STATS.load(Ordering::SeqCst)).to_string_as(true),
            percentage,
            times
        ));
    } else if args.path.is_file() {
        let mut image = ImageFile::load_from_path(&args.path)?;

        console.print_message(format!(
            "Encoding single file {} ({})",
            image.name.bold(),
            ByteSize::b(image.size).to_string_as(true).bold().blue()
        ));

        console.set_spinner("Processing...");

        let fsz = image.convert_to_avif_stored(args.quality, args.speed, thread_num, None)?;

        image.save_avif(args.name_type, args.keep)?;

        console.finish_spinner(&format!(
            "Encoding finished ({})",
            ByteSize::b(fsz).to_string_as(true).bold().green()
        ));
    }

    Ok(())
}
