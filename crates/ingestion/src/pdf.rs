//! PDF validation and layout extraction through local poppler utilities.
//!
//! Poppler is invoked as an argument vector (never a shell). Files are bounded,
//! output is capped, and every command has a timeout. `pdftohtml -xml` uses its
//! default 1.5x zoom, so coordinates are converted back to PDF points by
//! dividing by 1.5. The XML page box, not `pdfinfo`, defines each page size so a
//! mixed-size PDF remains correct.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{process::Command, time::timeout};

pub const MAX_PDF_BYTES: u64 = 200 * 1024 * 1024;
pub const MAX_PAGES: u32 = 200;
pub const MAX_XML_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RENDER_PIXELS: u64 = 24_000_000;
pub const PDF2XML_SCALE: f32 = 1.5;

#[derive(Debug, Error)]
pub enum PdfError {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutImage {
    pub bbox: BBox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLayout {
    pub page_number: u32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub lines: Vec<LayoutLine>,
    pub images: Vec<LayoutImage>,
    pub text: String,
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
}

impl Default for PdfTools {
    fn default() -> Self {
        Self {
            pdfinfo: PathBuf::from("pdfinfo"),
            pdftohtml: PathBuf::from("pdftohtml"),
            pdftoppm: PathBuf::from("pdftoppm"),
            pdfimages: PathBuf::from("pdfimages"),
        }
    }
}

impl PdfTools {
    fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .env("LC_ALL", "C.UTF-8")
            .stdin(Stdio::null())
            .stderr(Stdio::null());
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
        let output = self
            .command(&self.pdfinfo)
            .arg("-enc")
            .arg("UTF-8")
            .arg(path)
            .output()
            .await
            .map_err(|_| PdfError::Tool)?;
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
        let output = timeout(
            Duration::from_secs(30),
            self.command(&self.pdftohtml)
                .arg("-xml")
                .arg("-i")
                .arg("-f")
                .arg(&page_text)
                .arg("-l")
                .arg(&page_text)
                .arg("-stdout")
                .arg(path)
                .output(),
        )
        .await
        .map_err(|_| PdfError::Timeout)?
        .map_err(|_| PdfError::Tool)?;
        if !output.status.success() {
            return Err(PdfError::Tool);
        }
        if output.stdout.len() > MAX_XML_BYTES {
            return Err(PdfError::Layout);
        }
        let xml = String::from_utf8_lossy(&output.stdout);
        parse_page_xml(&xml, page)
    }

