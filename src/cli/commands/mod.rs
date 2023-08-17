use clap::Subcommand;

use self::{avif::Avif, watch::Watch};

pub mod avif;
pub mod watch;

#[derive(Debug, Subcommand, Clone)]
pub enum Commands {
    /// Convert images to AVIF format
    Avif(Avif),
    /// Watch directory for new image files and convert them
    Watch(Watch),
}
