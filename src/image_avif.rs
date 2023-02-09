use color_eyre::eyre::{bail, Result};
use dssim_core::Dssim;
use indicatif::ProgressBar;
use ravif::{Encoder, Img};
use rgb::{FromSlice, RGBA};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::name_fun::Name;

#[derive(Debug, Clone)]
pub struct ImageFile {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub bitmap: Vec<RGBA<u8>>,
    height: u32,
    width: u32,
}

impl ImageFile {
    pub fn from_path(path: &Path) -> Result<Self> {
        if let Some(ext) = path.extension() {
            if !(ext == "jpg" || ext == "png" || ext == "jpeg" || ext == "jfif" || ext == "webp") {
                bail!("Unsupported image format");
            }
        } else {
            bail!("Invalid file extension");
        }

        Ok(Self {
            path: path.to_path_buf(),
            name: path.file_name().unwrap().to_string_lossy().to_string(),
            size: path.metadata()?.len(),
            bitmap: vec![],
            height: 0,
            width: 0,
        })
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        if let Some(ext) = path.extension() {
            if !(ext == "jpg" || ext == "png" || ext == "jpeg" || ext == "jfif" || ext == "webp") {
                bail!("Unsupported image format");
            }
        } else {
            bail!("Invalid file extension");
        }

        let raw = image::open(path)?.to_rgba8();

        let conv = raw.as_rgba();

        Ok(Self {
            path: path.to_path_buf(),
            name: path.file_name().unwrap().to_string_lossy().to_string(),
            size: path.metadata()?.len(),
            bitmap: conv.to_vec(),
            height: raw.height(),
            width: raw.width(),
        })
    }

    pub fn convert_to_avif_stored(
        &self,
        quality: u8,
        speed: u8,
        threads: usize,
        progress: Option<ProgressBar>,
    ) -> Result<Vec<u8>> {
        let encodable_img = Img::new(
            self.bitmap.as_slice(),
            self.width as usize,
            self.height as usize,
        );

        let encoder = Encoder::new()
            .with_num_threads(Some(threads))
            .with_alpha_quality(100.)
            .with_quality(quality as f32)
            .with_speed(speed);

        let encoded_img = encoder.encode_rgba(encodable_img)?;

        let avif = encoded_img.avif_file;

        if let Some(pb) = progress {
            pb.inc(1);
        }

        Ok(avif)
    }

    pub fn convert_to_avif(
        &self,
        quality: u8,
        speed: u8,
        threads: usize,
        progress: Option<ProgressBar>,
    ) -> Result<Vec<u8>> {
        let raw_img = image::open(&self.path)?;

        let (width, height) = (raw_img.width(), raw_img.height());

        let binding = raw_img.to_rgba8();

        let encodable_img = Img::new(binding.as_rgba(), width as usize, height as usize);

        let encoder = Encoder::new()
            .with_num_threads(Some(threads))
            .with_alpha_quality(100.)
            .with_quality(quality as f32)
            .with_speed(speed);

        let encoded_img = encoder.encode_rgba(encodable_img)?;

        let avif = encoded_img.avif_file;

        if let Some(pb) = progress {
            pb.inc(1);
        }

        Ok(avif)
    }

    pub fn calculate_ssim(&self, avif_image: &[u8]) -> Result<f64> {
        let image_original = Dssim::new()
            .create_image_rgba(&self.bitmap, self.width as usize, self.height as usize)
            .unwrap();

        let encoded = image::load_from_memory_with_format(avif_image, image::ImageFormat::Avif)?;

        let image_new_dssim = Dssim::new()
            .create_image_rgba(
                encoded.to_rgba8().as_rgba(),
                encoded.width() as usize,
                encoded.height() as usize,
            )
            .unwrap();

        let ssim: f64 = Dssim::new()
            .compare(&image_original, image_new_dssim)
            .0
            .into();

        Ok(ssim)
    }

    pub fn save_avif(&self, avif_image: &[u8], name: Name, keep: bool) -> Result<()> {
        let fname = name.generate_name(avif_image);

        let binding = self.path.canonicalize()?;
        let fpath = binding.parent().unwrap();

        fs::write(fpath.join(format!("{fname}.avif")), avif_image)?;

        if !keep {
            fs::remove_file(&self.path)?;
        }

        Ok(())
    }

    pub fn full_convert(
        self,
        quality: u8,
        speed: u8,
        threads: usize,
        bar: Option<ProgressBar>,
        name: Name,
        keep: bool,
    ) -> Result<u64> {
        let fdata = self.convert_to_avif(quality, speed, threads, bar)?;
        self.save_avif(&fdata, name, keep)?;

        Ok(fdata.len() as u64)
    }
}
