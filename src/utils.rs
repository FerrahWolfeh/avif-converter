use std::{fmt::Write, fs, path::Path};

use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use once_cell::sync::Lazy;

use crate::image_file::ImageFile;

pub static PROGRESS_BAR: Lazy<ProgressBar> =
    Lazy::new(|| ProgressBar::new(0).with_style(bar_style()));

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

#[derive(Debug, Copy, Clone)]
pub struct ThreadCount {
    pub task_threads: usize,
    pub spawn_threads: usize,
}

pub fn sys_threads(num: usize) -> usize {
    let sel_thread_count = if num > 0 { num } else { num_cpus::get() };

    assert_ne!(sel_thread_count, 0);
    sel_thread_count
}

pub fn calculate_tread_count(num_threads: usize, num_items: usize) -> ThreadCount {
    let sel_thread_count = sys_threads(num_threads);

    let job_per_thread = if num_items >= sel_thread_count {
        1
    } else {
        sel_thread_count / num_items
    };

    ThreadCount {
        task_threads: job_per_thread,
        spawn_threads: sel_thread_count,
    }
}
