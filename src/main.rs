#![feature(portable_simd)]
use cli::{commands::Commands, Args};
use color_eyre::eyre::Result;

mod cli;
mod console;
mod encoders;
mod image_file;
mod name_fun;
mod utils;

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::builder().format_timestamp(None).init();
    let args: Args = Args::init();
    let globals = args.clone(); // Inneficient as fuck but whatever

    match args.command {
        Commands::Avif(dtd) => dtd.run_conv(&globals),
        Commands::Watch(dtd) => dtd.watch_folder(&globals),
    }
}
