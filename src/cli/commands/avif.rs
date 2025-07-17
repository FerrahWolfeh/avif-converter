use std::{path::PathBuf, process::exit, sync::atomic::Ordering, time::Instant};

use bytesize::ByteSize;
use clap::Args;
use color_eyre::eyre::bail;
use log::{debug, trace};
use owo_colors::OwoColorize;
use threadpool::ThreadPool;

use crate::{
    cli::{Args as Globals, FINAL_STATS, ITEMS_PROCESSED, SUCCESS_COUNT},
    console::ConsoleMsg,
    image_file::ImageFile,
    utils::{calculate_tread_count, parse_files, sys_threads, PROGRESS_BAR},
};

#[cfg(feature = "ssim")]
use crate::ssim::{calculate_psnr, calculate_ssim_and_diff, overlay_images};

#[cfg(feature = "ssim")]
use image::ImageFormat;

use color_eyre::Result;

use super::EncodeFuncs;

#[derive(Args, Debug, Clone)]
#[clap(author, about, long_about = None)]
pub struct Avif {
    /// File or directory containing images to convert
    #[clap(value_name = "PATH", required = true)]
    pub path: Vec<PathBuf>,

    /// Enable benchmark mode
    #[clap(
        long,
        default_value_t = false,
        conflicts_with = "name_type",
        conflicts_with = "keep",
        conflicts_with = "output_file",
        global = true
    )]
    pub benchmark: bool,

    #[clap(short, long, conflicts_with = "name_type", value_name = "OUTPUT")]
    pub output_file: Option<PathBuf>,

    /// Send a notification to the desktop when all jobs are finished
    #[clap(short = 'N', long, default_value_t = false)]
    pub notify: bool,

    /// Measure SSIM of encoded vs original image.
    #[cfg(feature = "ssim")]
    #[clap(long = "ssim", default_value_t = false)]
    pub ssim: bool,

    /// Save SSIM difference as an image along with the encoded file.
    #[cfg(feature = "ssim")]
    #[clap(long = "ssim_save", default_value_t = false, requires = "ssim")]
    pub ssim_save: bool,

    /// SSIM window size. Lower values may speed up SSIM calculation, but will sometimes decrease precision.
    #[cfg(feature = "ssim")]
    #[clap(long = "window_size", default_value_t = 8, requires = "ssim")]
    pub window_size: u8,
}

impl EncodeFuncs for Avif {
    fn run_conv(self, globals: &Globals) -> Result<()> {
        let console = ConsoleMsg::new(globals.quiet, self.notify);
        let error_con = ConsoleMsg::new(globals.quiet, self.notify);

        let l_size = self.path.len();

        debug!("Entry Count: {l_size}");
        debug!("Entries: {:?}", &self.path);

        let u = if l_size > 1 || self.path[0].is_dir() {
            self.batch_conv(console, globals)
        } else {
            self.single_file_conv(console, globals)
        };

        if let Err(error) = u {
            error_con.notify_error(&error.to_string())?;
            exit(1);
        }

        Ok(())
    }