    /// Count embedded raster images per page. Used to distinguish a genuinely
    /// blank page from an image-only scan: pdftohtml does not always emit an
    /// `<image>` element for a full-page scan.
    pub async fn image_counts(&self, path: &Path) -> Result<HashMap<u32, u32>, PdfError> {
        let output = timeout(
            Duration::from_secs(30),
            self.command(&self.pdfimages)
                .arg("-list")
                .arg(path)
                .output(),
        )
        .await
        .map_err(|_| PdfError::Timeout)?
        .map_err(|_| PdfError::Tool)?;
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
        let page_text = page.to_string();
        let dpi_text = dpi.to_string();
        let output = timeout(
            Duration::from_secs(60),
            self.command(&self.pdftoppm)
                .arg("-f")
                .arg(&page_text)
                .arg("-l")
                .arg(&page_text)
                .arg("-r")
                .arg(&dpi_text)
                .arg("-png")
                .arg("-singlefile")
                .arg(path)
                .arg(output_prefix)
                .output(),
        )
        .await
        .map_err(|_| PdfError::Timeout)?
        .map_err(|_| PdfError::Tool)?;
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
}

fn attribute(attributes: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = attributes.find(&needle)? + needle.len();
    let quote = attributes[start..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &attributes[start + quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_owned())
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
    decode_entities(&stripped).trim().to_owned()
}

fn parse_page_xml(xml: &str, page_number: u32) -> Result<PageLayout, PdfError> {
    let page_start = xml.find("<page ").ok_or(PdfError::Layout)?;
    let page_attributes_end = xml[page_start..]
        .find('>')
        .map(|offset| page_start + offset)
        .ok_or(PdfError::Layout)?;
    let page_attributes = &xml[page_start + 6..page_attributes_end];
    let raw_width = attribute(page_attributes, "width")
        .and_then(|value| value.parse::<f32>().ok())
        .ok_or(PdfError::Layout)?;
    let raw_height = attribute(page_attributes, "height")
        .and_then(|value| value.parse::<f32>().ok())
        .ok_or(PdfError::Layout)?;
    if raw_width <= 0.0 || raw_height <= 0.0 {
        return Err(PdfError::Layout);
    }
    let width_pt = raw_width / PDF2XML_SCALE;
    let height_pt = raw_height / PDF2XML_SCALE;

    let mut lines = Vec::new();
    let mut images = Vec::new();
    let mut cursor = page_attributes_end + 1;
    while cursor < xml.len() {
        let Some(relative) = xml[cursor..].find('<') else {
            break;
        };
        let tag_start = cursor + relative;
        let Some(close_offset) = xml[tag_start..].find('>') else {
            break;
        };
        let tag_end = tag_start + close_offset;
        let tag = &xml[tag_start + 1..tag_end];
        let tag_name = tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or_default();
        if tag_name == "text" && !tag.starts_with('/') {
            let Some(content_end) = xml[tag_end + 1..].find("</text>") else {
                return Err(PdfError::Layout);
            };
            let content = &xml[tag_end + 1..tag_end + 1 + content_end];
            let top = attribute(tag, "top")
                .and_then(|value| value.parse::<f32>().ok())
                .ok_or(PdfError::Layout)?;
            let left = attribute(tag, "left")
                .and_then(|value| value.parse::<f32>().ok())
                .ok_or(PdfError::Layout)?;
            let width = attribute(tag, "width")
                .and_then(|value| value.parse::<f32>().ok())
                .ok_or(PdfError::Layout)?;
            let height = attribute(tag, "height")
                .and_then(|value| value.parse::<f32>().ok())
                .ok_or(PdfError::Layout)?;
            let text = strip_tags(content);
            if !text.is_empty() {
                lines.push(LayoutLine {
                    text,
                    bbox: BBox {
                        x0: left / PDF2XML_SCALE,
                        y0: top / PDF2XML_SCALE,
                        x1: (left + width) / PDF2XML_SCALE,
                        y1: (top + height) / PDF2XML_SCALE,
                    },
                });
            }
            cursor = tag_end + 1 + content_end + "</text>".len();
            continue;
        }
        if tag_name == "image" || tag_name == "img" {
            let top = attribute(tag, "top")
                .and_then(|value| value.parse::<f32>().ok())
                .ok_or(PdfError::Layout)?;
            let left = attribute(tag, "left")
                .and_then(|value| value.parse::<f32>().ok())
                .ok_or(PdfError::Layout)?;
            let width = attribute(tag, "width")
                .and_then(|value| value.parse::<f32>().ok())
                .ok_or(PdfError::Layout)?;
            let height = attribute(tag, "height")
                .and_then(|value| value.parse::<f32>().ok())
                .ok_or(PdfError::Layout)?;
            images.push(LayoutImage {
                bbox: BBox {
                    x0: left / PDF2XML_SCALE,
                    y0: top / PDF2XML_SCALE,
                    x1: (left + width) / PDF2XML_SCALE,
                    y1: (top + height) / PDF2XML_SCALE,
                },
            });
        }
        cursor = tag_end + 1;
    }
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
<pdf2xml><page number="1" top="0" left="0" height="1188" width="918">
<text top="119" left="108" width="702" height="25" font="0">A &amp; B</text>
<text top="176" left="108" width="423" height="25" font="0"><b>42%</b></text>
<image top="10" left="20" width="100" height="50" src="x.png"/>
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
}
