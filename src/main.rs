#[cfg(feature = "ssim")]
use atomic_float::AtomicF64;
use bytesize::ByteSize;
use clap::Parser;
use color_eyre::eyre::Result;
use image_avif::ImageFile;
use imgref::Img;
use indicatif::ProgressBar;
use log::{debug, log_enabled, Level};
use owo_colors::OwoColorize;
use rayon::{prelude::*, ThreadPoolBuilder};
use rgb::RGBA;
use std::{
    mem::size_of,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use utils::{bar_style, search_dir, ConsoleMsg};

mod image_avif;
mod name_fun;

#[cfg(feature = "ssim")]
mod ssim;
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

    #[cfg(feature = "ssim")]
    /// Calculate the SSIM metric of the original end encoded files. Might use a lot of RAM with very big images.
    #[clap(long = "ssim", default_value_t = false)]
    calculate_ssim: bool,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::builder().format_timestamp(None).init();
    let args: Args = Args::parse();

    let thread_num = if args.threads > 0 {
        args.threads
    } else {
        num_cpus::get()
    };

    let pool = ThreadPoolBuilder::new().num_threads(thread_num).build()?;

    let mut console = ConsoleMsg::new(args.quiet);

    if args.path.is_dir() {
        console.set_spinner("Searching for files...");

        let mut paths = search_dir(&args.path);
        let psize = paths.len();

        let con = console.finish_spinner(&format!("Found {psize} files."));

        if log_enabled!(Level::Debug) {
            let mem_size: usize = paths
                .iter()
                .map(|item| {
                    let vsize = size_of::<Option<Img<Vec<RGBA<u8>>>>>();
                    let unw_item = item.bitmap.as_ref().unwrap();
                    let mem_byte_usg = unw_item.buf().len() * 4;

                    vsize + mem_byte_usg
                })
                .sum();
            debug!(
                "All loaded files occupy {} RAM",
                ByteSize::b(mem_size as u64).to_string_as(true)
            );
        };

        let (final_stats, success_count, global_ctr) =
            (AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0));

        #[cfg(feature = "ssim")]
        let global_ssim = AtomicF64::new(0.0);

        let initial_size: u64 = paths.iter().map(|item| item.size).sum();

        let progress_bar = ProgressBar::new(paths.len() as u64).with_style(bar_style());

        progress_bar.enable_steady_tick(Duration::from_millis(100));

        let threads = if paths.len() >= thread_num {
            1
        } else {
            thread_num / paths.len()
        };

        let start = Instant::now();

        pool.install(|| {
            if let Some(bs) = args.batch_size {
                while !paths.is_empty() {
                    let chunk_len = bs.min(paths.len());

                    let chunk = Vec::from_iter(paths.drain(..chunk_len));

                    let threads = if chunk_len >= thread_num {
                        1
                    } else {
                        thread_num / chunk_len
                    };

                    if log_enabled!(Level::Debug) {
                        let mem_size: usize = chunk
                            .iter()
                            .map(|item| {
                                let vsize = size_of::<Option<Img<Vec<RGBA<u8>>>>>();
                                let unw_item = item.bitmap.as_ref().unwrap();
                                let mem_byte_usg = unw_item.buf().len() * 4;

                                vsize + mem_byte_usg
                            })
                            .sum();
                        debug!(
                            "File batch with size {} occupies {} RAM",
                            chunk_len,
                            ByteSize::b(mem_size as u64).to_string_as(true)
                        );
                    };

                    chunk
                        .into_par_iter()
                        .with_max_len(1)
                        .with_min_len(1)
                        .for_each(|mut item| {
                            if let Ok(results) = item.full_convert(
                                args.quality,
                                args.speed,
                                threads,
                                Some(progress_bar.clone()),
                                args.name_type,
                                args.keep,
                            ) {
                                final_stats.fetch_add(results.size, Ordering::SeqCst);
                                success_count.fetch_add(1, Ordering::SeqCst);

                                #[cfg(feature = "ssim")]
                                global_ssim.fetch_add(results.ssim, Ordering::SeqCst);
                            } else {
                                global_ctr.fetch_add(1, Ordering::SeqCst);
                            }
                        });
                }
            } else {
                paths.into_par_iter().with_max_len(1).for_each(|mut item| {
                    if let Ok(results) = item.full_convert(
                        args.quality,
                        args.speed,
                        threads,
                        Some(progress_bar.clone()),
                        args.name_type,
                        args.keep,
                    ) {
                        final_stats.fetch_add(results.size, Ordering::SeqCst);
                        success_count.fetch_add(1, Ordering::SeqCst);

                        #[cfg(feature = "ssim")]
                        global_ssim.fetch_add(results.ssim, Ordering::SeqCst);
                    } else {
                        global_ctr.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
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

        let times = {
            let ratio = final_stats.load(Ordering::Relaxed) as f32 / initial_size as f32;
            if ratio > 0. {
                let st1 = format!("~{ratio:.2}X smaller");
                format!("{}", st1.green())
            } else {
                let st1 = format!("~{ratio:.2}X bigger");
                format!("{}", st1.red())
            }
        };

        #[cfg(feature = "ssim")]
        if args.calculate_ssim {
            con.print_message(format!(
                "Encoded {} files in {elapsed:.2?}.\n{} {} | {} {} ({} or {}) | Mean SSIM: {:.8}",
                success_count.load(Ordering::SeqCst),
                texts[0],
                ByteSize::b(initial_size).to_string_as(true).blue().bold(),
                texts[1],
                ByteSize::b(final_stats.load(Ordering::SeqCst)).to_string_as(true),
                percentage,
                times,
                global_ssim.load(Ordering::SeqCst) / success_count.load(Ordering::SeqCst) as f64
            ));
            return Ok(());
        }

        con.print_message(format!(
            "Encoded {} files in {elapsed:.2?}.\n{} {} | {} {} ({} or {})",
            success_count.load(Ordering::SeqCst),
            texts[0],
            ByteSize::b(initial_size).to_string_as(true).blue().bold(),
            texts[1],
            ByteSize::b(final_stats.load(Ordering::SeqCst)).to_string_as(true),
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

        #[cfg(feature = "ssim")]
        if args.calculate_ssim {
            use crate::ssim::CalculateSSIM;
            let ssim = image.calculate_ssim()?;

            console.finish_spinner(&format!(
                "Encoding finished ({}) | SSIM: {:.6}",
                ByteSize::b(image.avif_data.len() as u64)
                    .to_string_as(true)
                    .bold()
                    .green(),
                ssim
            ));
            return Ok(());
        }

        console.finish_spinner(&format!(
            "Encoding finished ({})",
            ByteSize::b(fsz).to_string_as(true).bold().green()
        ));
    }

    Ok(())
}