    fn batch_conv(self, console: ConsoleMsg, globals: &Globals) -> Result<()> {
        if self.output_file.is_some() {
            bail!("Cannot assign an output file to a directory")
        }

        let mut console = console;
        console.set_spinner("Searching for files...");

        let mut paths = parse_files(&self.path);
        let psize = paths.len();

        paths.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));

        let con = console.finish_spinner(&format!("Found {psize} files."));

        let job_num = calculate_tread_count(globals.threads, psize);

        let pool = ThreadPool::with_name("Encoder Thread".to_string(), job_num.spawn_threads);

        let initial_size: u64 = paths.iter().map(|item| item.metadata.size).sum();

        con.setup_bar(psize as u64);

        let start = Instant::now();

        for mut item in paths.drain(..) {
            let globals = globals.clone();
            pool.execute(move || {
                Globals::set_encoder_priority(globals.priority);
                let enc_start = Instant::now();

                let bar = if globals.quiet {
                    None
                } else {
                    Some(PROGRESS_BAR.clone())
                };

                if let Ok(r_size) = item.convert_to_avif_stored(
                    globals.quality,
                    globals.speed,
                    job_num.task_threads,
                    globals.bit_depth,
                    globals.remove_alpha,
                    bar,
                ) {
                    SUCCESS_COUNT.fetch_add(1, Ordering::SeqCst);
                    FINAL_STATS.fetch_add(r_size, Ordering::SeqCst);
                }

                if !self.benchmark {
                    item.save_avif(None, globals.name_type, globals.keep)
                        .unwrap();
                }

                trace!(
                    "Finished encoding: {} | {:?} | {:?}",
                    item.original_name(),
                    enc_start.elapsed().bold().cyan(),
                    start.elapsed().bold().green()
                );

                drop(item);

                ITEMS_PROCESSED.fetch_add(1, Ordering::SeqCst);

                if globals.quiet {
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
        debug!("Initial size: {initial_size}");

        let initial_delta = FINAL_STATS.load(Ordering::Relaxed) as f32 / initial_size as f32;

        let delta = (initial_delta * 100.) - 100.;

        debug!("Delta: {delta}");

        let percentage = if delta < 0. {
            let st1 = format!("{delta:.2}%");
            format!("{}", st1.green())
        } else {
            let st1 = format!("+{delta:.2}%");
            format!("{}", st1.red())
        };

        let times = {
            let ratio = 1. / initial_delta;
            debug!("Ratio: {ratio}");
            if ratio > 0. {
                let st1 = format!("~{ratio:.1}X smaller");
                format!("{}", st1.green())
            } else {
                let st1 = format!("~{ratio:.1}X bigger");
                format!("{}", st1.red())
            }
        };

        con.print_message(format!(
            "Encoded {} files in {elapsed:.2?}.\n{} {} | {} {} ({} or {})",
            SUCCESS_COUNT.load(Ordering::SeqCst),
            texts[0],
            ByteSize::b(initial_size).blue().bold(),
            texts[1],
            ByteSize::b(FINAL_STATS.load(Ordering::SeqCst))
                .green()
                .bold(),
            percentage,
            times
        ));

        con.notify_text(&format!(
            "Encoded {} files in {elapsed:.2?}\n{} → {}",
            SUCCESS_COUNT.load(Ordering::SeqCst),
            ByteSize::b(initial_size),
            ByteSize::b(FINAL_STATS.load(Ordering::SeqCst))
        ))?;

        Ok(())
    }

    fn single_file_conv(self, console: ConsoleMsg, globals: &Globals) -> Result<()> {
        let mut console = console;
        let mut image = ImageFile::new_from_path(&self.path[0])?;
        let image_size = image.metadata.size;

        console.print_message(format!(
            "Encoding single file {} ({})",
            image.metadata.name.bold(),
            ByteSize::b(image.metadata.size).bold().blue()
        ));

        console.set_spinner("Processing...");

        let start = Instant::now();

        let fsz = image.convert_to_avif_stored(
            globals.quality,
            globals.speed,
            sys_threads(globals.threads),
            globals.bit_depth,
            globals.remove_alpha,
            None,
        )?;

        let ending = start.elapsed();

        let console = console.finish_spinner(&format!(
            "Encoding finished in {:?} ({})",
            ending,
            ByteSize::b(fsz).bold().green()
        ));

        #[cfg(feature = "ssim")]
        if self.ssim {
            let original = &image.bitmap;
            let encoded = &image.get_avif_bitmap();

            let (ssim_scale, diff_image) =
                calculate_ssim_and_diff(original, encoded, globals, self.window_size);

            if self.ssim_save {
                let fdest = if let Some(path) = &self.output_file {
                    if path.is_dir() {
                        path.clone()
                    } else {
                        // unwrap is safe here, as a file path will always have a parent (even if it's `""`)
                        path.parent().unwrap().to_path_buf()
                    }
                } else {
                    let u1 = image.metadata.path.canonicalize()?;
                    u1.parent().unwrap().to_path_buf()
                };

                let overlaid_file_name =
                    fdest.join(format!("overlaid_ws_{}.png", self.window_size));
                let overlaid_image = overlay_images(&original.to_rgba8(), &diff_image, 0.6, 0.4);
                overlaid_image.save_with_format(overlaid_file_name, ImageFormat::Png)?;
            }

            let psnr = calculate_psnr(original, encoded);

            println!("SSIM: {ssim_scale:.2?}");

            let psnr_threshold = 35.0;

            let psnr_display = if psnr < psnr_threshold {
                format!("{}", format!("{psnr:.2} dB").red())
            } else {
                format!("{}", format!("{psnr:.2} dB").green())
            };

            // Display the PSNR value with color coding
            println!("PSNR: {psnr_display}");
        }

        if !self.benchmark {
            image.save_avif(self.output_file, globals.name_type, globals.keep)?;
        }

        let bmp = image.bitmap.clone();

        drop(image);

        console.notify_image(
            &format!(
                "Finished in {:.2?} \n {} → {}",
                ending,
                ByteSize::b(image_size),
                ByteSize::b(fsz)
            ),
            bmp,
        )?;

        Ok(())
    }
}
