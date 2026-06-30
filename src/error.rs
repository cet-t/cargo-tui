use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Cargo.toml が見つかりません")]
    NoCargoToml,

    #[error("IO エラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML パースエラー: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("HTTP エラー: {0}")]
    Http(#[from] reqwest::Error),
}
