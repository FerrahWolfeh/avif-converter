use crate::encoders::avif::encode::Encoder;
use color_eyre::eyre::{bail, Result};
use image::{imageops::overlay, DynamicImage, ImageBuffer, ImageFormat, ImageReader as Reader};
use indicatif::ProgressBar;
use log::debug;
use std::{
    fs::{self, File},
    io::{BufReader, Cursor, Read, Seek},
    path::{Path, PathBuf},
};

use crate::{exif::create_exif_from_png_chunks, name_fun::Name};

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
    pub exif_data: Vec<u8>,
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
            exif_data: vec![],
            bitmap: DynamicImage::new_rgba8(0, 0),
            encoded_data: vec![],
            height: 0,
            width: 0,
            format: ImageFormat::Bmp,
        })
    }

    fn load_image_data_from_reader<R: Read + Seek>(
        &mut self,
        mut reader: R,
        format: ImageFormat,
        remove_alpha: bool,
    ) -> Result<()> {
        if format == ImageFormat::Png {
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer)?;

            let png_reader = png::Decoder::new(Cursor::new(&buffer)).read_info()?;
            let info = png_reader.info();
            if let Some(exif) = create_exif_from_png_chunks(&info.uncompressed_latin1_text)? {
                self.exif_data = exif;
            }
            drop(png_reader);

            let image_data = Reader::with_format(Cursor::new(buffer), ImageFormat::Png);
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

            return Ok(());
        }

        // For non-PNG images, use a generic reader
        let mut image_data = Reader::with_format(BufReader::new(reader), format);

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

    pub fn load_image_data(&mut self, remove_alpha: bool) -> Result<()> {
        let image_file = File::open(&self.metadata.path)?;
        let img_reader = BufReader::new(image_file);
        let format = ImageFormat::from_extension(&self.metadata.extension).unwrap();
        self.load_image_data_from_reader(img_reader, format, remove_alpha)
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

        let mut encoder = Encoder::new()
            .with_num_threads(threads)
            .with_alpha_quality(quality as f32)
            .with_quality(quality as f32)
            .with_speed(speed)
            .with_bit_depth(depth);

        if !self.exif_data.is_empty() {
            encoder = encoder.with_exif_data(self.exif_data.clone());
        }

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

        let target_path = path.clone().map_or(avif_name, |p| {
            if p.is_dir() {
                p.join(format!("{fname}.avif"))
            } else {
                p
            }
        });

        // Always write to a temporary file first for safety.
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp_file_path = fpath.join(format!(".{fname}.tmp"));
        fs::write(&temp_file_path, &self.encoded_data)?;

        // Atomically move the temporary file to the final target path.
        fs::rename(&temp_file_path, &target_path)?;

        if !keep {
            // If not keeping the original, remove it, unless it's the same as the target.
            let canonical_target = target_path
                .canonicalize()
                .unwrap_or_else(|_| target_path.clone());
            if binding != canonical_target {
                fs::remove_file(&binding)?;
            }
        }

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
