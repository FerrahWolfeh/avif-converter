use std::simd::u8x32;
use std::time::Instant;

use color_eyre::eyre::Result;
use imgref::Img;
use log::{debug, trace};
use rav1e::prelude::*;
use rgb::{FromSlice, RGB8, RGBA};

use crate::image_file::ImageFile;

use super::alpha::blurred_dirty_alpha;
use super::error::Error;

/// The newly-created image file + extra info FYI
#[non_exhaustive]
#[derive(Clone)]
pub struct EncodedImage {
    /// AVIF (HEIF+AV1) encoded image data
    pub avif_file: Vec<u8>,
    /// FYI: number of bytes of AV1 payload used for the color
    pub color_byte_size: usize,
    /// FYI: number of bytes of AV1 payload used for the alpha channel
    pub alpha_byte_size: usize,
}

/// Encoder config builder
#[derive(Debug, Clone)]
pub struct Encoder {
    /// 0-255 scale
    quantizer: u8,
    /// 0-255 scale
    alpha_quantizer: u8,
    /// rav1e preset 1 (slow) 10 (fast but crappy)
    speed: u8,
    /// How many threads should be used (0 = match core count), None - use global rayon thread pool
    threads: usize,
}

/// Builder methods
impl Encoder {
    /// Start here
    #[must_use]
    pub fn new() -> Self {
        Self {
            quantizer: quality_to_quantizer(80.),
            alpha_quantizer: quality_to_quantizer(80.),
            speed: 5,
            threads: num_cpus::get(),
        }
    }

    /// Quality `1..=100`. Panics if out of range.
    #[inline(always)]
    #[track_caller]
    #[must_use]
    pub fn with_quality(mut self, quality: f32) -> Self {
        assert!((1. ..=100.).contains(&quality));
        self.quantizer = quality_to_quantizer(quality);
        self
    }

    /// Quality for the alpha channel only. `1..=100`. Panics if out of range.
    #[inline(always)]
    #[track_caller]
    #[must_use]
    pub fn with_alpha_quality(mut self, quality: f32) -> Self {
        assert!((1. ..=100.).contains(&quality));
        self.alpha_quantizer = quality_to_quantizer(quality);
        self
    }

    /// `1..=10`. 1 = very very slow, but max compression.
    /// 10 = quick, but larger file sizes and lower quality.
    #[inline(always)]
    #[track_caller]
    #[must_use]
    pub fn with_speed(mut self, speed: u8) -> Self {
        assert!((1..=10).contains(&speed));
        self.speed = speed;
        self
    }

    /// Configures `rayon` thread pool size.
    /// The default `None` is to use all threads in the default `rayon` thread pool.
    #[inline(always)]
    #[track_caller]
    #[must_use]
    pub fn with_num_threads(mut self, num_threads: usize) -> Self {
        self.threads = num_threads;
        self
    }
}

/// Once done with config, call one of the `encode_*` functions
impl Encoder {
    /// Make a new AVIF image from RGBA pixels (non-premultiplied, alpha last)
    ///
    /// Make the `Img` for the `buffer` like this:
    ///
    /// ```rust,ignore
    /// Img::new(&pixels_rgba[..], width, height)
    /// ```
    ///
    /// If you have pixels as `u8` slice, then first do:
    ///
    /// ```rust,ignore
    /// use rgb::ComponentSlice;
    /// let pixels_rgba = pixels_u8.as_rgba();
    /// ```
    ///
    /// If all pixels are opaque, the alpha channel will be left out automatically.
    ///
    /// This function takes 8-bit inputs, but will generate an AVIF file using 10-bit depth.
    ///
    /// returns AVIF file with info about sizes about AV1 payload.
    fn encode_rgba(&self, in_buffer: Img<&[RGBA<u8>]>) -> Result<EncodedImage> {
        let new_alpha = blurred_dirty_alpha(in_buffer);
        let buffer = new_alpha.as_ref().map(|b| b.as_ref()).unwrap_or(in_buffer);

        let width = buffer.width();
        let height = buffer.height();
        let planes = buffer.pixels().map(|px| rgb_to_8_bit_ycbcr(px.rgb()));
        let alpha = buffer.pixels().map(|px| px.a);
        self.encode_raw_planes_8_bit(width, height, planes, Some(alpha), PixelRange::Full)
    }

