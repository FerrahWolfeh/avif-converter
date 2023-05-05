use clap::ValueEnum;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sha2::{Digest, Sha256};

use crate::image_file::ImageFile;

#[derive(Debug, ValueEnum, Copy, Clone)]
#[repr(u8)]
pub enum Name {
    MD5,
    SHA256,
    Random,
    Same,
}

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
            Name::Random => Self::random_string(),
            Name::Same => data.metadata.name.clone(),
        }
    }

    fn random_string() -> String {
        let s = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from);

        String::from_iter(s)
    }
}
