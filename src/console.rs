use std::time::Duration;

use color_eyre::Result;
use image::{imageops::FilterType, DynamicImage};
use notify_rust::{Image, Notification};
use spinoff::{spinners, Color, Spinner, Streams};

use crate::utils::PROGRESS_BAR;

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

    pub fn notify_image(&self, message: &str, image: DynamicImage) -> Result<()> {
        let img = image.resize(512, 512, FilterType::Nearest);

        if self.notify {
            Notification::new()
                .appname("AVIF Converter")
                .summary("Conversion Completed")
                .body(message)
                .image_data(Image::from_rgba(
                    img.width() as i32,
                    img.height() as i32,
                    img.to_rgba8().into_vec(),
                )?)
                .show()?;
        }

        Ok(())
    }

    pub fn notify_error(&self, message: &str) -> Result<()> {
        if self.notify {
            Notification::new()
                .appname("AVIF Converter")
                .summary("Conversion Failed")
                .body(message)
                .show()?;
        }

        Ok(())
    }
}
