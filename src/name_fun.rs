use clap::ValueEnum;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sha2::{Digest, Sha256};

#[derive(Debug, ValueEnum, Copy, Clone)]
#[repr(u8)]
pub enum Name {
    MD5,
    SHA256,
    Random,
}

impl Name {
    pub fn generate_name(self, data: &[u8]) -> String {
        match self {
            Name::MD5 => {
                let digest = md5::compute(data);

                format!("{digest:x}")
            }
            Name::SHA256 => {
                let mut hasher = Sha256::new();

                hasher.update(data);

                hex::encode(hasher.finalize())
            }
            Name::Random => Self::random_string(),
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