use crate::encoders::avif::encode::Encoder;
use color_eyre::eyre::{bail, Result};
use image::{io::Reader, DynamicImage, ImageFormat};
use imgref::Img;
use indicatif::ProgressBar;
use rgb::FromSlice;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::name_fun::Name;

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub path: PathBuf,
    pub filename: String,
    pub name: String,
    pub extension: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct ImageFile {
    pub metadata: FileMetadata,
    pub format: ImageFormat,
    pub has_alpha: bool,
    pub bitmap: DynamicImage,
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
            metadata: FileMetadata {
                path: path.to_path_buf(),
                filename: path.file_name().unwrap().to_string_lossy().to_string(),
                name: path.file_stem().unwrap().to_string_lossy().to_string(),
                extension: path.extension().unwrap().to_string_lossy().to_string(),
                size: path.metadata()?.len(),
            },
            bitmap: DynamicImage::new_rgba8(0, 0),
            has_alpha: false,
            encoded_data: vec![],
            height: 0,
            width: 0,
            format: ImageFormat::Bmp,
        })
    }

    pub fn load_image_data(&mut self) -> Result<()> {
        let mut image_data = Reader::open(&self.metadata.path)?;

        let format = ImageFormat::from_extension(&self.metadata.extension).unwrap();

        image_data.set_format(format);

        let raw_image = image_data.decode()?;

        let (width, height) = (raw_image.width(), raw_image.height());

        self.bitmap = raw_image;
        self.format = format;
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
        if self.bitmap.as_bytes().is_empty() {
            self.load_image_data()?;
        }

        assert!(!self.bitmap.as_bytes().is_empty());

        let encoder = Encoder::new()
            .with_num_threads(threads)
            .with_alpha_quality(100.)
            .with_quality(quality as f32)
            .with_speed(speed);

        if self.has_alpha {
            let bmp = self.bitmap.to_rgba8();

            let encoded_img = encoder.encode_rgba(Img::new(
                bmp.as_rgba(),
                self.width as usize,
                self.height as usize,
            ))?;

            self.encoded_data = encoded_img.avif_file;
        } else {
            let bmp = self.bitmap.to_rgb8();

            let encoded_img = encoder.encode_rgb(Img::new(
                bmp.as_rgb(),
                self.width as usize,
                self.height as usize,
            ))?;

            self.encoded_data = encoded_img.avif_file;
        }

        if let Some(pb) = progress {
            pb.inc(1);
        }

        Ok(self.encoded_data.len() as u64)
    }

    pub fn save_avif(&self, name: Name, keep: bool) -> Result<()> {
        let fname = name.generate_name(self);

        let binding = self.metadata.path.canonicalize()?;
        let fpath = binding.parent().unwrap();

        fs::write(fpath.join(format!("{fname}.avif")), &self.encoded_data)?;

        if !keep {
            fs::remove_file(&self.metadata.path)?;
        }

        Ok(())
    }

    pub fn original_name(&self) -> String {
        self.metadata.filename.clone()
    }
}