    pub fn encode(&self, image: &mut ImageFile) -> Result<()> {
        if image.bitmap.color().has_alpha() {
            let pix_data = image.bitmap.to_rgba8();

            let start = Instant::now();
            if !Self::check_transparent_pixel(pix_data.as_rgba()) {
                trace!("SIMD Eval took {:?}", start.elapsed());
                debug!(
                    "Image {} has transparency, encoding fully.",
                    image.original_name()
                );

                let enc = self.encode_rgba(Img::new(
                    image.bitmap.to_rgba8().as_rgba(),
                    image.width as usize,
                    image.height as usize,
                ));

                image.encoded_data = enc?.avif_file;

                return Ok(());
            }
            trace!("SIMD Eval took {:?}", start.elapsed());
            debug!(
                "Image {} is opaque, discarding alpha channel.",
                image.original_name()
            )
        }

        image.encoded_data = self
            .encode_rgb(Img::new(
                image.bitmap.to_rgb8().as_rgb(),
                image.width as usize,
                image.height as usize,
            ))?
            .avif_file;

        Ok(())
    }

    fn check_transparent_pixel(image: &[RGBA<u8>]) -> bool {
        // Isolate only the alpha channel.
        let pixel_alpha = Vec::from_iter(image.iter().map(|pixel| pixel.a));

        let (_, sd, _) = pixel_alpha.as_simd::<32>();
        let alpha_mask = u8x32::splat(255);

        sd.iter().all(|pixel| {
            // let cmp = unsafe {
            //     let alpha_reg = _mm256_loadu_si256(pxl.as_ptr() as *const __m256i);
            //     let alpha_mask = _mm256_set1_epi8(-1);

            //     // Whatever happens, this thing generates 4 bytes that I need to check if they are 0b11111111 (-1)
            //     let alpha_cmp = _mm256_cmpeq_epi8(alpha_reg, alpha_mask);

            //     // Yup, this is the one. Not sure why I would only want to compare the leftmost bit, but seems faster.
            //     _mm256_movemask_epi8(alpha_cmp)
            // };

            // cmp.eq(&-1)

            pixel == &alpha_mask
        })
    }

    /// Make a new AVIF image from RGB pixels
    ///
    /// Make the `Img` for the `buffer` like this:
    ///
    /// ```rust,ignore
    /// Img::new(&pixels_rgb[..], width, height)
    /// ```
    ///
    /// If you have pixels as `u8` slice, then first do:
    ///
    /// ```rust,ignore
    /// use rgb::ComponentSlice;
    /// let pixels_rgb = pixels_u8.as_rgb();
    /// ```
    ///
    /// returns AVIF file, size of color metadata
    #[inline]
    fn encode_rgb(&self, buffer: Img<&[RGB8]>) -> Result<EncodedImage> {
        self.encode_rgb_internal(buffer.width(), buffer.height(), buffer.pixels())
    }

    fn encode_rgb_internal(
        &self,
        width: usize,
        height: usize,
        pixels: impl Iterator<Item = RGB8> + Send + Sync,
    ) -> Result<EncodedImage> {
        let planes = pixels.map(rgb_to_10_bit_ycbcr);
        self.encode_raw_planes_10_bit(width, height, planes, None::<[_; 0]>, PixelRange::Full)
    }

