//! Bounded local PNG analysis and verification overlays. No image decoder is
//! given a buffer inferred from untrusted dimensions without checking the cap.
use crate::pdf::{BBox, MAX_RENDER_PIXELS, PdfError, pixel_bbox};
use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::Path,
};

pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub gray: Vec<u8>,
}

impl Raster {
    pub fn load(path: &Path) -> Result<Self, PdfError> {
        let file = File::open(path).map_err(|_| PdfError::Io)?;
        if file.metadata().map_err(|_| PdfError::Io)?.len() > 64 * 1024 * 1024 {
            return Err(PdfError::UnsafeRender);
        }
        let mut decoder = png::Decoder::new(BufReader::new(file));
        decoder.set_limits(png::Limits {
            bytes: 128 * 1024 * 1024,
        });
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|_| PdfError::Layout)?;
        let info = reader.info();
        let pixels = u64::from(info.width) * u64::from(info.height);
        if pixels == 0 || pixels > MAX_RENDER_PIXELS {
            return Err(PdfError::UnsafeRender);
        }
        let len = reader.output_buffer_size().ok_or(PdfError::UnsafeRender)?;
        if len > MAX_RENDER_PIXELS as usize * 4 {
            return Err(PdfError::UnsafeRender);
        }
        let mut bytes = vec![0; len];
        let output = reader
            .next_frame(&mut bytes)
            .map_err(|_| PdfError::Layout)?;
        if output.bit_depth != png::BitDepth::Eight {
            return Err(PdfError::Layout);
        }
        let channels = output.color_type.samples();
        let gray = bytes[..output.buffer_size()]
            .chunks_exact(channels)
            .map(|p| match output.color_type {
                png::ColorType::Rgb => {
                    ((u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) / 3) as u8
                }
                png::ColorType::Rgba => {
                    let value = (u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) / 3;
                    ((value * u32::from(p[3]) + 255 * (255 - u32::from(p[3]))) / 255) as u8
                }
                png::ColorType::Grayscale => p[0],
                png::ColorType::GrayscaleAlpha => {
                    ((u32::from(p[0]) * u32::from(p[1]) + 255 * (255 - u32::from(p[1]))) / 255)
                        as u8
                }
                _ => 0,
            })
            .collect::<Vec<_>>();
        if gray.len() != pixels as usize {
            return Err(PdfError::Layout);
        }
        Ok(Self {
            width: output.width,
            height: output.height,
            gray,
        })
    }

    pub fn variance(&self, bbox: BBox, size: (f32, f32), dpi: u32) -> Result<f64, PdfError> {
        let [x, y, w, h] = pixel_bbox(bbox, size.0, size.1, dpi)?;
        if x.checked_add(w).is_none_or(|end| end > self.width)
            || y.checked_add(h).is_none_or(|end| end > self.height)
        {
            return Err(PdfError::UnsafeRender);
        }
        let mut sum = 0.0;
        let mut sum2 = 0.0;
        for row in y..y + h {
            for col in x..x + w {
                let value = f64::from(self.gray[(row * self.width + col) as usize]) / 255.0;
                sum += value;
                sum2 += value * value;
            }
        }
        let n = f64::from(w) * f64::from(h);
        Ok((sum2 / n - (sum / n).powi(2)).max(0.0))
    }

    /// Grayscale analysis image with colored action rectangles; source remains
    /// separate and unchanged. All output stays inside the private run folder.
    pub fn overlay(
        &self,
        path: &Path,
        boxes: &[(BBox, [u8; 3])],
        size: (f32, f32),
        dpi: u32,
    ) -> Result<(), PdfError> {
        let mut rgb = self
            .gray
            .iter()
            .flat_map(|v| [*v, *v, *v])
            .collect::<Vec<_>>();
        for &(bbox, color) in boxes {
            let [x, y, w, h] = pixel_bbox(bbox, size.0, size.1, dpi)?;
            if x + w > self.width || y + h > self.height {
                return Err(PdfError::UnsafeRender);
            }
            for row in y..y + h {
                for col in x..x + w {
                    if row < y + 2
                        || row >= y + h.saturating_sub(2)
                        || col < x + 2
                        || col >= x + w.saturating_sub(2)
                    {
                        let at = ((row * self.width + col) * 3) as usize;
                        rgb[at..at + 3].copy_from_slice(&color);
                    }
                }
            }
        }
        let file = File::create(path).map_err(|_| PdfError::Io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| PdfError::Io)?;
        }
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(|_| PdfError::Io)?
            .write_image_data(&rgb)
            .map_err(|_| PdfError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analysis_variance_and_overlay_use_the_same_pixel_geometry() {
        let directory = tempfile::tempdir().unwrap();
        let mut raster = Raster {
            width: 2000,
            height: 1125,
            gray: vec![255; 2000 * 1125],
        };
        let bbox = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 960.0,
            y1: 540.0,
        };
        assert_eq!(raster.variance(bbox, (960.0, 540.0), 150).unwrap(), 0.0);
        raster.gray[100] = 0;
        assert!(raster.variance(bbox, (960.0, 540.0), 150).unwrap() > 0.0);
        let path = directory.path().join("overlay.png");
        raster
            .overlay(&path, &[(bbox, [255, 0, 0])], (960.0, 540.0), 150)
            .unwrap();
        let decoded = Raster::load(&path).unwrap();
        assert_eq!((decoded.width, decoded.height), (2000, 1125));
        assert_eq!(decoded.gray[0], 85);
    }
    #[test]
    fn malformed_png_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("invalid.png");
        std::fs::write(&path, b"not a PNG").unwrap();
        assert!(Raster::load(&path).is_err());
    }
}
