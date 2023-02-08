use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use bytesize::ByteSize;
use clap::Parser;
use color_eyre::eyre::Result;
use image_avif::ImageFile;
use indicatif::ProgressBar;
use owo_colors::OwoColorize;
use rayon::{prelude::*, ThreadPoolBuilder};
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

    /// Supress console messages
    #[clap(long, default_value_t = false)]
    quiet: bool,

    /// Keep original file
    #[clap(short, long, default_value_t = false)]
    keep: bool,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args: Args = Args::parse();

    let thread_num = if args.threads > 0 {
        args.threads
    } else {
        num_cpus::get()
    };

    let pool = ThreadPoolBuilder::new().num_threads(thread_num).build()?;

    if args.path.is_dir() {
        let mut console = ConsoleMsg::new(args.quiet);

        console.set_spinner("Searching for files...");

        let paths = search_dir(&args.path);
        let psize = paths.len();

        let con = console.finish_spinner(&format!("Found {psize} files."));

        let final_stats: AtomicU64 = AtomicU64::new(0);
        let success_count: AtomicU64 = AtomicU64::new(0);

        let global_ctr: AtomicU64 = AtomicU64::new(0);

        let initial_size: u64 = paths.iter().map(|item| item.size).sum();

        let progress_bar = ProgressBar::new(paths.len() as u64).with_style(bar_style());

        progress_bar.enable_steady_tick(Duration::from_millis(100));

        let start = Instant::now();

        let threads = if paths.len() >= thread_num {
            1
        } else {
            thread_num / paths.len()
        };

        pool.install(|| {
            paths.into_par_iter().with_max_len(1).for_each(|item| {
                if let Ok(size) = item.full_convert(
                    args.quality,
                    args.speed,
                    threads,
                    Some(progress_bar.clone()),
                    args.name_type,
                    args.keep,
                ) {
                    final_stats.fetch_add(size, Ordering::SeqCst);
                    success_count.fetch_add(1, Ordering::SeqCst);
                } else {
                    global_ctr.fetch_add(1, Ordering::SeqCst);
                }
            })
        });

        let elapsed = start.elapsed();

        progress_bar.finish();

        let texts = [
            *"Original folder size".bold().0,
            *"New folder size".bold().0,
        ];

        let delta =
            ((final_stats.load(Ordering::SeqCst) as f32 / initial_size as f32) * 100.) - 100.;

        let percentage = if delta < 0. {
            let st1 = format!("{delta:.2}%");
            format!("{}", st1.green())
        } else {
            let st1 = format!("+{delta:.2}%");
            format!("{}", st1.red())
        };

        con.print_message(format!(
            "Encoded {} files in {elapsed:.2?}.\n{} {} | {} {} ({})",
            success_count.load(Ordering::SeqCst),
            texts[0],
            ByteSize::b(initial_size).to_string_as(true).blue().bold(),
            texts[1],
            ByteSize::b(final_stats.load(Ordering::SeqCst)).to_string_as(true),
            percentage
        ));
    } else if args.path.is_file() {
        let image = ImageFile::from_path(&args.path)?;

        let mut console = ConsoleMsg::new(args.quiet);

        console.print_message(format!(
            "Encoding single file {} ({})",
            image.name.bold(),
            ByteSize::b(image.size).to_string_as(true).bold().blue()
        ));

        console.set_spinner("Processing...");

        let fsz = image.convert_to_avif(args.quality, args.speed, thread_num, None)?;

        image.save_avif(&fsz, args.name_type, args.keep)?;

        console.finish_spinner(&format!(
            "Encoding finished ({})",
            ByteSize::b(fsz.len() as u64)
                .to_string_as(true)
                .bold()
                .green()
        ));
    }

    Ok(())
}