    /// Encodes AVIF from 3 planar channels that are in the color space described by `matrix_coefficients`,
    /// with sRGB transfer characteristics and color primaries.
    ///
    /// Alpha always uses full range. Chroma subsampling is not supported, and it's a bad idea for AVIF anyway.
    /// If there's no alpha, use `None::<[_; 0]>`.
    ///
    /// returns AVIF file, size of color metadata, size of alpha metadata overhead
    #[inline]
    pub fn encode_raw_planes_8_bit(
        &self,
        width: usize,
        height: usize,
        planes: impl IntoIterator<Item = [u8; 3]> + Send,
        alpha: Option<impl IntoIterator<Item = u8> + Send>,
        color_pixel_range: PixelRange,
    ) -> Result<EncodedImage> {
        self.encode_raw_planes(width, height, planes, alpha, color_pixel_range, 8)
    }

    /// Encodes AVIF from 3 planar channels that are in the color space described by `matrix_coefficients`,
    /// with sRGB transfer characteristics and color primaries.
    ///
    /// The pixels are 10-bit (values `0.=1023`).
    ///
    /// Alpha always uses full range. Chroma subsampling is not supported, and it's a bad idea for AVIF anyway.
    /// If there's no alpha, use `None::<[_; 0]>`.
    ///
    /// returns AVIF file, size of color metadata, size of alpha metadata overhead
    #[inline]
    pub fn encode_raw_planes_10_bit(
        &self,
        width: usize,
        height: usize,
        planes: impl IntoIterator<Item = [u16; 3]> + Send,
        alpha: Option<impl IntoIterator<Item = u16> + Send>,
        color_pixel_range: PixelRange,
    ) -> Result<EncodedImage> {
        self.encode_raw_planes(width, height, planes, alpha, color_pixel_range, 10)
    }

    #[inline(never)]
    fn encode_raw_planes<P: rav1e::Pixel + Default>(
        &self,
        width: usize,
        height: usize,
        planes: impl IntoIterator<Item = [P; 3]> + Send,
        alpha: Option<impl IntoIterator<Item = P> + Send>,
        color_pixel_range: PixelRange,
        bit_depth: u8,
    ) -> Result<EncodedImage> {
        let color_description = Some(ColorDescription {
            transfer_characteristics: TransferCharacteristics::SRGB,
            color_primaries: ColorPrimaries::BT709, // sRGB-compatible
            matrix_coefficients: MatrixCoefficients::BT601,
        });

        let threads = self.threads;
        trace!("Initializing encoder with {threads} threads.");

        trace!("Encoding color channel");

        let color = encode_to_av1::<P>(
            &Av1EncodeConfig {
                width,
                height,
                bit_depth: bit_depth.into(),
                quantizer: self.quantizer.into(),
                speed: SpeedTweaks::from_my_preset(self.speed, self.quantizer),
                threads,
                pixel_range: color_pixel_range,
                chroma_sampling: ChromaSampling::Cs444,
                color_description,
            },
            move |frame| init_frame_color(width, height, planes, frame),
        );

        let alpha = alpha.map(|alpha| {
            trace!("Encoding alpha channel");
            encode_to_av1::<P>(
                &Av1EncodeConfig {
                    width,
                    height,
                    bit_depth: bit_depth.into(),
                    quantizer: self.alpha_quantizer.into(),
                    speed: SpeedTweaks::from_my_preset(self.speed, self.alpha_quantizer),
                    threads,
                    pixel_range: PixelRange::Full,
                    chroma_sampling: ChromaSampling::Cs400,
                    color_description: None,
                },
                |frame| init_frame_alpha_pix(width, height, alpha, frame),
            )
        });

        let (color, alpha) = (color?, alpha.transpose()?);

        let avif_file = avif_serialize::Aviffy::new()
            .matrix_coefficients(avif_serialize::constants::MatrixCoefficients::Bt601)
            .premultiplied_alpha(false)
            .to_vec(
                &color,
                alpha.as_deref(),
                width as u32,
                height as u32,
                bit_depth,
            );
        let color_byte_size = color.len();
        let alpha_byte_size = alpha.as_ref().map_or(0, |a| a.len());

        Ok(EncodedImage {
            avif_file,
            color_byte_size,
            alpha_byte_size,
        })
    }
}

