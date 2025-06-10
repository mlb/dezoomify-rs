use std::error::Error;
use std::fmt;

use crate::dezoomer::TileReference;
use crate::encoder::tile_buffer::TileBufferMsg;
use custom_error::custom_error;
use reqwest::{self, header};
use tokio::sync::mpsc::error::SendError;

custom_error! {
    pub ZoomError
    Networking{source: reqwest::Error} = "network error: {source}",
    Dezoomer{source: DezoomerError} = "Dezoomer error: {source}",
    NoLevels = "A zoomable image was found, but it did not contain any zoom level",
    NoBulkUrl { bulk_file_path: String } = "No url found in bulk file {bulk_file_path}",
    NoTile = "Could not get any tile for the image. See https://dezoomify-rs.ophir.dev/no-tile-error",
    PartialDownload{successful_tiles: u64, total_tiles: u64, destination: String} =
        "Only {successful_tiles} tiles out of {total_tiles} could be downloaded. \
        The resulting image was still created in '{destination}'.",
    Image{source: image::ImageError} = "invalid image error: {source}",
    PostProcessing{source: Box<dyn Error>} = "unable to process the downloaded tile: {source}",
    Io{source: std::io::Error} = "Input/Output error: {source}",
    Yaml{source: serde_yaml::Error} = "Invalid YAML configuration file: {source}",
    TileCopyError{x:u32, y:u32, twidth:u32, theight:u32, width:u32, height:u32} =
                                "Unable to copy a {twidth}x{theight} tile \
                                 at position {x},{y} \
                                 on a canvas of size {width}x{height}",
    MalformedTileStr{tile_str: String} = "Malformed tile string: '{tile_str}' \
                                          expected 'x y url'",
    NoSuchDezoomer{name: String} = "No such dezoomer: {name}",
    InvalidHeaderName{source: header::InvalidHeaderName} = "Invalid header name: {source}",
    InvalidHeaderValue{source: header::InvalidHeaderValue} = "Invalid header value: {source}",
    AsyncError{source: tokio::task::JoinError} = "Unable get the result from a thread: {source}",
    BufferToImage{source: BufferToImageError} = "{source}",
    WriteError{source: SendError<TileBufferMsg>} = "Unable to write tile {source:?}",
    PngError{source: png::EncodingError} = "PNG encoding error: {source}",
}

custom_error! {
    pub BufferToImageError
    Image{source: image::ImageError} = "invalid image error: {source}",
    PostProcessing{e: Box<dyn Error + Send>} = "unable to process the downloaded tile: {e}",
}

custom_error! {pub DezoomerError
    NeedsData{uri: String}           = "Need to download data from {uri}",
    WrongDezoomer{name:&'static str} = "The '{name}' dezoomer cannot handle this URI",
    DownloadError{msg: String} = "Unable to download required data: {msg}",
    Other{source: Box<dyn Error>}    = "Unable to create the dezoomer: {source}"
}

impl DezoomerError {
    pub fn wrap<E: Error + 'static>(err: E) -> DezoomerError {
        DezoomerError::Other { source: err.into() }
    }
}

pub fn image_error_to_io_error(err: image::ImageError) -> std::io::Error {
    match err {
        image::ImageError::IoError(e) => e,
        e => make_io_err(e),
    }
}

pub fn make_io_err<E>(e: E) -> std::io::Error
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    std::io::Error::other(e)
}

#[derive(Debug)]
pub struct TileDownloadError {
    pub tile_reference: TileReference,
    pub cause: ZoomError,
}

impl fmt::Display for TileDownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Unable to download tile \'{}\'. Cause: {}",
            self.tile_reference.url, self.cause
        )
    }
}

impl Error for TileDownloadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}
