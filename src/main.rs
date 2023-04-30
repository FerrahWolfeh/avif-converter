use bytesize::ByteSize;
use cli::Args;
use color_eyre::eyre::Result;
use image_avif::ImageFile;
use log::{debug, trace};
use owo_colors::OwoColorize;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    thread::{self, sleep},
    time::{Duration, Instant},
};
use threadpool::ThreadPool;
use utils::{search_dir, ConsoleMsg};

mod cli;
mod image_avif;
mod name_fun;
mod utils;

use crate::utils::PROGRESS_BAR;

static SUCCESS_COUNT: AtomicU64 = AtomicU64::new(0);
static FINAL_STATS: AtomicU64 = AtomicU64::new(0);
static ITEMS_PROCESSED: AtomicU64 = AtomicU64::new(0);

struct ThreadCount {
    task_threads: usize,
    spawn_threads: usize,
}

fn sys_threads(num: usize) -> usize {
    let sel_thread_count = if num > 0 { num } else { num_cpus::get() };

    assert_ne!(sel_thread_count, 0);
    sel_thread_count
}

fn calculate_tread_count(num_threads: usize, num_items: usize) -> ThreadCount {
    let sel_thread_count = sys_threads(num_threads);

    let job_per_thread = if num_items >= sel_thread_count {
        1
    } else {
        num_items / sel_thread_count
    };

    ThreadCount {
        task_threads: job_per_thread,
        spawn_threads: sel_thread_count,
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::builder().format_timestamp(None).init();
    let args: Args = Args::init();

    let mut console = ConsoleMsg::new(args.quiet);

    if args.path.is_dir() {
        console.set_spinner("Searching for files...");

        let mut paths = search_dir(&args.path);
        let psize = paths.len();

        let con = console.finish_spinner(&format!("Found {psize} files."));

        let job_num = calculate_tread_count(args.threads, psize);

        let pool = ThreadPool::with_name("Encoder Thread".to_string(), job_num.spawn_threads);

        let initial_size: u64 = paths.iter().map(|item| item.size).sum();

        con.setup_bar(psize as u64);

        let start = Instant::now();

        for item in paths.drain(..) {
            let mut item = item;
            pool.execute(move || {
                let enc_start = Instant::now();
                trace!(
                    "{} id: {:?} - Encoding file: {}",
                    thread::current().name().unwrap_or("Encoder Thread"),
                    thread::current().id(),
                    item.original_name()
                );

                let bar = if args.quiet {
                    None
                } else {
                    Some(PROGRESS_BAR.clone())
                };

                if let Ok(r_size) =
                    item.convert_to_avif_stored(args.quality, args.speed, job_num.task_threads, bar)
                {
                    SUCCESS_COUNT.fetch_add(1, Ordering::SeqCst);
                    FINAL_STATS.fetch_add(r_size, Ordering::SeqCst);
                }

                if !args.benchmark {
                    item.save_avif(args.name_type, args.keep).unwrap()
                }

                trace!(
                    "{} id: {:?} - Finished encoding: {} | {:?} | {:?}",
                    thread::current().name().unwrap_or("Encoder Thread"),
                    thread::current().id(),
                    item.original_name(),
                    enc_start.elapsed().bold().cyan(),
                    start.elapsed().bold().green()
                );

                drop(item);

                ITEMS_PROCESSED.fetch_add(1, Ordering::SeqCst);

                if args.quiet {
                    debug!(
                        "Items Processed: {}",
                        ITEMS_PROCESSED.load(Ordering::Relaxed)
                    );
                }
            });
            // Debounce in order to start threads safely
            sleep(Duration::from_millis(100));
        }

        debug!("Total of {} jobs queued", pool.queued_count());
        debug!("Pool has {} waiting threads", pool.active_count());

        pool.join();

        let elapsed = start.elapsed();

        con.finish_bar();

        let texts = [
            *"Original folder size".bold().0,
            *"New folder size".bold().0,
        ];

        debug!("Final stats: {}", FINAL_STATS.load(Ordering::Relaxed));
        debug!("Initial size: {}", initial_size);

        let initial_delta = FINAL_STATS.load(Ordering::Relaxed) as f32 / initial_size as f32;

        let delta = (initial_delta * 100.) - 100.;

        debug!("Delta: {}", delta);

        let percentage = if delta < 0. {
            let st1 = format!("{delta:.2}%");
            format!("{}", st1.green())
        } else {
            let st1 = format!("+{delta:.2}%");
            format!("{}", st1.red())
        };

        let times = {
            let ratio = 1. / initial_delta;
            debug!("Ratio: {}", ratio);
            if ratio > 0. {
                let st1 = format!("~{:.1}X smaller", ratio);
                format!("{}", st1.green())
            } else {
                let st1 = format!("~{:.1}X bigger", ratio);
                format!("{}", st1.red())
            }
        };

        con.print_message(format!(
            "Encoded {} files in {elapsed:.2?}.\n{} {} | {} {} ({} or {})",
            SUCCESS_COUNT.load(Ordering::SeqCst),
            texts[0],
            ByteSize::b(initial_size).to_string_as(true).blue().bold(),
            texts[1],
            ByteSize::b(FINAL_STATS.load(Ordering::SeqCst))
                .to_string_as(true)
                .green()
                .bold(),
            percentage,
            times
        ));
    } else if args.path.is_file() {
        let mut image = ImageFile::new_from_path(&args.path)?;

        console.print_message(format!(
            "Encoding single file {} ({})",
            image.name.bold(),
            ByteSize::b(image.size).to_string_as(true).bold().blue()
        ));

        console.set_spinner("Processing...");

        let fsz = image.convert_to_avif_stored(
            args.quality,
            args.speed,
            sys_threads(args.threads),
            None,
        )?;

        image.save_avif(args.name_type, args.keep)?;

        console.finish_spinner(&format!(
            "Encoding finished ({})",
            ByteSize::b(fsz).to_string_as(true).bold().green()
        ));
    }

    Ok(())
}
