use image::{GrayImage, Luma};
use rayon::prelude::*;

pub fn calculate_ssim_and_diff(img1: &GrayImage, img2: &GrayImage) -> (f64, GrayImage) {
    assert_eq!(img1.dimensions(), img2.dimensions());

    let (width, height) = img1.dimensions();
    let mut diff_image = GrayImage::new(width, height); // To store the difference image

    let total_ssim: f64 = (0..height)
        .into_par_iter()
        .map(|y| {
            let mut ssim_row_total = 0.0;

            for x in 0..width {
                let p1 = img1.get_pixel(x, y)[0] as f64;
                let p2 = img2.get_pixel(x, y)[0] as f64;

                let mean_p1 = p1;
                let mean_p2 = p2;

                let variance_p1 = p1 * p1;
                let variance_p2 = p2 * p2;

                let covariance = p1 * p2;

                let c1 = 0.01 * 255.0; // Constants to stabilize division
                let c2 = 0.03 * 255.0;

                let ssim = ((2.0 * mean_p1 * mean_p2 + c1) * (2.0 * covariance + c2))
                    / ((mean_p1.powi(2) + mean_p2.powi(2) + c1) * (variance_p1 + variance_p2 + c2));

                // Add SSIM value to the total
                ssim_row_total += ssim;

                // Generate difference image by scaling the absolute difference
                let diff_value = ((p1 - p2).abs() * 255.0 / 255.0) as u8; // Scale the difference to fit 0-255 range
                diff_image.put_pixel(x, y, Luma([diff_value])); // Store difference in diff image
            }

            ssim_row_total
        })
        .sum(); // Sum all the rows' SSIM totals in parallel

    // Compute the final SSIM score (average over all pixels)
    let avg_ssim = total_ssim / (width * height) as f64;

    (avg_ssim, diff_image)
}
