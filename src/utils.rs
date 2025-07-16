use std::{fmt::Write, fs, path::PathBuf};

use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use once_cell::sync::Lazy;

use crate::image_file::ImageFile;

pub static PROGRESS_BAR: Lazy<ProgressBar> =
    Lazy::new(|| ProgressBar::new(0).with_style(bar_style()));

pub fn parse_files(paths: &[PathBuf]) -> Vec<ImageFile> {
    paths
        .iter()
        .flat_map(|item| {
            if item.is_dir() {
                // If it's a directory, we attempt to read the directory entries
                if let Ok(dir) = fs::read_dir(item) {
                    // Flatten the directory iterator, map each entry to ImageFile, and collect results
                    dir.flatten()
                        .filter_map(|entry| {
                            // Try to create an ImageFile from the entry path
                            ImageFile::new_from_path(&entry.path()).ok()
                        })
                        .collect::<Vec<ImageFile>>() // Collect directory entries into a vector
                } else {
                    Vec::new() // If directory read fails, return an empty Vec
                }
            } else if item.is_file() {
                // If it's a file, try to create an ImageFile from it
                ImageFile::new_from_path(item).ok().into_iter().collect()
            } else {
                Vec::new() // If it's neither a file nor a directory, return an empty Vec
            }
        })
        .collect()
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

#[cfg(feature = "ssim")]
pub fn ssim_bar_style() -> ProgressStyle {
    ProgressStyle::default_bar()
    .template("{spinner:.red.bold} {elapsed_precise:.bold} [{wide_bar:.blue.bold}] {percent:.bold} {pos:.bold} | {win_sec:.bold} (eta. {eta}")
    .unwrap()
    .with_key("pos", |state: &ProgressState, w: &mut dyn Write| {
        write!(w, "{}/{}", state.pos(), state.len().unwrap()).unwrap();
    })
    .with_key("percent", |state: &ProgressState, w: &mut dyn Write| {
        write!(w, "{:>3.0}%", state.fraction() * 100_f32).unwrap();
    })
    .with_key(
        "win_sec",
        |state: &ProgressState, w: &mut dyn Write| match state.per_sec() {
            files_sec if files_sec.abs() < f64::EPSILON => write!(w, "0 windows/s").unwrap(),
              files_sec if files_sec < 1.0 => write!(w, "{:.2} s/window", 1.0 / files_sec).unwrap(),
              files_sec => write!(w, "{files_sec:.2} windows/s").unwrap(),
        },
    )
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

pub fn truncate_str(str: &str, size: usize) -> String {
    assert!(str.len() > 3);

    if str.len() <= size {
        return str.to_string();
    }

    let file_name: Vec<char> = str.chars().rev().collect();
    let file_extension = file_name.iter().position(|c| !c.is_alphanumeric());
    let mut truncated = str[..size - file_extension.unwrap_or(size)].to_string();
    truncated.push_str(
        &file_name[file_extension.unwrap_or(0)..file_extension.unwrap_or(size)]
            .iter()
            .rev()
            .cloned()
            .take(3)
            .collect::<String>(),
    );
    let ext = file_extension
        .unwrap_or(size)
        .to_string()
        .chars()
        .rev()
        .collect::<String>();
    truncated.push_str(&ext);
    truncated.push_str("...");
    truncated
}
