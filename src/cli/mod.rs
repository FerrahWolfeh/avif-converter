use crate::image_file::ImageFile;
use crate::search_dir;
use crate::utils::{calculate_tread_count, sys_threads, PROGRESS_BAR};
use bytesize::ByteSize;
use log::{debug, error, trace};
use owo_colors::OwoColorize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use thread_priority::{set_current_thread_priority, ThreadPriority};
use threadpool::ThreadPool;

use clap::{Parser, ValueEnum};

use crate::console::ConsoleMsg;
use crate::name_fun::Name;
use color_eyre::eyre::{bail, Result};

mod commands;

static SUCCESS_COUNT: AtomicU64 = AtomicU64::new(0);
static FINAL_STATS: AtomicU64 = AtomicU64::new(0);
static ITEMS_PROCESSED: AtomicU64 = AtomicU64::new(0);

fn bit_values(s: &str) -> Result<u8, String> {
    const DEPTHS: [u8; 3] = [8, 10, 12];
    let depth: u8 = s
        .parse()
        .map_err(|_| format!("`{s}` isn't a valid number"))?;

    if DEPTHS.contains(&depth) {
        Ok(depth)
    } else {
        Err("bit depth must be either 8, 10 or 12".to_string())
    }
}

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

    /// Encoded image bit depth.
    #[clap(short = 'd', long, default_value_t = 10, value_parser(bit_values))]
    pub bit_depth: u8,

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

    /// Enable benchmark mode
    #[clap(
        long,
        default_value_t = false,
        conflicts_with = "name_type",
        conflicts_with = "keep",
        conflicts_with = "output_file"
    )]
    pub benchmark: bool,

    #[clap(long, default_value_t = false)]
    pub remove_alpha: bool,

    #[clap(short, long, conflicts_with = "name_type", value_name = "OUTPUT")]
    pub output_file: Option<PathBuf>,

    /// Send a notification to the desktop when all jobs are finished
    #[clap(short = 'N', long, default_value_t = false)]
    pub notify: bool,

    /// Set encoder threads priority
    #[clap(short, long, value_enum, default_value_t = ThreadNice::Default)]
    pub priority: ThreadNice,
}

#[derive(Debug, Copy, Clone, ValueEnum, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThreadNice {
    Max,
    Min,
    Default,
}

impl Args {
    pub fn init() -> Self {
        Self::parse()
    }

    fn set_encoder_priority(thread_level: ThreadNice) {
        let thread_response = match thread_level {
            ThreadNice::Max => ThreadPriority::Max,
            ThreadNice::Min | ThreadNice::Default => ThreadPriority::Min,
        };

        if thread_level == ThreadNice::Default {
            return;
        };

        if set_current_thread_priority(thread_response).is_ok() {
            debug!("Thread priority set to {:?}", thread_response);
        } else {
            error!("Failed to set thread priority. Leaving as default")
        }
    }

    pub fn run_conv(self) -> Result<()> {
        let console = ConsoleMsg::new(self.quiet, self.notify);
        let error_con = ConsoleMsg::new(self.quiet, self.notify);

        let u = {
            if self.path.is_dir() {
                self.dir_conv(console)
            } else if self.path.is_file() {
                self.single_file_conv(console)
            } else {
                bail!("Unsupported operation")
            }
        };

        if let Err(error) = u {
            error_con.notify_error(&error.to_string())?;
        }

        Ok(())
    }

    fn dir_conv(self, console: ConsoleMsg) -> Result<()> {
        if self.output_file.is_some() {
            bail!("Cannot assign an output file to a directory")
        }

        let mut console = console;
        console.set_spinner("Searching for files...");

        let mut paths = search_dir(&self.path);
        let psize = paths.len();

        paths.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));

        let con = console.finish_spinner(&format!("Found {psize} files."));

        let job_num = calculate_tread_count(self.threads, psize);

        let pool = ThreadPool::with_name("Encoder Thread".to_string(), job_num.spawn_threads);

        let initial_size: u64 = paths.iter().map(|item| item.metadata.size).sum();

        con.setup_bar(psize as u64);

        let start = Instant::now();

        for mut item in paths.drain(..) {
            pool.execute(move || {
                Self::set_encoder_priority(self.priority);
                let enc_start = Instant::now();

                let bar = if self.quiet {
                    None
                } else {
                    Some(PROGRESS_BAR.clone())
                };

                if let Ok(r_size) = item.convert_to_avif_stored(
                    self.quality,
                    self.speed,
                    job_num.task_threads,
                    self.bit_depth,
                    bar,
                ) {
                    SUCCESS_COUNT.fetch_add(1, Ordering::SeqCst);
                    FINAL_STATS.fetch_add(r_size, Ordering::SeqCst);
                }

                if !self.benchmark {
                    item.save_avif(None, self.name_type, self.keep).unwrap();
                }

                trace!(
                    "Finished encoding: {} | {:?} | {:?}",
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
        }

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

        con.notify_text(&format!(
            "Encoded {} files in {elapsed:.2?}\n{} → {}",
            SUCCESS_COUNT.load(Ordering::SeqCst),
            ByteSize::b(initial_size).to_string_as(true),
            ByteSize::b(FINAL_STATS.load(Ordering::SeqCst)).to_string_as(true)
        ))?;

        Ok(())
    }

    fn single_file_conv(self, console: ConsoleMsg) -> Result<()> {
        let mut console = console;
        let mut image = ImageFile::new_from_path(&self.path)?;
        let image_size = image.metadata.size;

        console.print_message(format!(
            "Encoding single file {} ({})",
            image.metadata.name.bold(),
            ByteSize::b(image.metadata.size)
                .to_string_as(true)
                .bold()
                .blue()
        ));

        console.set_spinner("Processing...");

        let start = Instant::now();

        let fsz = image.convert_to_avif_stored(
            self.quality,
            self.speed,
            sys_threads(self.threads),
            self.bit_depth,
            None,
        )?;

        if !self.benchmark {
            image.save_avif(self.output_file, self.name_type, self.keep)?;
        }

        let bmp = image.bitmap.clone();

        drop(image);

        console.notify_image(
            &format!(
                "Finished in {:.2?} \n {} → {}",
                start.elapsed(),
                ByteSize::b(image_size).to_string_as(true),
                ByteSize::b(fsz).to_string_as(true)
            ),
            bmp,
        )?;

        console.finish_spinner(&format!(
            "Encoding finished in {:?} ({})",
            start.elapsed(),
            ByteSize::b(fsz).to_string_as(true).bold().green()
        ));

        Ok(())
    }
}
