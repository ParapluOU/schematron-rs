use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to parse XML: {0}")]
    Xml(String),
    #[error("not a Schematron schema: expected a <schema> element, found <{0}>")]
    NotASchema(String),
    #[error("{0}")]
    Schema(String),
    #[error("XPath error in {context}: {message}")]
    XPath { context: String, message: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