#[inline(always)]
fn rgb_to_ycbcr(px: rgb::RGB<u8>, depth: u8) -> (f32, f32, f32) {
    let matrix = [0.2990, 0.5870, 0.1140]; // BT601

    let max_value = ((1 << depth) - 1) as f32;
    let scale = max_value / 255.;
    let shift = (max_value * 0.5).round();
    let y = scale * matrix[0] * f32::from(px.r)
        + scale * matrix[1] * f32::from(px.g)
        + scale * matrix[2] * f32::from(px.b);
    let cb = (f32::from(px.b) * scale - y).mul_add(0.5 / (1. - matrix[2]), shift);
    let cr = (f32::from(px.r) * scale - y).mul_add(0.5 / (1. - matrix[0]), shift);
    (y.round(), cb.round(), cr.round())
}

#[inline(always)]
fn rgb_to_10_bit_ycbcr(px: rgb::RGB<u8>) -> [u16; 3] {
    let (y, u, v) = rgb_to_ycbcr(px, 10);
    [y as u16, u as u16, v as u16]
}

#[inline(always)]
fn rgb_to_8_bit_ycbcr(px: rgb::RGB<u8>) -> [u8; 3] {
    let (y, u, v) = rgb_to_ycbcr(px, 8);
    [y as u8, u as u8, v as u8]
}

fn quality_to_quantizer(quality: f32) -> u8 {
    let q = quality / 100.;
    let x = if q >= 0.85 {
        (1. - q) * 3.
    } else if q > 0.25 {
        1. - 0.125 - q * 0.5
    } else {
        1. - q
    };
    (x * 255.).round() as u8
}

#[derive(Debug, Copy, Clone)]
struct SpeedTweaks {
    pub speed_preset: u8,

    pub fast_deblock: Option<bool>,
    pub reduced_tx_set: Option<bool>,
    pub tx_domain_distortion: Option<bool>,
    pub tx_domain_rate: Option<bool>,
    pub encode_bottomup: Option<bool>,
    pub rdo_tx_decision: Option<bool>,
    pub cdef: Option<bool>,
    /// loop restoration filter
    pub lrf: Option<bool>,
    pub sgr_complexity_full: Option<bool>,
    pub use_satd_subpel: Option<bool>,
    pub inter_tx_split: Option<bool>,
    pub fine_directional_intra: Option<bool>,
    pub complex_prediction_modes: Option<bool>,
    pub partition_range: Option<(u8, u8)>,
    pub min_tile_size: u16,
}

impl SpeedTweaks {
    pub fn from_my_preset(speed: u8, quantizer: u8) -> Self {
        let low_quality = quantizer < quality_to_quantizer(55.);
        let high_quality = quantizer > quality_to_quantizer(80.);
        let max_block_size = if high_quality { 16 } else { 64 };

        Self {
            speed_preset: speed,

            partition_range: Some(match speed {
                0 => (4, 64.min(max_block_size)),
                1 if low_quality => (4, 64.min(max_block_size)),
                2 if low_quality => (4, 32.min(max_block_size)),
                1..=4 => (4, 16),
                5..=8 => (8, 16),
                _ => (16, 16),
            }),

            complex_prediction_modes: Some(speed <= 1), // 2x-3x slower, 2% better
            sgr_complexity_full: Some(speed <= 2), // 15% slower, barely improves anything -/+1%

            encode_bottomup: Some(speed <= 2), // may be costly (+60%), may even backfire

            // big blocks disabled at 3

            // these two are together?
            rdo_tx_decision: Some(speed <= 4 && !high_quality), // it tends to blur subtle textures
            reduced_tx_set: Some(speed == 4 || speed >= 9), // It interacts with tx_domain_distortion too?

            // 4px blocks disabled at 5
            fine_directional_intra: Some(speed <= 6),
            fast_deblock: Some(speed >= 7 && !high_quality), // mixed bag?

            // 8px blocks disabled at 8
            lrf: Some(low_quality && speed <= 8), // hardly any help for hi-q images. recovers some q at low quality
            cdef: Some(low_quality && speed <= 9), // hardly any help for hi-q images. recovers some q at low quality

            inter_tx_split: Some(speed >= 9), // mixed bag even when it works, and it backfires if not used together with reduced_tx_set
            tx_domain_rate: Some(speed >= 10), // 20% faster, but also 10% larger files!

            tx_domain_distortion: None, // very mixed bag, sometimes helps speed sometimes it doesn't
            use_satd_subpel: Some(false), // doesn't make sense
            min_tile_size: match speed {
                0 => 4096,
                1 => 2048,
                2 => 1024,
                3 => 512,
                4 => 256,
                _ => 128,
            } * if high_quality { 2 } else { 1 },
        }
    }

