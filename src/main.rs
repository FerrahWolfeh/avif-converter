use std::{
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use bytesize::ByteSize;
use clap::Parser;
use color_eyre::eyre::Result;
use image_avif::ImageFile;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use owo_colors::OwoColorize;
use rayon::{prelude::*, ThreadPoolBuilder};
use spinoff::{spinners, Color, Spinner, Streams};

mod image_avif;
mod name_fun;

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

    ThreadPoolBuilder::new()
        .num_threads(thread_num)
        .build_global()
        .unwrap();

    if args.path.is_dir() {
        let spin_collect = Spinner::new_with_stream(
            spinners::Dots,
            "Searching for files...",
            Color::Green,
            Streams::Stderr,
        );

        let paths = search_dir(&args.path);
        let psize = paths.len();

        spin_collect.success(&format!("Found {psize} files."));

        let mut final_stats = Vec::with_capacity(psize);

        let mut global_ctr = 0;

        let initial_size: u64 = paths.iter().map(|item| item.size).sum();

        let progress_bar = ProgressBar::new(paths.len() as u64).with_style(bar_style());

        progress_bar.enable_steady_tick(Duration::from_millis(100));

        let start = Instant::now();

        let threads = if paths.len() >= thread_num {
            1
        } else {
            thread_num / paths.len()
        };

        paths
            .par_iter()
            .with_max_len(1)
            .map(|item| -> Result<u64> {
                let fdata = item.convert_to_avif(
                    args.quality,
                    args.speed,
                    threads,
                    Some(progress_bar.clone()),
                )?;
                item.save_avif(&fdata, args.name_type, args.keep)?;

                Ok(fdata.len() as u64)
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

struct ConsoleMsg {
    spinner: Option<Spinner>,
    quiet: bool,
}

impl ConsoleMsg {
    #[must_use]
    pub fn new(quiet: bool) -> Self {
        Self {
            spinner: None,
            quiet,
        }
    }

    pub fn set_spinner(&mut self, message: &'static str) {
        if !self.quiet {
            let spinner =
                Spinner::new_with_stream(spinners::Dots, message, Color::Green, Streams::Stderr);

            self.spinner = Some(spinner);
        }
    }

    pub fn finish_spinner(mut self, message: &str) -> Self {
        if let Some(spin) = self.spinner {
            spin.success(message);
            self.spinner = None
        }

        self
    }

    pub fn print_message(&self, message: String) {
        if !self.quiet {
            println!("{message}");
        }
    }
}

fn search_dir(dir: &Path) -> Vec<ImageFile> {
    let paths = fs::read_dir(dir).unwrap();

    Vec::from_iter(paths.filter_map(|entry| {
        let entry = entry.unwrap();
        let path = entry.path();
        if let Ok(image_file) = ImageFile::from_path(&path) {
            return Some(image_file);
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
