use color_eyre::eyre::{bail, Result};
//use dssim_core::Dssim;
use imgref::Img;
use indicatif::ProgressBar;
//use libavif::decode_rgb;
use load_image::{
    export::imgref::{ImgVec, ImgVecKind},
    load_path,
};
use ravif::Encoder;
use rgb::{ComponentMap, RGBA, RGBA8};
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
    pub bitmap: Option<Img<Vec<RGBA<u8>>>>,
    pub avif_data: Vec<u8>,
    pub height: usize,
    pub width: usize,
}

#[derive(Debug, Copy, Clone)]
pub struct ImageOutInfo {
    pub size: u64,
    #[cfg(feature = "ssim")]
    pub ssim: f64,
}

impl ImageFile {
    pub fn load_from_path(path: &Path) -> Result<Self> {
        if let Some(ext) = path.extension() {
            if !(ext == "jpg" || ext == "png" || ext == "jpeg" || ext == "jfif" || ext == "webp") {
                bail!("Unsupported image format");
            }
        } else {
            bail!("Invalid file extension");
        }

        let raw = load_path(path)?.into_imgvec();

        let r2 = Self::load_rgba_data(raw)?;

        let (width, height) = (r2.width(), r2.height());

        Ok(Self {
            path: path.to_path_buf(),
            name: path.file_name().unwrap().to_string_lossy().to_string(),
            size: path.metadata()?.len(),
            bitmap: Some(r2),
            avif_data: vec![],
            height,
            width,
        })
    }

    pub fn convert_to_avif_stored(
        &mut self,
        quality: u8,
        speed: u8,
        threads: usize,
        progress: Option<ProgressBar>,
    ) -> Result<u64> {
        let encoder = Encoder::new()
            .with_num_threads(Some(threads))
            .with_alpha_quality(100.)
            .with_quality(quality as f32)
            .with_speed(speed);

        let encoded_img = encoder.encode_rgba(self.bitmap.as_ref().unwrap().as_ref())?;

        self.avif_data = encoded_img.avif_file;

        if let Some(pb) = progress {
            pb.inc(1);
        }

        Ok(self.avif_data.len() as u64)
    }

    pub fn save_avif(&self, name: Name, keep: bool) -> Result<()> {
        let fname = name.generate_name(&self.avif_data);

        let binding = self.path.canonicalize()?;
        let fpath = binding.parent().unwrap();

        fs::write(fpath.join(format!("{fname}.avif")), &self.avif_data)?;

        if !keep {
            fs::remove_file(&self.path)?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn full_convert(
        &mut self,
        quality: u8,
        speed: u8,
        threads: usize,
        bar: Option<ProgressBar>,
        name: Name,
        keep: bool,
    ) -> Result<ImageOutInfo> {
        let fdata = self.convert_to_avif_stored(quality, speed, threads, bar)?;
        self.save_avif(name, keep)?;

        Ok(ImageOutInfo {
            size: fdata,
            #[cfg(feature = "ssim")]
            ssim: 0.0,
        })
    }

    fn load_rgba_data(data: ImgVecKind) -> Result<ImgVec<RGBA8>> {
        let img = match data {
            load_image::export::imgref::ImgVecKind::RGB8(img) => {
                img.map_buf(|buf| buf.into_iter().map(|px| px.alpha(255)).collect())
            }
            load_image::export::imgref::ImgVecKind::RGBA8(img) => img,
            load_image::export::imgref::ImgVecKind::RGB16(img) => img.map_buf(|buf| {
                buf.into_iter()
                    .map(|px| px.map(|c| (c >> 8) as u8).alpha(255))
                    .collect()
            }),
            load_image::export::imgref::ImgVecKind::RGBA16(img) => img.map_buf(|buf| {
                buf.into_iter()
                    .map(|px| px.map(|c| (c >> 8) as u8))
                    .collect()
            }),
            load_image::export::imgref::ImgVecKind::GRAY8(img) => img.map_buf(|buf| {
                buf.into_iter()
                    .map(|g| {
                        let c = g.0;
                        RGBA8::new(c, c, c, 255)
                    })
                    .collect()
            }),
            load_image::export::imgref::ImgVecKind::GRAY16(img) => img.map_buf(|buf| {
                buf.into_iter()
                    .map(|g| {
                        let c = (g.0 >> 8) as u8;
                        RGBA8::new(c, c, c, 255)
                    })
                    .collect()
            }),
            load_image::export::imgref::ImgVecKind::GRAYA8(img) => img.map_buf(|buf| {
                buf.into_iter()
                    .map(|g| {
                        let c = g.0;
                        RGBA8::new(c, c, c, g.1)
                    })
                    .collect()
            }),
            load_image::export::imgref::ImgVecKind::GRAYA16(img) => img.map_buf(|buf| {
                buf.into_iter()
                    .map(|g| {
                        let c = (g.0 >> 8) as u8;
                        RGBA8::new(c, c, c, (g.1 >> 8) as u8)
                    })
                    .collect()
            }),
        };

        Ok(img)
    }
}
