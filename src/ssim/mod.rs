use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use atomic_float::AtomicF64;
use image::{DynamicImage, GrayImage, Luma, Rgba, RgbaImage};
use rayon::iter::IntoParallelIterator;
use rayon::prelude::*;

use crate::cli::Args as Globals;
use crate::utils::ssim_bar_style;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressFinish};

/// Constants used in SSIM calculation
const K1: f64 = 0.01;
const K2: f64 = 0.03;
const L: f64 = 255.0; // Dynamic range of pixel values

#[inline(always)]
fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

#[inline(always)]
fn variance(values: &[f64], mean: f64) -> f64 {
    values.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / values.len() as f64
}

#[inline(always)]
fn covariance(values1: &[f64], values2: &[f64], mean1: f64, mean2: f64) -> f64 {
    values1
        .iter()
        .zip(values2)
        .map(|(&x1, &x2)| (x1 - mean1) * (x2 - mean2))
        .sum::<f64>()
        / values1.len() as f64
}

pub fn calculate_psnr(img1: &DynamicImage, img2: &DynamicImage) -> f64 {
    let img1 = img1.to_rgb8();
    let img2 = img2.to_rgb8();

    // Ensure images are the same dimensions
    assert_eq!(img1.dimensions(), img2.dimensions());

    let (width, height) = img1.dimensions();

    // Convert images to raw pixel data in parallel
    let img1_pixels: Vec<u8> = img1.into_raw();
    let img2_pixels: Vec<u8> = img2.into_raw();

    // Calculate MSE (Mean Squared Error) in parallel
    let mse: f64 = img1_pixels
        .par_iter()
        .zip(img2_pixels.par_iter())
        .map(|(&p1, &p2)| {
            let diff = p1 as f64 - p2 as f64;
            diff * diff
        })
        .sum::<f64>()
        / (width * height * 3) as f64; // Multiply by 3 for RGB channels

    // Handle edge case where images are identical
    if mse == 0.0 {
        return f64::MAX; // Infinite PSNR
    }

    // Maximum pixel value for 8-bit image is 255
    let max_pixel_value: f64 = 255.0;

    // Compute PSNR using the formula

    10.0 * (max_pixel_value.powi(2) / mse).log10()
}

pub fn calculate_ssim_and_diff(
    img1: &DynamicImage,
    img2: &DynamicImage,
    globals: &Globals,
    win_size: u8,
) -> (f64, RgbaImage) {
    let img1 = img1.to_luma8();
    let img2 = img2.to_luma8();

    assert_eq!(img1.dimensions(), img2.dimensions());

    let (width, height) = img1.dimensions();
    let diff_image = Arc::new(Mutex::new(GrayImage::new(width, height))); // To store the difference image

    let window_size = win_size as u32; // 8x8 window for SSIM calculation

    let c1 = (K1 * L).powi(2);
    let c2 = (K2 * L).powi(2);

    let ssim_total = AtomicF64::new(0.0);
    let window_count = AtomicU32::new(0);

    let bar = ProgressBar::new((height - window_size + 1) as u64).with_style(ssim_bar_style());

    bar.enable_steady_tick(Duration::from_millis(100));

    if globals.quiet {
        bar.finish_and_clear()
    }

    // Parallelize over windows of the image
    (0..height - window_size + 1)
        .into_par_iter()
        .progress_with(bar)
        .with_finish(ProgressFinish::AndClear)
        .for_each(|y| {
            for x in 0..width - window_size + 1 {
                let mut window1 = Vec::new();
                let mut window2 = Vec::new();

                for wy in 0..window_size {
                    for wx in 0..window_size {
                        window1.push(img1.get_pixel(x + wx, y + wy)[0] as f64);
                        window2.push(img2.get_pixel(x + wx, y + wy)[0] as f64);
                    }
                }

                let mean1 = mean(&window1);
                let mean2 = mean(&window2);

                let variance1 = variance(&window1, mean1);
                let variance2 = variance(&window2, mean2);

                let cov = covariance(&window1, &window2, mean1, mean2);

                // SSIM formula for each window
                let ssim = ((2.0 * mean1 * mean2 + c1) * (2.0 * cov + c2))
                    / ((mean1.powi(2) + mean2.powi(2) + c1) * (variance1 + variance2 + c2));

                ssim_total.fetch_add(ssim, Ordering::Relaxed);
                window_count.fetch_add(1, Ordering::Relaxed);

                // Generate difference image for this window
                let diff_value = ((mean1 - mean2).abs() * 255.0).clamp(0.0, L) as u8; // Scale the difference to fit 0-255 range

                // Store the difference in the image
                for wy in 0..window_size {
                    for wx in 0..window_size {
                        let mut diff_img = diff_image.lock().unwrap();
                        diff_img.put_pixel(x + wx, y + wy, Luma([diff_value]));
                    }
                }
            }
        });

    // Compute the final SSIM score (average over all windows)
    let avg_ssim = ssim_total.load(Ordering::Relaxed) / window_count.load(Ordering::Relaxed) as f64;

    // Return the SSIM value and the difference image
    let released_img = diff_image.lock().unwrap();

    let heatmap_image = apply_colormap(&released_img);

    (avg_ssim, heatmap_image)
}

// Example grayscale to RGB colormap (linear heatmap style)
fn grayscale_to_heatmap(value: u8) -> Rgba<u8> {
    let normalized = value as f32 / 255.0; // Clamp between [0.0, 1.0]

    if normalized < 0.5 {
        let ratio = normalized * 2.0; // Scale back between [0.0, 1.0]

        let red = 0;
        let green = (ratio * 255.0) as u8;
        let blue = (255.0 * (1.0 - ratio)) as u8;

        Rgba([blue, green, red, 255])
    } else {
        let ratio = (normalized - 0.5) * 2.0; // This just looks wrong and insane for someone who is bad at math, but here we are again back at [0.0, 1.0]

        let red = (ratio * 255.0) as u8;
        let green = (255.0 * (1.0 - ratio)) as u8;
        let blue = 0;

        Rgba([blue, green, red, 255])
    }
}

// Apply colormap to the difference image
fn apply_colormap(diff_img: &GrayImage) -> RgbaImage {
    let (width, height) = diff_img.dimensions();
    let mut colorized_img = RgbaImage::new(width, height);

    for (x, y, pixel) in diff_img.enumerate_pixels() {
        let gray_value = pixel[0]; // Get grayscale value
        let colored_pixel = grayscale_to_heatmap(gray_value); // Map to color
        colorized_img.put_pixel(x, y, colored_pixel); // Put colored pixel
    }

    colorized_img
}

pub fn overlay_images(img1: &RgbaImage, img2: &RgbaImage, alpha: f32, beta: f32) -> RgbaImage {
    assert_eq!(img1.dimensions(), img2.dimensions());

    let (width, height) = img1.dimensions();
    let mut blended_image = RgbaImage::new(width, height);

    for (x, y, pixel1) in img1.enumerate_pixels() {
        let pixel2 = img2.get_pixel(x, y);

        let blended_pixel = Rgba([
            ((pixel1[0] as f32 * alpha + pixel2[0] as f32 * beta) as u8),
            ((pixel1[1] as f32 * alpha + pixel2[1] as f32 * beta) as u8),
            ((pixel1[2] as f32 * alpha + pixel2[2] as f32 * beta) as u8),
            255, // Keep full opacity for RGBA
        ]);

        blended_image.put_pixel(x, y, blended_pixel);
    }

    blended_image
}
