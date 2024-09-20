use clap::Subcommand;

use crate::console::ConsoleMsg;

use self::{avif::Avif, watch::Watch};

use super::Args as Globals;
use color_eyre::Result;

pub mod avif;
//pub mod png;
pub mod watch;

#[derive(Debug, Subcommand, Clone)]
pub enum Commands {
    /// Convert images to AVIF format
    Avif(Avif),
    /// Watch directory for new image files and convert them
    Watch(Watch),
}

pub trait EncodeFuncs {
    fn run_conv(self, globals: &Globals) -> Result<()>;

    fn batch_conv(self, console: ConsoleMsg, globals: &Globals) -> Result<()>;

    fn single_file_conv(self, console: ConsoleMsg, globals: &Globals) -> Result<()>;
}