    pub(crate) fn speed_settings(&self) -> SpeedSettings {
        let mut speed_settings = SpeedSettings::from_preset(self.speed_preset);

        speed_settings.multiref = false;
        speed_settings.rdo_lookahead_frames = 1;
        speed_settings.scene_detection_mode = SceneDetectionSpeed::None;
        speed_settings.motion.include_near_mvs = false;

        if let Some(v) = self.fast_deblock {
            speed_settings.fast_deblock = v;
        }
        if let Some(v) = self.reduced_tx_set {
            speed_settings.transform.reduced_tx_set = v;
        }
        if let Some(v) = self.tx_domain_distortion {
            speed_settings.transform.tx_domain_distortion = v;
        }
        if let Some(v) = self.tx_domain_rate {
            speed_settings.transform.tx_domain_rate = v;
        }
        if let Some(v) = self.encode_bottomup {
            speed_settings.partition.encode_bottomup = v;
        }
        if let Some(v) = self.rdo_tx_decision {
            speed_settings.transform.rdo_tx_decision = v;
        }
        if let Some(v) = self.cdef {
            speed_settings.cdef = v;
        }
        if let Some(v) = self.lrf {
            speed_settings.lrf = v;
        }
        if let Some(v) = self.inter_tx_split {
            speed_settings.transform.enable_inter_tx_split = v;
        }
        if let Some(v) = self.sgr_complexity_full {
            speed_settings.sgr_complexity = if v {
                SGRComplexityLevel::Full
            } else {
                SGRComplexityLevel::Reduced
            }
        };
        if let Some(v) = self.use_satd_subpel {
            speed_settings.motion.use_satd_subpel = v;
        }
        if let Some(v) = self.fine_directional_intra {
            speed_settings.prediction.fine_directional_intra = v;
        }
        if let Some(v) = self.complex_prediction_modes {
            speed_settings.prediction.prediction_modes = if v {
                PredictionModesSetting::ComplexAll
            } else {
                PredictionModesSetting::Simple
            }
        };
        if let Some((min, max)) = self.partition_range {
            assert!(min <= max);
            fn sz(s: u8) -> BlockSize {
                match s {
                    4 => BlockSize::BLOCK_4X4,
                    8 => BlockSize::BLOCK_8X8,
                    16 => BlockSize::BLOCK_16X16,
                    32 => BlockSize::BLOCK_32X32,
                    64 => BlockSize::BLOCK_64X64,
                    128 => BlockSize::BLOCK_128X128,
                    _ => panic!("bad size {s}"),
                }
            }
            speed_settings.partition.partition_range = PartitionRange::new(sz(min), sz(max));
        }

        speed_settings
    }
}

struct Av1EncodeConfig {
    pub width: usize,
    pub height: usize,
    pub bit_depth: usize,
    pub quantizer: usize,
    pub speed: SpeedTweaks,
    /// 0 means num_cpus
    pub threads: usize,
    pub pixel_range: PixelRange,
    pub chroma_sampling: ChromaSampling,
    pub color_description: Option<ColorDescription>,
}

