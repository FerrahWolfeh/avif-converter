use std::path::PathBuf;

use clap::Parser;

use crate::name_fun::Name;

mod commands;

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
}
