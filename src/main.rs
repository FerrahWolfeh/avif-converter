#![feature(portable_simd)]
use cli::Args;
use color_eyre::eyre::Result;

use utils::{search_dir, ConsoleMsg};

mod cli;
mod encoders;
mod image_file;
mod name_fun;
mod utils;

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::builder().format_timestamp(None).init();
    let args: Args = Args::init();

    args.run_conv()?;

    Ok(())
}
