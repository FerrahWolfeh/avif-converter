use clap::Subcommand;

pub mod avif;

#[derive(Debug, Subcommand)]
pub enum Commands {
    Avif,
    Convert,
}
