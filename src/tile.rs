use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader};
use log::warn;
use std::io::Cursor;

use crate::dezoomer::{PostProcessFn, TileReference};
use crate::errors::BufferToImageError;
use crate::network::fetch_uri;
use crate::{Vec2d, ZoomError};

#[derive(Clone)]
pub struct Tile {
    pub image: image::DynamicImage,
    pub position: Vec2d,
    pub icc_profile: Option<Vec<u8>>,
    pub exif_metadata: Option<Vec<u8>>,
}

impl Tile {
    pub fn size(&self) -> Vec2d {
        self.image.dimensions().into()
    }
    pub fn bottom_right(&self) -> Vec2d {
        self.size() + self.position
    }

    pub fn builder() -> TileBuilder {
        TileBuilder::default()
    }
    pub async fn download(
        post_process_fn: PostProcessFn,
        tile_reference: &TileReference,
        client: &reqwest::Client,
    ) -> Result<Tile, ZoomError> {
        let bytes = fetch_uri(&tile_reference.url, client).await?;
        let tile_reference = tile_reference.clone();

        let tile: Result<Tile, BufferToImageError> = tokio::spawn(async move {
            tokio::task::block_in_place(move || {
                let transformed_bytes = if let PostProcessFn::Fn(post_process) = post_process_fn {
                    post_process(&tile_reference, bytes)
                        .map_err(|e| BufferToImageError::PostProcessing { e })?
                } else {
                    bytes
                };

                let image_with_metadata = load_image_with_metadata(&transformed_bytes)?;

                Ok(Tile {
                    image: image_with_metadata.image,
                    position: tile_reference.position,
                    icc_profile: image_with_metadata.icc_profile,
                    exif_metadata: image_with_metadata.exif_metadata,
                })
            })
        })
        .await?;
        Ok(tile?)
    }
    pub fn empty(position: Vec2d, size: Vec2d) -> Tile {
        Tile {
            image: DynamicImage::new_rgba8(size.x, size.y),
            position,
            icc_profile: None,
            exif_metadata: None,
        }
    }
    pub fn position(&self) -> Vec2d {
        self.position
    }
}

#[derive(Default)]
pub struct TileBuilder {
    image: Option<image::DynamicImage>,
    position: Option<Vec2d>,
    icc_profile: Option<Vec<u8>>,
    exif_metadata: Option<Vec<u8>>,
}

impl TileBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_image(mut self, image: image::DynamicImage) -> Self {
        self.image = Some(image);
        self
    }

    pub fn at_position(mut self, position: Vec2d) -> Self {
        self.position = Some(position);
        self
    }

    pub fn with_icc_profile(mut self, profile: Vec<u8>) -> Self {
        self.icc_profile = Some(profile);
        self
    }

    pub fn with_exif_metadata(mut self, metadata: Vec<u8>) -> Self {
        self.exif_metadata = Some(metadata);
        self
    }

    pub fn build(self) -> Tile {
        Tile {
            image: self.image.expect("Image is required"),
            position: self.position.unwrap_or(Vec2d { x: 0, y: 0 }),
            icc_profile: self.icc_profile,
            exif_metadata: self.exif_metadata,
        }
    }
}

/// Represents an image loaded with its associated metadata
#[derive(Debug)]
struct ImageWithMetadata {
    image: DynamicImage,
    icc_profile: Option<Vec<u8>>,
    exif_metadata: Option<Vec<u8>>,
}

type MetadataResult = Result<ImageWithMetadata, image::ImageError>;

fn load_image_with_metadata(bytes: &[u8]) -> MetadataResult {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;

    // Try to get a decoder from the reader
    if let Ok(mut decoder) = reader.into_decoder() {
        // Extract ICC profile first
        let icc_profile = decoder.icc_profile().unwrap_or_else(|e| {
            warn!("Failed to extract ICC profile from tile: {e}");
            None
        });

        // Extract EXIF metadata
        let exif_metadata = decoder.exif_metadata().unwrap_or_else(|e| {
            warn!("Failed to extract EXIF metadata from tile: {e}");
            None
        });

        // Then decode the image using the same decoder
        let image = DynamicImage::from_decoder(decoder)?;

        Ok(ImageWithMetadata {
            image,
            icc_profile,
            exif_metadata,
        })
    } else {
        // Fallback to standard loading without metadata
        let image = image::load_from_memory(bytes)?;
        Ok(ImageWithMetadata {
            image,
            icc_profile: None,
            exif_metadata: None,
        })
    }
}

impl std::fmt::Debug for Tile {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Tile")
            .field("x", &self.position.x)
            .field("y", &self.position.y)
            .field("width", &self.image.width())
            .field("height", &self.image.height())
            .field("has_icc_profile", &self.icc_profile.is_some())
            .field("has_exif_metadata", &self.exif_metadata.is_some())
            .finish()
    }
}

impl PartialEq for Tile {
    fn eq(&self, other: &Self) -> bool {
        self.position == other.position
            && self.size() == other.size()
            && self.icc_profile == other.icc_profile
            && self.exif_metadata == other.exif_metadata
            && self
                .image
                .pixels()
                .all(|(x, y, pix)| other.image.get_pixel(x, y) == pix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageBuffer;

    #[test]
    fn test_load_image_with_icc_profile() {
        // Test with empty bytes (should return error)
        let empty_bytes = vec![];
        let result = load_image_with_metadata(&empty_bytes);
        assert!(result.is_err());

        // Test with invalid image data (should return error)
        let invalid_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0]; // Incomplete JPEG header
        let result = load_image_with_metadata(&invalid_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_tile_with_metadata() {
        let tile = Tile {
            image: image::DynamicImage::ImageRgb8(
                ImageBuffer::from_raw(2, 2, vec![255; 12]).unwrap(),
            ),
            position: Vec2d { x: 0, y: 0 },
            icc_profile: Some(vec![1, 2, 3, 4]), // Mock ICC profile
            exif_metadata: Some(vec![5, 6, 7, 8]), // Mock EXIF data
        };

        assert_eq!(tile.position(), Vec2d { x: 0, y: 0 });
        assert_eq!(tile.size(), Vec2d { x: 2, y: 2 });
        assert!(tile.icc_profile.is_some());
        assert!(tile.exif_metadata.is_some());
        assert_eq!(tile.icc_profile.unwrap().len(), 4);
        assert_eq!(tile.exif_metadata.unwrap().len(), 4);
    }

    #[test]
    fn test_empty_tile_has_no_metadata() {
        let tile = Tile::empty(Vec2d { x: 10, y: 10 }, Vec2d { x: 5, y: 5 });
        assert!(tile.icc_profile.is_none());
        assert!(tile.exif_metadata.is_none());
        assert_eq!(tile.position(), Vec2d { x: 10, y: 10 });
        assert_eq!(tile.size(), Vec2d { x: 5, y: 5 });
    }
}
