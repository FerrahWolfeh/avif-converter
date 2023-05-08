use crate::image_file::ImageFile;
use crate::search_dir;
use crate::utils::{calculate_tread_count, sys_threads, PROGRESS_BAR};
use bytesize::ByteSize;
use log::{debug, trace};
use owo_colors::OwoColorize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, sleep};
use std::time::{Duration, Instant};
use threadpool::ThreadPool;

use clap::Parser;

use crate::name_fun::Name;
use crate::ConsoleMsg;
use color_eyre::eyre::{bail, Result};

mod commands;

static SUCCESS_COUNT: AtomicU64 = AtomicU64::new(0);
static FINAL_STATS: AtomicU64 = AtomicU64::new(0);
static ITEMS_PROCESSED: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Parser)]
pub struct Args {
    /// File or directory containing images to convert
    #[clap(value_name = "PATH")]
    pub path: PathBuf,

    #[clap(short, long, default_value_t = 70, value_name = "QUALITY")]
    pub quality: u8,

    #[clap(short, long, default_value_t = 4, value_name = "SPEED")]
    pub speed: u8,

    #[clap(short, long, value_enum, default_value_t = Name::MD5)]
    pub name_type: Name,

    /// Defaults to number of CPU cores. Use 0 for all cores
    #[clap(short, long, default_value_t = 0, value_name = "THREADS")]
    pub threads: usize,

    /// How many images to keep in memory at once
    #[clap(short, long)]
    pub batch_size: Option<usize>,

    /// Supress console messages
    #[clap(long, default_value_t = false)]
    pub quiet: bool,

    /// Keep original file
    #[clap(short, long, default_value_t = false)]
    pub keep: bool,

    /// Enable benchmark mode (will not save any file after encode)
    #[clap(long, default_value_t = false)]
    pub benchmark: bool,

    #[clap(long, default_value_t = false)]
    pub remove_alpha: bool,
}

impl Args {
    pub fn init() -> Self {
        Self::parse()
    }

    pub fn run_conv(self) -> Result<()> {
        let console = ConsoleMsg::new(self.quiet);

        if self.path.is_dir() {
            self.dir_conv(console)
        } else if self.path.is_file() {
            self.single_file_conv(console)
        } else {
            bail!("Unsupported operation")
        }
    }

    fn dir_conv(self, console: ConsoleMsg) -> Result<()> {
        let mut console = console;
        console.set_spinner("Searching for files...");

        let mut paths = search_dir(&self.path);
        let psize = paths.len();

        let con = console.finish_spinner(&format!("Found {psize} files."));

        let job_num = calculate_tread_count(self.threads, psize);

        let pool = ThreadPool::with_name("Encoder Thread".to_string(), job_num.spawn_threads);

        let initial_size: u64 = paths.iter().map(|item| item.metadata.size).sum();

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

                let bar = if self.quiet {
                    None
                } else {
                    Some(PROGRESS_BAR.clone())
                };

                if let Ok(r_size) =
                    item.convert_to_avif_stored(self.quality, self.speed, job_num.task_threads, bar)
                {
                    SUCCESS_COUNT.fetch_add(1, Ordering::SeqCst);
                    FINAL_STATS.fetch_add(r_size, Ordering::SeqCst);
                }

                if !self.benchmark {
                    item.save_avif(self.name_type, self.keep).unwrap()
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

                if self.quiet {
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
        Ok(())
    }

    fn single_file_conv(self, console: ConsoleMsg) -> Result<()> {
        let mut console = console;
        let mut image = ImageFile::new_from_path(&self.path)?;

        console.print_message(format!(
            "Encoding single file {} ({})",
            image.metadata.name.bold(),
            ByteSize::b(image.metadata.size)
                .to_string_as(true)
                .bold()
                .blue()
        ));

        console.set_spinner("Processing...");

        let fsz = image.convert_to_avif_stored(
            self.quality,
            self.speed,
            sys_threads(self.threads),
            None,
        )?;

        if !self.benchmark {
            image.save_avif(self.name_type, self.keep)?;
        }

        console.finish_spinner(&format!(
            "Encoding finished ({})",
            ByteSize::b(fsz).to_string_as(true).bold().green()
        ));

        Ok(())
    }
}
