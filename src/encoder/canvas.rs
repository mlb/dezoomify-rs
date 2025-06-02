use image::{
    ExtendedColorType, GenericImageView, ImageBuffer, ImageEncoder, ImageResult, Pixel, PixelWithColorType, Rgb,
    Rgba,
};
use log::debug;
use std::io;
use std::path::{Path, PathBuf};

use crate::Vec2d;
use crate::ZoomError;
use crate::encoder::Encoder;
use crate::tile::Tile;
use std::fs::File;
use std::io::BufWriter;

type CanvasBuffer<Pix> = ImageBuffer<Pix, Vec<<Pix as Pixel>::Subpixel>>;

pub struct Canvas<Pix: Pixel = Rgba<u8>> {
    image: CanvasBuffer<Pix>,
    destination: PathBuf,
    image_writer: ImageWriter,
    icc_profile: Option<Vec<u8>>,
}

impl<Pix: Pixel> Canvas<Pix> {
    pub fn new_generic(destination: PathBuf, size: Vec2d) -> Result<Self, ZoomError> {
        Ok(Canvas {
            image: ImageBuffer::new(size.x, size.y),
            destination,
            image_writer: ImageWriter::Generic,
            icc_profile: None,
        })
    }

    pub fn new_jpeg(
        destination: PathBuf,
        size: Vec2d,
        quality: u8,
    ) -> Result<Canvas<Rgb<u8>>, ZoomError> {
        Ok(Canvas::<Rgb<u8>> {
            image: ImageBuffer::new(size.x, size.y),
            destination,
            image_writer: ImageWriter::Jpeg { quality },
            icc_profile: None,
        })
    }
}

trait FromRgba {
    fn from_rgba(rgba: Rgba<u8>) -> Self;
}

impl FromRgba for Rgba<u8> {
    fn from_rgba(rgba: Rgba<u8>) -> Self {
        rgba
    }
}

impl FromRgba for Rgb<u8> {
    fn from_rgba(rgba: Rgba<u8>) -> Self {
        rgba.to_rgb()
    }
}

impl<Pix: Pixel<Subpixel = u8> + PixelWithColorType + Send + FromRgba + 'static> Encoder
    for Canvas<Pix>
{
    fn add_tile(&mut self, tile: Tile) -> io::Result<()> {
        debug!("Copying tile data from {tile:?}");
        
        // Capture ICC profile from the first tile that has one
        if self.icc_profile.is_none() && tile.icc_profile.is_some() {
            self.icc_profile = tile.icc_profile.clone();
            debug!("Captured ICC profile from tile (size: {} bytes)", 
                   self.icc_profile.as_ref().unwrap().len());
        }
        
        let min_pos = tile.position();
        let canvas_size = self.size();
        if !min_pos.fits_inside(canvas_size) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "tile too large for image",
            ));
        }
        let max_pos = tile.bottom_right().min(canvas_size);
        let size = max_pos - min_pos;
        for y in 0..size.y {
            let canvas_y = y + min_pos.y;
            for x in 0..size.x {
                let canvas_x = x + min_pos.x;
                let p = tile.image.get_pixel(x, y);
                self.image.put_pixel(canvas_x, canvas_y, Pix::from_rgba(p));
            }
        }
        Ok(())
    }

    fn finalize(&mut self) -> io::Result<()> {
        self.image_writer
            .write(&self.image, &self.destination, &self.icc_profile)
            .map_err(|e| match e {
                image::ImageError::IoError(e) => e,
                other => io::Error::other(other),
            })?;
        Ok(())
    }

    fn size(&self) -> Vec2d {
        self.image.dimensions().into()
    }
}

pub enum ImageWriter {
    Generic,
    Jpeg { quality: u8 },
}

impl ImageWriter {
    fn write<Pix: Pixel<Subpixel = u8> + PixelWithColorType>(
        &self,
        image: &CanvasBuffer<Pix>,
        destination: &Path,
        icc_profile: &Option<Vec<u8>>,
    ) -> ImageResult<()> {
        match *self {
            ImageWriter::Jpeg { quality } => {
                let file = File::create(destination)?;
                let fout = &mut BufWriter::new(file);
                let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(fout, quality);
                
                // Set ICC profile if available
                if let Some(profile) = icc_profile {
                    if let Err(e) = encoder.set_icc_profile(profile.clone()) {
                        debug!("Failed to set ICC profile for JPEG: {}", e);
                    } else {
                        debug!("Applied ICC profile to JPEG output");
                    }
                }
                
                encoder.encode(
                    image.as_raw(),
                    image.width(),
                    image.height(),
                    ExtendedColorType::Rgb8,
                )?;
            }
            ImageWriter::Generic => {
                // For generic format, we need to handle ICC profiles based on the file extension
                if let Some(profile) = icc_profile {
                    self.write_with_icc_profile(image, destination, profile)?;
                } else {
                    image.save(destination)?;
                }
            }
        };
        Ok(())
    }

    fn write_with_icc_profile<Pix: Pixel<Subpixel = u8> + PixelWithColorType>(
        &self,
        image: &CanvasBuffer<Pix>,
        destination: &Path,
        icc_profile: &Vec<u8>,
    ) -> ImageResult<()> {
        let extension = destination.extension().and_then(|s| s.to_str()).unwrap_or("");
        
        match extension.to_lowercase().as_str() {
            "png" => {
                Self::encode_with_icc_profile::<Pix, image::codecs::png::PngEncoder<BufWriter<File>>>(
                    image,
                    destination,
                    icc_profile,
                    image::codecs::png::PngEncoder::new,
                    "PNG"
                )
            }
            "tiff" | "tif" => {
                Self::encode_with_icc_profile::<Pix, image::codecs::tiff::TiffEncoder<BufWriter<File>>>(
                    image,
                    destination,
                    icc_profile,
                    image::codecs::tiff::TiffEncoder::new,
                    "TIFF"
                )
            }
            "webp" => {
                Self::encode_with_icc_profile::<Pix, image::codecs::webp::WebPEncoder<BufWriter<File>>>(
                    image,
                    destination,
                    icc_profile,
                    image::codecs::webp::WebPEncoder::new_lossless,
                    "WebP"
                )
            }
            _ => {
                // For other formats, fall back to the standard save method
                debug!("ICC profile not supported for format: {}", extension);
                image.save(destination)
            }
        }
    }

    fn encode_with_icc_profile<Pix, E>(
        image: &CanvasBuffer<Pix>,
        destination: &Path,
        icc_profile: &Vec<u8>,
        encoder_factory: fn(BufWriter<File>) -> E,
        format_name: &str,
    ) -> ImageResult<()>
    where
        Pix: Pixel<Subpixel = u8> + PixelWithColorType,
        E: ImageEncoder,
    {
        let file = File::create(destination)?;
        let fout = BufWriter::new(file);
        let mut encoder = encoder_factory(fout);
        
        if let Err(e) = encoder.set_icc_profile(icc_profile.clone()) {
            debug!("Failed to set ICC profile for {}: {}", format_name, e);
        } else {
            debug!("Applied ICC profile to {} output", format_name);
        }
        
        encoder.write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            Pix::COLOR_TYPE.into(),
        )
    }
}
