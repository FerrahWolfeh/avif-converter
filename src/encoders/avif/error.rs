use thiserror::Error;

#[derive(Debug)]
#[doc(hidden)]
pub struct EncodingErrorDetail; // maybe later

/// Failures enum
#[derive(Debug, Error)]
pub enum Error {
    /// Slices given to `encode_raw_planes` must be `width * height` large.
    #[error("Provided buffer is smaller than width * height")]
    TooFewPixels,

    #[error("Unsupported matrix coefficient")]
    UnsupportedMatrixCoeff,
}
