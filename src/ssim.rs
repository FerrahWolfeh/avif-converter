use color_eyre::eyre::Result;
use dssim_core::Dssim;
use indicatif::ProgressBar;
use libavif::decode_rgb;
use rgb::FromSlice;

use crate::{
    image_avif::{ImageFile, ImageOutInfo},
    name_fun::Name,
};

pub trait CalculateSSIM {
    fn calculate_ssim(&self) -> Result<f64>;

    #[allow(clippy::too_many_arguments)]
    fn full_convert_ssim(
        &mut self,
        quality: u8,
        speed: u8,
        threads: usize,
        bar: Option<ProgressBar>,
        name: Name,
        keep: bool,
        ssim: bool,
    ) -> Result<ImageOutInfo>;
}

impl CalculateSSIM for ImageFile {
    fn calculate_ssim(&self) -> Result<f64> {
        let binding = self.bitmap.clone().unwrap();
        let og_img = binding.into_buf();

        let image_original = Dssim::new()
            .create_image_rgba(&og_img, self.width, self.height)
            .unwrap();

        let avif_pix_data = decode_rgb(&self.avif_data)?;

        let image_new_dssim = Dssim::new()
            .create_image_rgba(avif_pix_data.as_rgba(), self.width, self.height)
            .unwrap();

        let ssim: f64 = Dssim::new()
            .compare(&image_original, image_new_dssim)
            .0
            .into();

        Ok(ssim)
    }

    #[allow(clippy::too_many_arguments)]
    fn full_convert_ssim(
        &mut self,
        quality: u8,
        speed: u8,
        threads: usize,
        bar: Option<ProgressBar>,
        name: Name,
        keep: bool,
        ssim: bool,
    ) -> Result<ImageOutInfo> {
        let fdata = self.convert_to_avif_stored(quality, speed, threads, bar)?;
        self.save_avif(name, keep)?;

        let ssim = if ssim { self.calculate_ssim()? } else { 0.0 };

        Ok(ImageOutInfo { size: fdata, ssim })
    }
}
