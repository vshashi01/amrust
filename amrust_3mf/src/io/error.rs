use image::ImageError;
use thiserror::Error;
use zip::result::ZipError;

/// An error that can occur while writing a 3MF file
#[derive(Debug, Error)]
pub enum Error {
    /// I/O error while writing 3MF file
    #[error("I/O error while importing/exporting to 3MF file")]
    Io(#[from] std::io::Error),

    /// Error writing ZIP file (3MF files are ZIP files)
    #[error("Error writing ZIP file (3MF files are ZIP files)")]
    Zip(#[from] ZipError),

    #[error("Error reading 3mf file: {0}")]
    ReadError(String),

    #[error("Error reading thumbnail image: {0}")]
    ImageReadError(#[from] ImageError),

    #[error("Error writing 3mf file: {0}")]
    WriteError(String),

    #[error("Derialization error from Instant-Xml")]
    InstantXmlError(#[from] instant_xml::Error),

    #[error("Thumbnail error: {0}")]
    ThumbnailError(String),
}