fn rav1e_config(p: &Av1EncodeConfig) -> Config {
    // AV1 needs all the CPU power you can give it,
    // except when it'd create inefficiently tiny tiles
    let tiles = {
        let threads = p.threads;
        threads.min((p.width * p.height) / (p.speed.min_tile_size as usize).pow(2))
    };
    let speed_settings = p.speed.speed_settings();
    let cfg = Config::new().with_encoder_config(EncoderConfig {
        width: p.width,
        height: p.height,
        time_base: Rational::new(1, 1),
        sample_aspect_ratio: Rational::new(1, 1),
        bit_depth: p.bit_depth,
        chroma_sampling: p.chroma_sampling,
        chroma_sample_position: ChromaSamplePosition::Unknown,
        pixel_range: p.pixel_range,
        color_description: p.color_description,
        mastering_display: None,
        content_light: None,
        enable_timing_info: false,
        still_picture: true,
        error_resilient: false,
        switch_frame_interval: 0,
        min_key_frame_interval: 0,
        max_key_frame_interval: 0,
        reservoir_frame_delay: None,
        low_latency: false,
        quantizer: p.quantizer,
        min_quantizer: p.quantizer as _,
        bitrate: 0,
        tune: Tune::Psychovisual,
        tile_cols: 0,
        tile_rows: 0,
        tiles,
        film_grain_params: None,
        level_idx: None,
        speed_settings,
    });
    cfg.with_threads(p.threads)
}

fn init_frame_color<P: rav1e::Pixel + Default>(
    width: usize,
    height: usize,
    planes: impl IntoIterator<Item = [P; 3]> + Send,
    frame: &mut Frame<P>,
) -> Result<()> {
    let mut f = frame.planes.iter_mut();
    let mut planes = planes.into_iter();

    // it doesn't seem to be necessary to fill padding area
    let mut y = f.next().unwrap().mut_slice(Default::default());
    let mut u = f.next().unwrap().mut_slice(Default::default());
    let mut v = f.next().unwrap().mut_slice(Default::default());

    for ((y, u), v) in y
        .rows_iter_mut()
        .zip(u.rows_iter_mut())
        .zip(v.rows_iter_mut())
        .take(height)
    {
        let y = &mut y[..width];
        let u = &mut u[..width];
        let v = &mut v[..width];
        for ((y, u), v) in y.iter_mut().zip(u).zip(v) {
            let px = planes.next().ok_or(Error::TooFewPixels)?;
            *y = px[0];
            *u = px[1];
            *v = px[2];
        }
    }
    Ok(())
}

fn init_frame_alpha_pix<P: rav1e::Pixel + Default>(
    width: usize,
    height: usize,
    planes: impl IntoIterator<Item = P> + Send,
    frame: &mut Frame<P>,
) -> Result<()> {
    let mut y = frame.planes[0].mut_slice(Default::default());
    let mut planes = planes.into_iter();

    for y in y.rows_iter_mut().take(height) {
        let y = &mut y[..width];
        for y in y.iter_mut() {
            *y = planes.next().ok_or(Error::TooFewPixels)?;
        }
    }
    Ok(())
}

#[inline(never)]
fn encode_to_av1<P: rav1e::Pixel>(
    p: &Av1EncodeConfig,
    init: impl FnOnce(&mut Frame<P>) -> Result<()>,
) -> Result<Vec<u8>> {
    let mut ctx: Context<P> = rav1e_config(p).new_context()?;
    let mut frame = ctx.new_frame();

    init(&mut frame)?;
    ctx.send_frame(frame)?;
    ctx.flush();

    let mut out = Vec::new();
    loop {
        match ctx.receive_packet() {
            Ok(mut packet) => match packet.frame_type {
                FrameType::KEY => {
                    out.append(&mut packet.data);
                }
                _ => continue,
            },
            Err(EncoderStatus::Encoded) | Err(EncoderStatus::LimitReached) => break,
            Err(err) => Err(err)?,
        }
    }
    Ok(out)
}
