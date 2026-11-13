//! PDF validation and layout extraction through local poppler utilities.
//!
//! Poppler is invoked as an argument vector (never a shell). Files are bounded,
//! output is capped, and every command has a timeout. Text/font/image geometry
//! is extracted at explicit 72 DPI, then normalized to the rotated CropBox.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use quick_xml::{
    Reader, XmlVersion,
    events::{BytesStart, Event},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::process::Command;

use crate::process::{HelperError, HelperLimits, run_helper};

pub const MAX_PDF_BYTES: u64 = 200 * 1024 * 1024;
pub const MAX_PAGES: u32 = 200;
pub const MAX_XML_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RENDER_PIXELS: u64 = 24_000_000;
pub const PDF2XML_SCALE: f32 = 1.0;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error(transparent)]
    Helper(#[from] HelperError),
    #[error("pdf file could not be read")]
    Io,
    #[error("the source is larger than the 200 MiB limit")]
    TooLarge,
    #[error("the file is not a PDF")]
    NotPdf,
    #[error("the PDF is encrypted")]
    Encrypted,
    #[error("the PDF has no pages")]
    NoPages,
    #[error("the PDF exceeds the page limit")]
    PageLimit,
    #[error("the PDF tool failed")]
    Tool,
    #[error("the PDF tool timed out")]
    Timeout,
    #[error("the PDF layout output is malformed")]
    Layout,
    #[error("the requested page is out of range")]
    PageOutOfRange,
    #[error("the render request is outside the safety bounds")]
    UnsafeRender,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfInfo {
    pub page_count: u32,
    pub encrypted: bool,
    pub page_width_pt: f32,
    pub page_height_pt: f32,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl BBox {
    pub fn is_valid(self) -> bool {
        [self.x0, self.y0, self.x1, self.y1]
            .iter()
            .all(|v| v.is_finite())
            && self.x1 > self.x0
            && self.y1 > self.y0
    }

    pub fn area(self) -> f32 {
        self.width() * self.height()
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let bbox = Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        };
        bbox.is_valid().then_some(bbox)
    }

    pub fn width(self) -> f32 {
        (self.x1 - self.x0).max(0.0)
    }

    pub fn height(self) -> f32 {
        (self.y1 - self.y0).max(0.0)
    }

    pub fn padded(self, padding: f32, page_width: f32, page_height: f32) -> Self {
        Self {
            x0: (self.x0 - padding).max(0.0),
            y0: (self.y0 - padding).max(0.0),
            x1: (self.x1 + padding).min(page_width),
            y1: (self.y1 + padding).min(page_height),
        }
    }

    pub fn intersects(self, other: Self) -> bool {
        self.x0 < other.x1 && self.x1 > other.x0 && self.y0 < other.y1 && self.y1 > other.y0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutLine {
    pub text: String,
    pub bbox: BBox,
    #[serde(default)]
    pub font_family: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutImage {
    pub bbox: BBox,
    /// Exact encoded-image identity, never the data URL or private image bytes.
    #[serde(default)]
    pub source_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLayout {
    pub page_number: u32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub lines: Vec<LayoutLine>,
    pub images: Vec<LayoutImage>,
    pub text: String,
    #[serde(default)]
    pub geometry: Option<PageGeometry>,
    #[serde(default)]
    pub geometry_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageGeometry {
    pub media_box: BBox,
    pub crop_box: BBox,
    pub rotation: u16,
}

impl PageGeometry {
    /// Unrotated PDF user space (bottom-left) -> rotated media-space (top-left).
    pub fn display_bbox(&self, bbox: BBox) -> BBox {
        let point = |x: f32, y: f32| match self.rotation {
            0 => (x - self.media_box.x0, self.media_box.y1 - y),
            90 => (y - self.media_box.y0, x - self.media_box.x0),
            180 => (self.media_box.x1 - x, y - self.media_box.y0),
            270 => (self.media_box.y1 - y, self.media_box.x1 - x),
            _ => unreachable!("validated PDF rotation"),
        };
        let points = [
            point(bbox.x0, bbox.y0),
            point(bbox.x0, bbox.y1),
            point(bbox.x1, bbox.y0),
            point(bbox.x1, bbox.y1),
        ];
        BBox {
            x0: points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min),
            y0: points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min),
            x1: points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max),
            y1: points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max),
        }
    }

    pub fn display_size(&self) -> (f32, f32) {
        let crop = self.display_bbox(self.crop_box);
        (crop.width(), crop.height())
    }

    fn normalize(&self, bbox: BBox) -> Option<BBox> {
        let crop = self.display_bbox(self.crop_box);
        bbox.intersection(crop).map(|b| BBox {
            x0: b.x0 - crop.x0,
            y0: b.y0 - crop.y0,
            x1: b.x1 - crop.x0,
            y1: b.y1 - crop.y0,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageRoute {
    Digital,
    Ocr,
    Hybrid,
    Blank,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageFeatures {
    pub route: PageRoute,
    pub chars: u32,
    pub words: u32,
    pub image_count: u32,
    pub math_score: f32,
}

#[derive(Debug, Clone)]
pub struct PdfTools {
    pub pdfinfo: PathBuf,
    pub pdftohtml: PathBuf,
    pub pdftoppm: PathBuf,
    pub pdfimages: PathBuf,
    pub pdftotext: PathBuf,
}

impl Default for PdfTools {
    fn default() -> Self {
        Self {
            pdfinfo: PathBuf::from("pdfinfo"),
            pdftohtml: PathBuf::from("pdftohtml"),
            pdftoppm: PathBuf::from("pdftoppm"),
            pdfimages: PathBuf::from("pdfimages"),
            pdftotext: PathBuf::from("pdftotext"),
        }
    }
}

impl PdfTools {
    fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        command.env("LC_ALL", "C.UTF-8");
        command
    }

    pub async fn validate(&self, path: &Path) -> Result<PdfInfo, PdfError> {
        let metadata = tokio::fs::metadata(path).await.map_err(|_| PdfError::Io)?;
        if metadata.len() == 0 || metadata.len() > MAX_PDF_BYTES {
            return Err(PdfError::TooLarge);
        }
        let bytes = tokio::fs::read(path).await.map_err(|_| PdfError::Io)?;
        if !bytes.starts_with(b"%PDF-") {
            return Err(PdfError::NotPdf);
        }
        let tail_start = bytes.len().saturating_sub(4096);
        if !bytes[tail_start..]
            .windows(5)
            .any(|window| window == b"%%EOF")
        {
            return Err(PdfError::NotPdf);
        }
        let output = run_helper(
            self.command(&self.pdfinfo)
                .arg("-enc")
                .arg("UTF-8")
                .arg(path),
            HelperLimits::pdf(Duration::from_secs(15), 64 * 1024),
        )
        .await?;
        if !output.status.success() {
            return Err(PdfError::Tool);
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut page_count = None;
        let mut encrypted = None;
        let mut page_size = None;
        let mut title = None;
        for line in text.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim();
                match key.trim() {
                    "Pages" => page_count = value.parse::<u32>().ok(),
                    "Encrypted" => encrypted = Some(value.eq_ignore_ascii_case("yes")),
                    "Page size" => {
                        let mut numbers = value
                            .split_whitespace()
                            .filter_map(|part| part.parse::<f32>().ok());
                        if let (Some(width), Some(height)) = (numbers.next(), numbers.next()) {
                            page_size = Some((width, height));
                        }
                    }
                    "Title" if !value.is_empty() => title = Some(value.to_owned()),
                    _ => {}
                }
            }
        }
        let page_count = page_count.ok_or(PdfError::NoPages)?;
        if page_count == 0 {
            return Err(PdfError::NoPages);
        }
        if page_count > MAX_PAGES {
            return Err(PdfError::PageLimit);
        }
        if encrypted.unwrap_or(false) {
            return Err(PdfError::Encrypted);
        }
        let (page_width_pt, page_height_pt) = page_size.ok_or(PdfError::Layout)?;
        Ok(PdfInfo {
            page_count,
            encrypted: false,
            page_width_pt,
            page_height_pt,
            title,
        })
    }

    /// Extract one page as XML. The subprocess is bounded in time and output.
    pub async fn layout(&self, path: &Path, page: u32) -> Result<PageLayout, PdfError> {
        if page == 0 {
            return Err(PdfError::PageOutOfRange);
        }
        let page_text = page.to_string();
        let output = run_helper(
            self.command(&self.pdftohtml)
                .arg("-xml")
                .args([
                    "-zoom",
                    "1",
                    "-noroundcoord",
                    "-fontfullname",
                    "-dataurls",
                    "-nomerge",
                ])
                .arg("-f")
                .arg(&page_text)
                .arg("-l")
                .arg(&page_text)
                .arg("-stdout")
                .arg(path),
            HelperLimits::pdf(Duration::from_secs(30), MAX_XML_BYTES),
        )
        .await?;
        if !output.status.success() {
            return Err(PdfError::Tool);
        }
        let xml = String::from_utf8_lossy(&output.stdout);
        let mut layout = parse_page_xml(&xml, page)?;
        let geometry = self.page_geometry(path, page).await?;
        let (width, height) = geometry.display_size();
        // pdftohtml emits degenerate zero-width boxes for 90/270-degree text.
        // Poppler's word-box extractor supplies accurate rotated geometry.
        if geometry.rotation != 0 {
            let output = run_helper(
                self.command(&self.pdftotext)
                    .args([
                        "-bbox-layout",
                        "-cropbox",
                        "-f",
                        &page_text,
                        "-l",
                        &page_text,
                    ])
                    .arg(path)
                    .arg("-"),
                HelperLimits::pdf(Duration::from_secs(30), MAX_XML_BYTES),
            )
            .await?;
            if !output.status.success() {
                return Err(PdfError::Tool);
            }
            let mut boxes = parse_word_lines(&String::from_utf8_lossy(&output.stdout))?;
            for line in &mut layout.lines {
                let key = text_key(&line.text);
                if let Some(index) = boxes.iter().position(|(text, _)| text_key(text) == key) {
                    line.bbox = boxes.remove(index).1;
                } else {
                    layout
                        .geometry_warnings
                        .push("rotated_text_geometry_unmatched".into());
                    line.bbox = BBox {
                        x0: 0.0,
                        y0: 0.0,
                        x1: 0.0,
                        y1: 0.0,
                    };
                }
            }
        } else {
            layout.lines.retain_mut(|line| {
                if let Some(bbox) = geometry.normalize(line.bbox) {
                    line.bbox = bbox;
                    true
                } else {
                    false
                }
            });
        }
        layout.images.retain_mut(|image| {
            if let Some(bbox) = geometry.normalize(image.bbox) {
                image.bbox = bbox;
                true
            } else {
                false
            }
        });
        layout.width_pt = width;
        layout.height_pt = height;
        layout.geometry = Some(geometry);
        layout.text = layout
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(layout)
    }

    pub async fn page_geometry(&self, path: &Path, page: u32) -> Result<PageGeometry, PdfError> {
        if page == 0 {
            return Err(PdfError::PageOutOfRange);
        }
        let page = page.to_string();
        let output = run_helper(
            self.command(&self.pdfinfo)
                .args(["-box", "-f", &page, "-l", &page])
                .arg(path),
            HelperLimits::pdf(Duration::from_secs(15), 64 * 1024),
        )
        .await?;
        if !output.status.success() {
            return Err(PdfError::Tool);
        }
        parse_geometry(&String::from_utf8_lossy(&output.stdout))
    }

    /// Count embedded raster images per page. Used to distinguish a genuinely
    /// blank page from an image-only scan: pdftohtml does not always emit an
    /// `<image>` element for a full-page scan.
    pub async fn image_counts(&self, path: &Path) -> Result<HashMap<u32, u32>, PdfError> {
        let output = run_helper(
            self.command(&self.pdfimages).arg("-list").arg(path),
            HelperLimits::pdf(Duration::from_secs(30), MAX_XML_BYTES),
        )
        .await?;
        if !output.status.success() {
            return Err(PdfError::Tool);
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut counts = HashMap::new();
        for line in text.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 5 || fields[0] == "page" {
                continue;
            }
            if let Ok(page) = fields[0].parse::<u32>() {
                *counts.entry(page).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }

    /// Render a page to PNG at the requested DPI. The caller owns the output
    /// prefix; poppler appends `.png` with `-singlefile`.
    pub async fn render_png(
        &self,
        path: &Path,
        page: u32,
        dpi: u32,
        output_prefix: &Path,
    ) -> Result<PathBuf, PdfError> {
        if !(72..=300).contains(&dpi) || page == 0 {
            return Err(PdfError::UnsafeRender);
        }
        let geometry = self.page_geometry(path, page).await?;
        let (width, height) = geometry.display_size();
        let _ = pixel_bbox(
            BBox {
                x0: 0.0,
                y0: 0.0,
                x1: width,
                y1: height,
            },
            width,
            height,
            dpi,
        )?;
        let page_text = page.to_string();
        let dpi_text = dpi.to_string();
        let output = run_helper(
            self.command(&self.pdftoppm)
                .arg("-f")
                .arg(&page_text)
                .arg("-l")
                .arg(&page_text)
                .arg("-r")
                .arg(&dpi_text)
                .arg("-png")
                .arg("-cropbox")
                .arg("-singlefile")
                .arg(path)
                .arg(output_prefix),
            HelperLimits::pdf(Duration::from_secs(60), 64 * 1024),
        )
        .await?;
        if !output.status.success() {
            return Err(PdfError::Tool);
        }
        let output_path = output_prefix.with_extension("png");
        let metadata = tokio::fs::metadata(&output_path)
            .await
            .map_err(|_| PdfError::Io)?;
        if metadata.len() == 0 || metadata.len() > 64 * 1024 * 1024 {
            return Err(PdfError::UnsafeRender);
        }
        Ok(output_path)
    }

    /// Render only a selected source rectangle, never upscale a 150-DPI image.
    pub async fn render_region_png(
        &self,
        path: &Path,
        page: u32,
        bbox: BBox,
        size: (f32, f32),
        output_prefix: &Path,
    ) -> Result<PathBuf, PdfError> {
        let [x, y, w, h] = pixel_bbox(bbox, size.0, size.1, 250)?;
        if page == 0 || w > crate::crops::MAX_CROP_DIMENSION || h > crate::crops::MAX_CROP_DIMENSION
        {
            return Err(PdfError::UnsafeRender);
        }
        let output = run_helper(
            self.command(&self.pdftoppm)
                .args([
                    "-f",
                    &page.to_string(),
                    "-l",
                    &page.to_string(),
                    "-r",
                    "250",
                    "-cropbox",
                    "-x",
                    &x.to_string(),
                    "-y",
                    &y.to_string(),
                    "-W",
                    &w.to_string(),
                    "-H",
                    &h.to_string(),
                    "-png",
                    "-singlefile",
                ])
                .arg(path)
                .arg(output_prefix),
            HelperLimits::pdf(Duration::from_secs(60), 64 * 1024),
        )
        .await?;
        if !output.status.success() {
            return Err(PdfError::Tool);
        }
        let png = output_prefix.with_extension("png");
        let metadata = tokio::fs::metadata(&png).await.map_err(|_| PdfError::Io)?;
        if metadata.len() == 0 || metadata.len() > 64 * 1024 * 1024 {
            return Err(PdfError::UnsafeRender);
        }
        Ok(png)
    }
}

/// One canonical point -> pixel mapping for analysis overlays and 250-DPI crops.
pub fn pixel_bbox(bbox: BBox, width: f32, height: f32, dpi: u32) -> Result<[u32; 4], PdfError> {
    if !bbox.is_valid()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || bbox.x0 < 0.0
        || bbox.y0 < 0.0
        || bbox.x1 > width
        || bbox.y1 > height
        || !(72..=300).contains(&dpi)
    {
        return Err(PdfError::UnsafeRender);
    }
    // Multiply before division: 960 * (150 / 72) can round just above 2000,
    // wrongly requesting an extra pixel; (960 * 150) / 72 is exactly 2000.
    let pixel = |v: f32| f64::from(v) * f64::from(dpi) / 72.0;
    let x = pixel(bbox.x0).floor();
    let y = pixel(bbox.y0).floor();
    let w = pixel(bbox.x1).ceil() - x;
    let h = pixel(bbox.y1).ceil() - y;
    if x > f64::from(u32::MAX)
        || y > f64::from(u32::MAX)
        || w < 1.0
        || h < 1.0
        || w * h > MAX_RENDER_PIXELS as f64
    {
        return Err(PdfError::UnsafeRender);
    }
    Ok([x as u32, y as u32, w as u32, h as u32])
}

fn xml_attrs(
    reader: &Reader<&[u8]>,
    start: &BytesStart<'_>,
) -> Result<HashMap<String, String>, PdfError> {
    start
        .attributes()
        .map(|attr| {
            let attr = attr.map_err(|_| PdfError::Layout)?;
            Ok((
                String::from_utf8(attr.key.as_ref().to_vec()).map_err(|_| PdfError::Layout)?,
                attr.decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                    .map_err(|_| PdfError::Layout)?
                    .into_owned(),
            ))
        })
        .collect()
}

fn number(attrs: &HashMap<String, String>, key: &str) -> Result<f32, PdfError> {
    let number = attrs
        .get(key)
        .and_then(|v| v.parse::<f32>().ok())
        .ok_or(PdfError::Layout)?;
    if !number.is_finite() {
        return Err(PdfError::Layout);
    }
    Ok(number)
}

fn text_key(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn parse_geometry(text: &str) -> Result<PageGeometry, PdfError> {
    let mut media = None;
    let mut crop = None;
    let mut rotation = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.ends_with("MediaBox") || key.ends_with("CropBox") {
            let v = value
                .split_whitespace()
                .map(str::parse::<f32>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| PdfError::Layout)?;
            if v.len() != 4 {
                return Err(PdfError::Layout);
            }
            let bbox = BBox {
                x0: v[0],
                y0: v[1],
                x1: v[2],
                y1: v[3],
            };
            if !bbox.is_valid() {
                return Err(PdfError::Layout);
            }
            if key.ends_with("MediaBox") {
                media = Some(bbox)
            } else {
                crop = Some(bbox)
            }
        } else if key.ends_with("rot") {
            let r = value
                .trim()
                .parse::<i32>()
                .map_err(|_| PdfError::Layout)?
                .rem_euclid(360);
            if ![0, 90, 180, 270].contains(&r) {
                return Err(PdfError::Layout);
            }
            rotation = Some(r as u16);
        }
    }
    let media_box = media.ok_or(PdfError::Layout)?;
    let crop_box = crop
        .ok_or(PdfError::Layout)?
        .intersection(media_box)
        .ok_or(PdfError::Layout)?;
    Ok(PageGeometry {
        media_box,
        crop_box,
        rotation: rotation.ok_or(PdfError::Layout)?,
    })
}

fn parse_word_lines(xml: &str) -> Result<Vec<(String, BBox)>, PdfError> {
    let mut reader = Reader::from_str(xml);
    let mut lines = Vec::new();
    let mut current = None;
    let mut text = String::new();
    loop {
        match reader.read_event().map_err(|_| PdfError::Layout)? {
            Event::Start(start) if start.name().as_ref() == b"line" => {
                let attrs = xml_attrs(&reader, &start)?;
                current = Some(BBox {
                    x0: number(&attrs, "xMin")?,
                    y0: number(&attrs, "yMin")?,
                    x1: number(&attrs, "xMax")?,
                    y1: number(&attrs, "yMax")?,
                });
                text.clear();
            }
            Event::Start(start) if start.name().as_ref() == b"word" => {
                let value = reader
                    .read_text(start.name())
                    .map_err(|_| PdfError::Layout)?;
                if !text.is_empty() {
                    text.push(' ')
                }
                text.push_str(&decode_entities(
                    &value.decode().map_err(|_| PdfError::Layout)?,
                ));
            }
            Event::End(end) if end.name().as_ref() == b"line" => {
                let bbox = current.take().ok_or(PdfError::Layout)?;
                if !bbox.is_valid() {
                    return Err(PdfError::Layout);
                }
                lines.push((text.clone(), bbox));
            }
            Event::DocType(dtd) if dtd.as_ref().contains(&b'[') => return Err(PdfError::Layout),
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(lines)
}

fn decode_entities(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut index = 0;
    while let Some(offset) = text[index..].find('&') {
        let start = index + offset;
        decoded.push_str(&text[index..start]);
        let Some(end_offset) = text[start..].find(';') else {
            decoded.push_str(&text[start..]);
            return decoded;
        };
        let end = start + end_offset;
        let entity = &text[start + 1..end];
        let replacement = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if entity.starts_with("#x") => u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32),
            _ if entity.starts_with('#') => {
                entity[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        if let Some(character) = replacement {
            decoded.push(character);
        } else {
            decoded.push_str(&text[start..=end]);
        }
        index = end + 1;
    }
    decoded.push_str(&text[index..]);
    decoded
}

fn strip_tags(text: &str) -> String {
    let mut stripped = String::with_capacity(text.len());
    let mut inside = false;
    for character in text.chars() {
        match character {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => stripped.push(character),
            _ => {}
        }
    }
    decode_entities(&stripped)
}

fn parse_page_xml(xml: &str, page_number: u32) -> Result<PageLayout, PdfError> {
    let mut reader = Reader::from_str(xml);
    let mut size = None;
    let mut fonts = HashMap::new();
    let mut lines = Vec::new();
    let mut images = Vec::new();
    loop {
        match reader.read_event().map_err(|_| PdfError::Layout)? {
            Event::Start(start) | Event::Empty(start) => {
                let attrs = xml_attrs(&reader, &start)?;
                match start.name().as_ref() {
                    b"page" => {
                        if size.is_some() {
                            return Err(PdfError::Layout);
                        }
                        let width = number(&attrs, "width")?;
                        let height = number(&attrs, "height")?;
                        if width <= 0.0 || height <= 0.0 {
                            return Err(PdfError::Layout);
                        }
                        size = Some((width, height));
                    }
                    b"fontspec" => {
                        if let (Some(id), Some(family)) = (attrs.get("id"), attrs.get("family")) {
                            fonts.insert(id.clone(), family.clone());
                        }
                    }
                    b"text" | b"image" | b"img" => {
                        let x = number(&attrs, "left")?;
                        let y = number(&attrs, "top")?;
                        let w = number(&attrs, "width")?;
                        let h = number(&attrs, "height")?;
                        let bbox = BBox {
                            x0: x.min(x + w),
                            y0: y.min(y + h),
                            x1: x.max(x + w),
                            y1: y.max(y + h),
                        };
                        if start.name().as_ref() == b"text" {
                            let content = reader
                                .read_text(start.name())
                                .map_err(|_| PdfError::Layout)?;
                            let text = strip_tags(&content.decode().map_err(|_| PdfError::Layout)?);
                            if !text.trim().is_empty() {
                                lines.push(LayoutLine {
                                    text,
                                    bbox,
                                    font_family: attrs
                                        .get("font")
                                        .and_then(|id| fonts.get(id))
                                        .cloned(),
                                });
                            }
                        } else {
                            let src = attrs.get("src").ok_or(PdfError::Layout)?;
                            if !src.starts_with("data:image/") {
                                return Err(PdfError::Layout);
                            }
                            images.push(LayoutImage {
                                bbox,
                                source_sha256: hex::encode(Sha256::digest(src.as_bytes())),
                            });
                        }
                    }
                    _ => {}
                }
            }
            Event::DocType(dtd) if dtd.as_ref().contains(&b'[') => return Err(PdfError::Layout),
            Event::Eof => break,
            _ => {}
        }
    }
    let (width_pt, height_pt) = size.ok_or(PdfError::Layout)?;
    lines.sort_by(|left, right| {
        left.bbox
            .y0
            .total_cmp(&right.bbox.y0)
            .then(left.bbox.x0.total_cmp(&right.bbox.x0))
    });
    let text = lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(PageLayout {
        page_number,
        width_pt,
        height_pt,
        lines,
        images,
        text,
        geometry: None,
        geometry_warnings: Vec::new(),
    })
}

pub fn classify_page(layout: &PageLayout, embedded_images: u32) -> PageFeatures {
    let chars = layout
        .text
        .chars()
        .filter(|character| !character.is_whitespace())
        .count() as u32;
    let words = layout
        .text
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .count() as u32;
    let math_symbols = layout
        .text
        .chars()
        .filter(|character| {
            matches!(
                character,
                '=' | '^'
                    | '+'
                    | '×'
                    | '÷'
                    | '√'
                    | '∑'
                    | '∫'
                    | '≤'
                    | '≥'
                    | '≠'
                    | '≈'
                    | '±'
                    | '∞'
                    | 'π'
            )
        })
        .count() as f32;
    let math_score = if words == 0 {
        0.0
    } else {
        (math_symbols / words as f32).min(1.0)
    };
    let image_count = embedded_images.max(layout.images.len() as u32);
    let route = if chars == 0 && image_count == 0 {
        PageRoute::Blank
    } else if chars < 80 {
        PageRoute::Ocr
    } else if image_count > 0 && chars < 260 && math_score >= 0.12 {
        PageRoute::Hybrid
    } else {
        PageRoute::Digital
    };
    PageFeatures {
        route,
        chars,
        words,
        image_count,
        math_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_and_image_boxes_and_entities() {
        let xml = r#"<?xml version="1.0"?>
<pdf2xml><page number="1" top="0" left="0" height="792" width="612">
<text top="80" left="72" width="468" height="16" font="0">A &amp; B</text>
<text top="117" left="72" width="282" height="16" font="0"><b>42%</b></text>
<image top="10" left="20" width="60" height="50" src="data:image/png;base64,AA=="/>
</page></pdf2xml>"#;
        let layout = parse_page_xml(xml, 1).unwrap();
        assert_eq!(layout.width_pt, 612.0);
        assert_eq!(layout.height_pt, 792.0);
        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[0].text, "A & B");
        assert_eq!(layout.lines[1].text, "42%");
        assert_eq!(layout.lines[0].bbox.x0, 72.0);
        assert_eq!(layout.images.len(), 1);
        assert_eq!(layout.images[0].bbox.x1, 80.0);
    }

    #[test]
    fn classification_routes_blank_image_and_digital_pages() {
        let empty = PageLayout {
            page_number: 1,
            width_pt: 612.0,
            height_pt: 792.0,
            lines: Vec::new(),
            images: Vec::new(),
            text: String::new(),
            geometry: None,
            geometry_warnings: Vec::new(),
        };
        assert_eq!(classify_page(&empty, 0).route, PageRoute::Blank);
        assert_eq!(classify_page(&empty, 1).route, PageRoute::Ocr);
        let digital = PageLayout {
            text: "Photosynthesis converts light energy into chemical energy across the leaf, and the reaction yield is measured in joules for this experiment.".into(),
            ..empty
        };
        assert_eq!(classify_page(&digital, 0).route, PageRoute::Digital);
    }

    /// Poppler-backed check against the committed M2 fixture.
    #[tokio::test]
    #[ignore = "requires local poppler binaries"]
    async fn annotated_fixture_layout_has_all_pages() {
        let tools = PdfTools::default();
        let path = crate::project_root().join("tests/fixtures/m2/annotated-english.pdf");
        let info = tools.validate(&path).await.unwrap();
        assert_eq!(info.page_count, 4);
        let counts = tools.image_counts(&path).await.unwrap();
        assert_eq!(counts.get(&3).copied(), Some(1));
        for page in 1..=4 {
            let layout = tools
                .layout(&path, page)
                .await
                .unwrap_or_else(|error| panic!("page {page}: {error:?}"));
            assert!((layout.width_pt - 612.0).abs() < 0.01);
            assert!((layout.height_pt - 792.0).abs() < 0.01);
        }
    }

    #[test]
    fn rejects_invalid_geometry_and_pixel_requests_without_silent_clipping() {
        assert!(
            parse_geometry("MediaBox: 0 0 100 100\nCropBox: 0 0 100 100\nPage rot: 45").is_err()
        );
        assert!(
            parse_geometry("MediaBox: 0 0 NaN 100\nCropBox: 0 0 100 100\nPage rot: 0").is_err()
        );
        assert!(
            pixel_bbox(
                BBox {
                    x0: -1.0,
                    y0: 0.0,
                    x1: 20.0,
                    y1: 20.0
                },
                100.0,
                100.0,
                250
            )
            .is_err()
        );
        assert!(
            pixel_bbox(
                BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 10000.0,
                    y1: 10000.0
                },
                10000.0,
                10000.0,
                150
            )
            .is_err()
        );
        assert_eq!(
            pixel_bbox(
                BBox {
                    x0: 1.0,
                    y0: 2.0,
                    x1: 11.0,
                    y1: 12.0
                },
                100.0,
                100.0,
                250
            )
            .unwrap(),
            [3, 6, 36, 36]
        );
        assert_eq!(
            pixel_bbox(
                BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 960.0,
                    y1: 540.0
                },
                960.0,
                540.0,
                150
            )
            .unwrap(),
            [0, 0, 2000, 1125]
        );
    }

    #[test]
    fn xml_keeps_indentation_fonts_and_rejects_entities_and_nonfinite_boxes() {
        let xml = r#"<pdf2xml><page number="1" width="100" height="100"><fontspec id="0" family="Symbol"/>
        <text top="1" left="2" width="30" height="10" font="0">    x &lt; 5</text></page></pdf2xml>"#;
        let layout = parse_page_xml(xml, 1).unwrap();
        assert_eq!(layout.lines[0].text, "    x < 5");
        assert_eq!(layout.lines[0].font_family.as_deref(), Some("Symbol"));
        assert!(parse_page_xml(&xml.replace("width=\"30\"", "width=\"NaN\""), 1).is_err());
        assert!(parse_page_xml("<!DOCTYPE a [<!ENTITY b SYSTEM 'file:///etc/passwd'>]><page width='100' height='100'/>",1).is_err());
    }

    #[tokio::test]
    #[ignore = "requires Poppler/ImageMagick and delegated helper cgroup"]
    async fn rotated_cropbox_geometry_and_250_dpi_source_crops_are_aligned() {
        use crate::{
            crops::{CropTools, Region, RegionKind, crop_pdf_region},
            raster::Raster,
        };
        let tools = PdfTools::default();
        let source = crate::project_root().join("tests/fixtures/m2/region-cases.pdf");
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("pages")).unwrap();
        std::fs::create_dir(directory.path().join("crops")).unwrap();
        for (page, bbox) in [
            (
                1,
                BBox {
                    x0: 40.0,
                    y0: 150.0,
                    x1: 240.0,
                    y1: 230.0,
                },
            ),
            (
                2,
                BBox {
                    x0: 70.0,
                    y0: 40.0,
                    x1: 150.0,
                    y1: 240.0,
                },
            ),
            (
                3,
                BBox {
                    x0: 260.0,
                    y0: 70.0,
                    x1: 460.0,
                    y1: 150.0,
                },
            ),
            (
                4,
                BBox {
                    x0: 150.0,
                    y0: 260.0,
                    x1: 230.0,
                    y1: 460.0,
                },
            ),
        ] {
            let layout = tools.layout(&source, page).await.unwrap();
            assert!(
                layout.geometry_warnings.is_empty(),
                "page {page}: {:?}",
                layout.geometry_warnings
            );
            assert_eq!(
                (layout.width_pt, layout.height_pt),
                if page % 2 == 0 {
                    (300.0, 500.0)
                } else {
                    (500.0, 300.0)
                }
            );
            assert!(
                layout
                    .lines
                    .iter()
                    .any(|line| line.font_family.as_deref() == Some("Symbol"))
            );
            assert!(layout.lines.iter().all(|line| line.bbox.is_valid()));
            assert!(layout.text.contains("    return x"));
            assert_eq!(layout.images.len(), 1);
            let (crop, png) = crop_pdf_region(
                &CropTools::default(),
                &source,
                &layout,
                directory.path(),
                uuid::Uuid::new_v4(),
                "synthetic",
                &Region {
                    index: 0,
                    kind: RegionKind::Image,
                    bbox,
                    text: None,
                },
            )
            .await
            .unwrap();
            assert_eq!(crop.dpi, 250);
            assert_eq!(crop.bbox, bbox);
            let raster = Raster::load(&png).unwrap();
            assert_eq!((crop.width, crop.height), (raster.width, raster.height));
            let center =
                raster.gray[((raster.height / 2) * raster.width + raster.width / 2) as usize];
            assert!(
                (i32::from(center) - 85).abs() <= 3,
                "source rectangle must be blue at every rotation, got {center}"
            );
        }
    }
}
