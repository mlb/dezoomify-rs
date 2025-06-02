use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::PathBuf;

use crate::tile::Tile;
use crate::{Vec2d, ZoomError};

use super::Encoder;
use super::pixel_streamer::PixelStreamer;

pub struct PngEncoder {
    pixel_streamer: Option<PixelStreamer<png::StreamWriter<'static, File>>>,
    file: Option<File>,
    compression: png::Compression,
    size: Vec2d,
    first_tile: bool,
}

impl PngEncoder {
    pub fn new(destination: PathBuf, size: Vec2d, compression: u8) -> Result<Self, ZoomError> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(destination)?;

        let compression_level = match compression {
            0..=19 => png::Compression::Fast,
            20..=60 => png::Compression::Default,
            _ => png::Compression::Best,
        };

        Ok(PngEncoder {
            pixel_streamer: None,
            file: Some(file),
            compression: compression_level,
            size,
            first_tile: true,
        })
    }

    fn write_header_with_profile(&mut self, icc_profile: Option<&Vec<u8>>) -> io::Result<()> {
        let file = self
            .file
            .take()
            .expect("File should be available when writing header");

        let writer = if let Some(profile) = icc_profile {
            let mut info = png::Info::default();
            info.width = self.size.x;
            info.height = self.size.y;
            info.color_type = png::ColorType::Rgb;
            info.bit_depth = png::BitDepth::Eight;
            info.compression = self.compression;
            info.icc_profile = Some(Cow::Owned(profile.clone()));

            log::debug!(
                "Setting ICC profile in PNG header (size: {} bytes)",
                profile.len()
            );
            png::Encoder::with_info(file, info)?
                .write_header()?
                .into_stream_writer_with_size(128 * 1024)?
        } else {
            let mut encoder = png::Encoder::new(file, self.size.x, self.size.y);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(self.compression);
            encoder
                .write_header()?
                .into_stream_writer_with_size(128 * 1024)?
        };

        self.pixel_streamer = Some(PixelStreamer::new(writer, self.size));
        Ok(())
    }
}

impl Encoder for PngEncoder {
    fn add_tile(&mut self, tile: Tile) -> io::Result<()> {
        if self.first_tile {
            // Write header with ICC profile from first tile if available
            let icc_profile = tile.icc_profile.as_ref();
            if icc_profile.is_some() {
                log::debug!(
                    "Using ICC profile from first tile (size: {} bytes)",
                    icc_profile.unwrap().len()
                );
            }
            self.write_header_with_profile(icc_profile)?;
            self.first_tile = false;
        }

        self.pixel_streamer
            .as_mut()
            .expect("tried to add a tile in a finalized image")
            .add_tile(tile)
    }

    fn finalize(&mut self) -> io::Result<()> {
        // If no tiles were added, write header without ICC profile
        if self.first_tile {
            self.write_header_with_profile(None)?;
        }

        let mut pixel_streamer = self
            .pixel_streamer
            .take()
            .expect("Tried to finalize an image twice");
        pixel_streamer.finalize()?;
        // Disabled because of https://github.com/image-rs/image-png/issues/307
        // let writer = pixel_streamer.into_writer();
        // writer.finish()?;
        Ok(())
    }

    fn size(&self) -> Vec2d {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use std::env::temp_dir;

    use image::{DynamicImage, ImageBuffer, Rgb};
    use itertools::Itertools;

    use super::*;

    #[test]
    fn test_png_create() {
        let destination = temp_dir().join("dezoomify-rs-png-test.png");
        let size = Vec2d { x: 2, y: 2 };
        let mut encoder = PngEncoder::new(destination.clone(), size, 1).unwrap();

        encoder
            .add_tile(Tile {
                position: Vec2d { x: 0, y: 1 },
                image: DynamicImage::ImageRgb8(ImageBuffer::from_raw(1, 1, vec![1, 2, 3]).unwrap()),
                icc_profile: None,
            })
            .unwrap();

        encoder.finalize().unwrap();
        let final_image = image::open(&destination).unwrap();
        let empty = Rgb::from([0u8, 0, 0]);
        assert_eq!(
            final_image.to_rgb8().pixels().copied().collect_vec(),
            vec![empty, empty, Rgb::from([1, 2, 3]), empty,]
        );
    }

    #[test]
    fn test_png_create_with_icc_profile() {
        let destination = temp_dir().join("dezoomify-rs-png-icc-test.png");
        let size = Vec2d { x: 1, y: 1 };
        let mut encoder = PngEncoder::new(destination.clone(), size, 1).unwrap();

        // Create a dummy ICC profile (simplified sRGB profile header)
        let icc_profile = vec![
            0x00, 0x00, 0x02, 0x0C, // Profile size (524 bytes)
            0x61, 0x64, 0x73, 0x70, // Signature 'adsp'
            0x00, 0x00, 0x00, 0x00, // Platform signature
            0x6D, 0x6E, 0x74, 0x72, // Device class 'mntr'
            0x52, 0x47, 0x42, 0x20, // Color space 'RGB '
        ];

        encoder
            .add_tile(Tile {
                position: Vec2d { x: 0, y: 0 },
                image: DynamicImage::ImageRgb8(
                    ImageBuffer::from_raw(1, 1, vec![255, 0, 0]).unwrap(),
                ),
                icc_profile: Some(icc_profile.clone()),
            })
            .unwrap();

        encoder.finalize().unwrap();
        assert!(destination.exists());

        // Verify the ICC profile was actually written to the PNG
        let file = std::fs::File::open(&destination).unwrap();
        let decoder = png::Decoder::new(file);
        let reader = decoder.read_info().unwrap();
        let info = reader.info();

        // Check that ICC profile exists and matches what we provided
        assert!(info.icc_profile.is_some());
        if let Some(embedded_profile) = &info.icc_profile {
            assert_eq!(embedded_profile.as_ref(), &icc_profile);
        }
    }
}
