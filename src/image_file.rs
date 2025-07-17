use crate::encoders::avif::encode::Encoder;
use color_eyre::eyre::{bail, Result};
use exif::Tag;
use image::{imageops::overlay, DynamicImage, ImageBuffer, ImageFormat, ImageReader as Reader};
use indicatif::ProgressBar;
use log::debug;
use png::text_metadata::TEXtChunk;
use std::{
    fs::{self, File},
    io::{BufReader, Seek},
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

    pub fn extract_png_metadata(&mut self, text_chunks: &[TEXtChunk]) -> Result<()> {
        let mut entries: Vec<(Tag, Vec<u8>)> = Vec::new();

        for chunk in text_chunks {
            match chunk.keyword.as_str() {
                "prompt" => {
                    entries.push((Tag::Make, format!("Prompt: {}", chunk.text).into_bytes()));
                }
                "workflow" => {
                    entries.push((
                        Tag::ImageDescription,
                        format!("Workflow: {}", chunk.text).into_bytes(),
                    ));
                }
                _ => {
                    // Store other chunks as UserComment
                    let comment = format!("{}: {}", chunk.keyword, chunk.text);
                    entries.push((Tag::UserComment, comment.into_bytes()));
                }
            }
        }

        if entries.is_empty() {
            return Ok(());
        }

        let mut exif_data: Vec<u8> = Vec::new();

        // 1. TIFF Header
        exif_data.extend_from_slice(&[0x4D, 0x4D, 0x00, 0x2A, 0x00, 0x00, 0x00, 0x08]); // Little-Endian, Offset to IFD

        // 2. IFD
        let num_ifd_entries = entries.len() as u16;
        exif_data.extend_from_slice(&num_ifd_entries.to_be_bytes());

        // Calculate the base offset for the tag values (after the IFD entries and next IFD offset)
        let value_offset_base = 8 + 2 + (12 * num_ifd_entries as usize) + 4;

        let mut current_value_offset = value_offset_base;

        for (tag, value) in &entries {
            // Entry: Tag, Type, Count, ValueOffset
            exif_data.extend_from_slice(&tag.1.to_be_bytes()); // Tag
            exif_data.extend_from_slice(&[0x00, 0x02]); // Type: ASCII (0x0002)
            exif_data.extend_from_slice(&(value.len() as u32 + 1).to_be_bytes()); // Count (String length + null terminator)
            exif_data.extend_from_slice(&(current_value_offset as u32).to_be_bytes()); // ValueOffset
            current_value_offset += value.len() + 1;
        }

        exif_data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Next IFD offset (0 for none)

        // 3. Tag Values
        for (_, value) in &entries {
            exif_data.extend_from_slice(value);
            exif_data.push(0x00); // Null terminator
        }

        // 4. EXIF Header
        let mut exif_header: Vec<u8> = vec![0x45, 0x78, 0x69, 0x66, 0x00, 0x00]; // "Exif\0\0"
        let len_bytes = 6u32.to_be_bytes(); // Length of "Exif\0\0" + offset

        exif_header.extend(exif_data);
        exif_header.splice(0..0, len_bytes.iter().copied());

        self.exif_data = exif_header;
        Ok(())
    }

    fn load_image_data_from_reader<R: std::io::Read + Seek>(
        &mut self,
        reader: R,
        format: ImageFormat,
        remove_alpha: bool,
    ) -> Result<()> {
        if format == ImageFormat::Png {
            let png_reader = png::Decoder::new(reader).read_info()?;
            let info = png_reader.info();
            self.extract_png_metadata(&info.uncompressed_latin1_text)?;

            drop(png_reader);

            let img_reader = BufReader::new(File::open(self.metadata.path.to_str().unwrap())?);

            let image_data = Reader::with_format(img_reader, ImageFormat::Png);
            let mut raw_image = image_data.decode()?;

            // The rest of the PNG-specific handling remains the same...

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

        let target_path = path.clone().map_or(avif_name.clone(), |p| {
            if p.is_dir() {
                p.join(format!("{fname}.avif"))
            } else {
                p
            }
        });

        if !keep {
            // Prepare target directory if needed
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Write the encoded data to a temporary file in the same directory as the original
            let temp_file_path = fpath.join(format!(".{fname}.tmp"));
            fs::write(&temp_file_path, &self.encoded_data)?;

            // Atomically replace the original file with the temporary file
            fs::rename(&temp_file_path, &target_path)?;

            if path.is_none() {
                // If no output path was specified, also rename the original file to .avif
                fs::rename(&binding, &avif_name)?;
            }
            return Ok(());
        }

        // For `keep` == true, just write the encoded data to the target path
        fs::write(&target_path, &self.encoded_data)?;
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
