use crate::encoders::avif::encode::Encoder;
use color_eyre::eyre::{bail, Result};
use image::{imageops::overlay, DynamicImage, ImageBuffer, ImageFormat, ImageReader as Reader};
use indicatif::ProgressBar;
use log::debug;
use std::{
    fs::{self, OpenOptions},
    io::{Seek, Write},
    path::{Path, PathBuf},
};

#[cfg(feature = "ssim")]
use std::io::Cursor;

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
    pub bitmap: DynamicImage,
    pub encoded_data: Vec<u8>,
    pub height: u32,
    pub width: u32,
}

impl ImageFile {
    pub fn new_from_path(path: &Path) -> Result<Self> {
        debug!("Initializing file {path:?}");
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if !(ext == "jpg"
                || ext == "png"
                || ext == "jpeg"
                || ext == "jfif"
                || ext == "webp"
                || ext == "bmp"
                || ext == "avif")
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
            encoded_data: vec![],
            height: 0,
            width: 0,
            format: ImageFormat::Bmp,
        })
    }

    pub fn load_image_data(&mut self, remove_alpha: bool) -> Result<()> {
        let mut image_data = Reader::open(&self.metadata.path)?;

        let format = ImageFormat::from_extension(&self.metadata.extension).unwrap();

        image_data.set_format(format);

        let mut raw_image = image_data.decode()?;

        let (width, height) = (raw_image.width(), raw_image.height());

        if width < 32 {
            bail!("Image width too small for encode!")
        }

        if remove_alpha && raw_image.color().has_alpha() {
            debug!("Replacing transparent pixels with black");
            let mut black_square = ImageBuffer::new(width, height);

            for (_, _, pixel) in black_square.enumerate_pixels_mut() {
                *pixel = image::Rgba([0, 0, 0, 255]);
            }

            overlay(&mut black_square, &raw_image, 0, 0);

            raw_image = DynamicImage::ImageRgba8(black_square);
        }

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
        depth: u8,
        remove_alpha: bool,
        progress: Option<ProgressBar>,
    ) -> Result<u64> {
        if self.bitmap.as_bytes().is_empty() {
            self.load_image_data(remove_alpha)?;
        }

        assert!(!self.bitmap.as_bytes().is_empty());

        let encoder = Encoder::new()
            .with_num_threads(threads)
            .with_alpha_quality(quality as f32)
            .with_quality(quality as f32)
            .with_speed(speed)
            .with_bit_depth(depth);

        encoder.encode(self)?;

        if let Some(pb) = progress {
            pb.inc(1);
        }

        Ok(self.encoded_data.len() as u64)
    }

    pub fn save_avif(&self, path: Option<PathBuf>, name: Name, keep: bool) -> Result<()> {
        let fname = name.generate_name(self);

        let binding = self.metadata.path.canonicalize()?;
        let fpath = binding.parent().unwrap();

        let avif_name = fpath.join(format!("{fname}.avif"));

        // If `path` is Some, save to the provided path
        if let Some(new_path) = path {
            let target_avif_name = new_path.join(format!("{fname}.avif"));

            if !keep {
                // If `keep` is false and we have a target path, we want to replace the original file
                let mut orig_file = OpenOptions::new().write(true).open(&binding)?;
                orig_file.set_len(self.encoded_data.len() as u64)?;
                orig_file.seek(std::io::SeekFrom::Start(0))?;
                orig_file.write_all(&self.encoded_data)?;

                // Attempt to rename (move) to the new path
                match fs::rename(&binding, &target_avif_name) {
                    Ok(_) => return Ok(()), // Success, file moved
                    Err(_) => {
                        // Rename failed (likely due to different filesystems), fallback to copy+delete
                        fs::copy(&binding, &target_avif_name)?;
                        fs::remove_file(&binding)?; // Remove original after successful copy
                    }
                }

                return Ok(());
            }

            // If `keep` is true, just save the AVIF to the target location
            fs::write(&target_avif_name, &self.encoded_data)?;

            return Ok(());
        }

        // If no `path` is provided, proceed with in-place modifications
        if !keep {
            let mut orig_file = OpenOptions::new().write(true).open(&binding)?;
            orig_file.set_len(self.encoded_data.len() as u64)?;
            orig_file.seek(std::io::SeekFrom::Start(0))?;
            orig_file.write_all(&self.encoded_data)?;

            // Rename (move) the file to the new AVIF name
            fs::rename(&binding, &avif_name)?;

            return Ok(());
        }

        // If `keep` is true, save AVIF to the same directory
        fs::write(&avif_name, &self.encoded_data)?;

        Ok(())
    }

    pub fn original_name(&self) -> String {
        self.metadata.filename.clone()
    }

    #[cfg(feature = "ssim")]
    pub fn get_avif_bitmap(&self) -> DynamicImage {
        let mut image_data = Reader::new(Cursor::new(&self.encoded_data));
        let format = ImageFormat::Avif;

        image_data.set_format(format);

        image_data.decode().unwrap()
    }
}
