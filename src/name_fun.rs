use blake2::{digest::typenum::U16, Blake2b, Digest as B2Digest};
use clap::ValueEnum;
use rand::{distr::Alphanumeric, rng, Rng};
use sha2::Sha256;

use crate::image_file::ImageFile;

#[derive(Debug, ValueEnum, Copy, Clone, Default)]
#[repr(u8)]
pub enum Name {
    #[default]
    MD5,
    SHA256,
    Blake2,
    Random,
    Same,
}

type Blake2b32char = Blake2b<U16>;

impl Name {
    pub fn generate_name(self, data: &ImageFile) -> String {
        match self {
            Name::MD5 => {
                let digest = md5::compute(&data.encoded_data);

                format!("{digest:x}")
            }
            Name::SHA256 => {
                let mut hasher = Sha256::new();

                hasher.update(&data.encoded_data);

                hex::encode(hasher.finalize())
            }
            Name::Blake2 => {
                let mut hasher = Blake2b32char::new();
                hasher.update(&data.encoded_data);
                hex::encode(hasher.finalize())
            }
            Name::Random => Self::random_string(),
            Name::Same => data.metadata.name.clone(),
        }
    }

    fn random_string() -> String {
        let rng = rng();
        let s = rng.sample_iter(&Alphanumeric).take(32).map(char::from);

        String::from_iter(s).to_lowercase()
    }
}
