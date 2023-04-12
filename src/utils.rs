use std::{fmt::Write, fs, path::Path};

use indicatif::{ProgressState, ProgressStyle};
use spinoff::{spinners, Color, Spinner, Streams};

use crate::image_avif::ImageFile;

pub struct ConsoleMsg {
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
