use color_eyre::eyre::{bail, Result};
use image::{io::Reader, ImageFormat, RgbaImage};
use imgref::Img;
use indicatif::ProgressBar;
use ravif::Encoder;
use rgb::FromSlice;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::name_fun::Name;

#[derive(Debug, Clone)]
pub struct ImageFile {
    pub path: PathBuf,
    pub filename: String,
    pub name: String,
    pub extension: String,
    pub size: u64,
    pub bitmap: RgbaImage,
    pub encoded_data: Vec<u8>,
    pub height: u32,
    pub width: u32,
}

impl ImageFile {
    pub fn new_from_path(path: &Path) -> Result<Self> {
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if !(ext == "jpg"
                || ext == "png"
                || ext == "jpeg"
                || ext == "jfif"
                || ext == "webp"
                || ext == "bmp")
            {
                bail!("Unsupported image format");
            }
        } else {
            bail!("Invalid file extension");
        }

        Ok(Self {
            path: path.to_path_buf(),
            filename: path.file_name().unwrap().to_string_lossy().to_string(),
            name: path.file_stem().unwrap().to_string_lossy().to_string(),
            extension: path.extension().unwrap().to_string_lossy().to_string(),
            size: path.metadata()?.len(),
            bitmap: RgbaImage::new(0, 0),
            encoded_data: vec![],
            height: 0,
            width: 0,
        })
    }

    pub fn load_image_data(&mut self) -> Result<()> {
        let mut image_data = Reader::open(&self.path)?;

        image_data.set_format(ImageFormat::from_extension(&self.extension).unwrap());

        let raw_image = image_data.decode()?;
        let rgb_data = raw_image.to_rgba8();

        let (width, height) = (raw_image.width(), raw_image.height());

        self.bitmap = rgb_data;
        self.width = width;
        self.height = height;

        Ok(())
    }

    pub fn convert_to_avif_stored(
        &mut self,
        quality: u8,
        speed: u8,
        threads: usize,
        progress: Option<ProgressBar>,
    ) -> Result<u64> {
        if self.bitmap.is_empty() {
            self.load_image_data()?;
        }

        assert!(!self.bitmap.is_empty());

        let encoder = Encoder::new()
            .with_num_threads(Some(threads))
            .with_alpha_quality(100.)
            .with_quality(quality as f32)
            .with_speed(speed);

        let encoded_img = encoder.encode_rgba(Img::new(
            self.bitmap.as_rgba(),
            self.width as usize,
            self.height as usize,
        ))?;

        self.encoded_data = encoded_img.avif_file;

        if let Some(pb) = progress {
            pb.inc(1);
        }

        Ok(self.encoded_data.len() as u64)
    }

    pub fn save_avif(&self, name: Name, keep: bool) -> Result<()> {
        let fname = name.generate_name(self);

        let binding = self.path.canonicalize()?;
        let fpath = binding.parent().unwrap();

        fs::write(fpath.join(format!("{fname}.avif")), &self.encoded_data)?;

        if !keep {
            fs::remove_file(&self.path)?;
        }

        Ok(())
    }

    pub fn original_name(&self) -> String {
        self.filename.clone()
    }
}
