use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageReader, ImageResult, Limits};
use std::io::Cursor;

pub struct Icon {
    pub full: Vec<u8>,
    pub thumb: Vec<u8>,
}

// Decodes jpeg/png/webp/gif (first frame), center-crops to square, returns 512px and 128px WebP.
pub fn make_icon(bytes: &[u8]) -> ImageResult<Icon> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    // Per-decode cap; aws_s3_utils limits how many decodes run at once.
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);

    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);

    let full = img.resize_to_fill(512, 512, FilterType::Lanczos3);
    let thumb = full.resize(128, 128, FilterType::Lanczos3);
    Ok(Icon {
        full: encode(&full)?,
        thumb: encode(&thumb)?,
    })
}

fn encode(img: &DynamicImage) -> ImageResult<Vec<u8>> {
    let mut out = Vec::new();
    img.write_with_encoder(WebPEncoder::new_lossless(&mut out))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};

    #[test]
    fn crops_and_resizes_to_webp() {
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(RgbImage::new(300, 200))
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();

        let icon = make_icon(&png).unwrap();
        for (bytes, size) in [(&icon.full, 512), (&icon.thumb, 128)] {
            let out = image::load_from_memory_with_format(bytes, ImageFormat::WebP).unwrap();
            assert_eq!((out.width(), out.height()), (size, size));
        }

        assert!(make_icon(b"not an image").is_err());
    }
}
