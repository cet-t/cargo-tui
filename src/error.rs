use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Cargo.toml not found")]
    NoCargoToml,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}
