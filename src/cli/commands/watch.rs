use bytesize::ByteSize;
use color_eyre::Result;
use log::{error, info};
use notify::{event::CreateKind, Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};
use threadpool::ThreadPool;

use crate::{
    cli::Args as Globals,
    image_file::ImageFile,
    utils::{sys_threads, truncate_str},
};
use clap::Args;

#[derive(Args, Debug, Clone)]
#[clap(author, about, long_about = None)]
pub struct Watch {
    /// File or directory to watch
    #[clap(value_name = "PATH")]
    pub path: PathBuf,
}

impl Watch {
    pub fn watch_folder(self, globals: &Globals) -> Result<()> {
        info!("Watching {:?}", self.path);

        if let Err(error) = self.watch(globals) {
            error!("Error: {error:?}");
        }
        Ok(())
    }

    fn watch(&self, globals: &Globals) -> notify::Result<()> {
        let pool =
            ThreadPool::with_name("Encoder Thread".to_string(), sys_threads(globals.threads));

        let (tx, rx) = std::sync::mpsc::channel();

        // Create a new debounced file watcher with a timeout of 2 seconds.
        // The tickrate will be selected automatically, as well as the underlying watch implementation.
        let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

        // Add a path to be watched. All files and directories at that path and
        // below will be monitored for changes.
        watcher.watch(&self.path, RecursiveMode::Recursive)?;

        // print all events and errors
        for result in rx {
            match result {
                Ok(events) => {
                    if events.kind == EventKind::Create(CreateKind::File) {
                        info!("Working on files: {:?}", &events.paths);
                        for item in events.paths {
                            let instance = self.clone();
                            let globals = globals.clone();
                            pool.execute(move || {
                                instance.conv_file(&item, &globals).unwrap();
                            })
                        }
                    }
                }
                Err(errors) => log::error!("{errors:?}"),
            }
        }

        Ok(())
    }

    fn conv_file(&self, path: &Path, globals: &Globals) -> Result<()> {
        let mut image = ImageFile::new_from_path(path)?;
        let image_size = image.metadata.size;

        let start = Instant::now();

        let fsz = image.convert_to_avif_stored(
            globals.quality,
            globals.speed,
            1,
            globals.bit_depth,
            globals.remove_alpha,
            None,
        )?;

        image.save_avif(None, globals.name_type, globals.keep)?;

        info!(
            "File '{}' encode finished. {} -> {} ({:?})",
            truncate_str(&image.metadata.filename, 32),
            ByteSize::b(image_size).to_string_as(true),
            ByteSize::b(fsz).to_string_as(true),
            start.elapsed()
        );

        Ok(())
    }
}
