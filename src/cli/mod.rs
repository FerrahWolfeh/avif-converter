use log::{debug, error};
use std::sync::atomic::AtomicU64;
use thread_priority::{set_current_thread_priority, ThreadPriority, ThreadPriorityValue};

use clap::{Parser, ValueEnum};

use crate::name_fun::Name;
use color_eyre::eyre::Result;

use self::commands::Commands;

pub mod commands;

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
    #[command(subcommand)]
    pub command: Commands,

    #[clap(
        short,
        long,
        default_value_t = 70,
        value_name = "QUALITY",
        global = true
    )]
    pub quality: u8,

    #[clap(short, long, default_value_t = 4, value_name = "SPEED", global = true)]
    pub speed: u8,

    #[clap(short, long, value_enum, default_value_t = Name::MD5, global = true)]
    pub name_type: Name,

    /// Encoded image bit depth.
    #[clap(
        short = 'd',
        long,
        default_value_t = 10,
        value_parser(bit_values),
        global = true
    )]
    pub bit_depth: u8,

    /// Defaults to number of CPU cores. Use 0 for all cores
    #[clap(
        short,
        long,
        default_value_t = 0,
        value_name = "THREADS",
        global = true
    )]
    pub threads: usize,

    /// How many images to keep in memory at once
    #[clap(short, long)]
    pub batch_size: Option<usize>,

    /// Supress console messages
    #[clap(long, default_value_t = false, global = true)]
    pub quiet: bool,

    /// Keep original file
    #[clap(short, long, default_value_t = false, global = true)]
    pub keep: bool,

    #[clap(long, default_value_t = false, global = true)]
    pub remove_alpha: bool,

    /// Set encoder threads priority
    #[clap(short, long, value_enum, default_value_t = ThreadNice::Default, global = true)]
    pub priority: ThreadNice,
}

#[derive(Debug, Copy, Clone, ValueEnum, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ThreadNice {
    Max = 0,
    Min = 99,
    Default = 55,
}

impl Args {
    pub fn init() -> Self {
        Self::parse()
    }

    fn set_encoder_priority(thread_level: ThreadNice) {
        let thread_response = ThreadPriorityValue::try_from(thread_level as u8).unwrap();

        if set_current_thread_priority(ThreadPriority::Crossplatform(thread_response)).is_ok() {
            debug!("Thread priority set to {thread_response:?}");
        } else {
            error!("Failed to set thread priority. Leaving as default")
        }
    }
}
