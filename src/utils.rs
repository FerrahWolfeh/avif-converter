use std::{fmt::Write, fs, path::Path, time::Duration};

use color_eyre::Result;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use notify_rust::Notification;
use once_cell::sync::Lazy;
use spinoff::{spinners, Color, Spinner, Streams};

use crate::image_file::ImageFile;

pub static PROGRESS_BAR: Lazy<ProgressBar> =
    Lazy::new(|| ProgressBar::new(0).with_style(bar_style()));

pub struct ConsoleMsg {
    spinner: Option<Spinner>,
    quiet: bool,
    notify: bool,
}

impl ConsoleMsg {
    #[must_use]
    pub fn new(quiet: bool, notify: bool) -> Self {
        Self {
            spinner: None,
            quiet,
            notify,
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

    pub fn setup_bar(&self, len: u64) {
        if !self.quiet {
            PROGRESS_BAR.set_length(len);

            PROGRESS_BAR.enable_steady_tick(Duration::from_millis(100));
        }
    }

    pub fn finish_bar(&self) {
        if !self.quiet {
            PROGRESS_BAR.finish_and_clear();
        }
    }

    pub fn notify_text(&self, message: &str) -> Result<()> {
        if self.notify {
            Notification::new()
                .appname("AVIF Converter")
                .summary("Conversion completed")
                .body(message)
                .icon("folder")
                .show()?;
        }

        Ok(())
    }

    pub fn notify_image(&self, message: &str, image_path: &Path) -> Result<()> {
        if self.notify {
            Notification::new()
                .appname("AVIF Converter")
                .summary("Conversion Completed")
                .body(message)
                .image_path(image_path.as_os_str().to_str().unwrap())
                .show()?;
        }

        Ok(())
    }
}

pub fn search_dir(dir: &Path) -> Vec<ImageFile> {
    let paths = fs::read_dir(dir).unwrap();

    Vec::from_iter(paths.filter_map(|entry| {
        let entry = entry.unwrap();
        let path = entry.path();
        if let Ok(image_file) = ImageFile::new_from_path(&path) {
            return Some(image_file);
        }
        None
    }))
}

pub fn bar_style() -> ProgressStyle {
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

#[derive(Debug, Copy, Clone)]
pub struct ThreadCount {
    pub task_threads: usize,
    pub spawn_threads: usize,
}

pub fn sys_threads(num: usize) -> usize {
    let sel_thread_count = if num > 0 { num } else { num_cpus::get() };

    assert_ne!(sel_thread_count, 0);
    sel_thread_count
}

pub fn calculate_tread_count(num_threads: usize, num_items: usize) -> ThreadCount {
    let sel_thread_count = sys_threads(num_threads);

    let job_per_thread = if num_items >= sel_thread_count {
        1
    } else {
        sel_thread_count / num_items
    };

    ThreadCount {
        task_threads: job_per_thread,
        spawn_threads: sel_thread_count,
    }
}
