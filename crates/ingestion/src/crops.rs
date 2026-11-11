//! Region selection and private WebP crop generation.
//!
//! Digital text boxes come from pdftohtml. Image-only pages use tesseract TSV
//! only for line geometry; the authoritative OCR text still comes from the
//! resident PaddleOCR-VL model. Each crop records both PDF-point and rendered
//! pixel coordinates, plus a digest and byte size.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::process::Command;
use uuid::Uuid;

use crate::pdf::{BBox, LayoutLine, PageLayout, PageRoute, PdfTools};

pub const MAX_CROPS_PER_PAGE: usize = 60;
pub const MAX_CROP_DIMENSION: u32 = 4096;
pub const MIN_CROP_DIMENSION: u32 = 8;
pub const MAX_CROP_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum CropError {
    #[error("the crop tool is unavailable")]
    Tool,
    #[error("the crop tool timed out")]
    Timeout,
    #[error("the crop request is outside the safety bounds")]
    Unsafe,
    #[error("the WebP encoder failed")]
    Encode,
    #[error("the crop file is not a valid WebP")]
    InvalidWebp,
    #[error("the crop could not be written")]
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    Text,
    Image,
    OcrLine,
    Page,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub index: u32,
    pub kind: RegionKind,
    pub bbox: BBox,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropOutput {
    pub id: Uuid,
    pub region_index: u32,
    pub kind: RegionKind,
    pub bbox: BBox,
    pub pixel_bbox: [u32; 4],
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
    pub sha256: String,
    pub bytes: u64,
    pub relative_path: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CropTools {
    pub magick: PathBuf,
    pub tesseract: PathBuf,
    pub pdf: PdfTools,
}

impl Default for CropTools {
    fn default() -> Self {
        Self {
            magick: PathBuf::from("magick"),
            tesseract: PathBuf::from("tesseract"),
            pdf: PdfTools::default(),
        }
    }
}

fn grouped_line_regions(layout: &PageLayout) -> Vec<Region> {
    let mut groups: Vec<(BBox, Vec<&LayoutLine>)> = Vec::new();
    for line in &layout.lines {
        if line.text.trim().is_empty() || line.bbox.width() < 2.0 || line.bbox.height() < 2.0 {
            continue;
        }
        let merge = groups.last().is_some_and(|(bbox, _)| {
            let vertical_gap = line.bbox.y0 - bbox.y1;
            let overlap = line.bbox.x0 < bbox.x1 && line.bbox.x1 > bbox.x0;
            vertical_gap <= bbox.height().max(line.bbox.height()) * 0.8 && overlap
        });
        if merge {
            let (bbox, lines) = groups.last_mut().expect("group exists");
            bbox.x0 = bbox.x0.min(line.bbox.x0);
            bbox.y0 = bbox.y0.min(line.bbox.y0);
            bbox.x1 = bbox.x1.max(line.bbox.x1);
            bbox.y1 = bbox.y1.max(line.bbox.y1);
            lines.push(line);
        } else {
            groups.push((line.bbox, vec![line]));
        }
    }
    groups
        .into_iter()
        .enumerate()
        .map(|(index, (bbox, lines))| Region {
            index: index as u32,
            kind: RegionKind::Text,
            bbox: bbox.padded(3.0, layout.width_pt, layout.height_pt),
            text: Some(
                lines
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        })
        .collect()
}

fn image_regions(layout: &PageLayout) -> Vec<Region> {
    let page_area = (layout.width_pt * layout.height_pt).max(1.0);
    layout
        .images
        .iter()
        .filter(|image| {
            let area = image.bbox.width() * image.bbox.height();
            image.bbox.width() >= 24.0 && image.bbox.height() >= 24.0 && area / page_area < 0.8
        })
        .enumerate()
        .map(|(index, image)| Region {
            index: index as u32,
            kind: RegionKind::Image,
            bbox: image.bbox.padded(2.0, layout.width_pt, layout.height_pt),
            text: None,
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct TsvWord {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    conf: f32,
    text: String,
}

fn parse_tsv(tsv: &str) -> Vec<(BBox, String)> {
    let mut lines = tsv.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let columns: Vec<&str> = header.split('\t').collect();
    let index = |name: &str| columns.iter().position(|column| *column == name);
    let (
        Some(level),
        Some(left),
        Some(top),
        Some(width),
        Some(height),
        Some(conf),
        Some(text),
        Some(block),
        Some(paragraph),
        Some(line),
    ) = (
        index("level"),
        index("left"),
        index("top"),
        index("width"),
        index("height"),
        index("conf"),
        index("text"),
        index("block_num"),
        index("par_num"),
        index("line_num"),
    )
    else {
        return Vec::new();
    };
    let mut grouped: std::collections::BTreeMap<(u32, u32, u32), Vec<TsvWord>> =
        std::collections::BTreeMap::new();
    for row in lines {
        let fields: Vec<&str> = row.split('\t').collect();
        if fields.len() <= level || fields[level] != "5" {
            continue;
        }
        let float_at = |position: usize| -> Option<f32> {
            fields
                .get(position)
                .and_then(|value| value.parse::<f32>().ok())
        };
        let int_at = |position: usize| -> Option<u32> {
            fields
                .get(position)
                .and_then(|value| value.parse::<u32>().ok())
        };
        let (Some(left), Some(top), Some(width), Some(height), Some(conf)) = (
            float_at(left),
            float_at(top),
            float_at(width),
            float_at(height),
            float_at(conf),
        ) else {
            continue;
        };
        let text = fields.get(text).copied().unwrap_or_default().trim();
        if text.is_empty() {
            continue;
        }
        let (Some(block), Some(paragraph), Some(line)) =
            (int_at(block), int_at(paragraph), int_at(line))
        else {
            continue;
        };
        grouped
            .entry((block, paragraph, line))
            .or_default()
            .push(TsvWord {
                left,
                top,
                width,
                height,
                conf,
                text: text.to_owned(),
            });
    }
    grouped
        .into_values()
        .filter_map(|words| {
            let first = words.first()?;
            let mut bbox = BBox {
                x0: first.left,
                y0: first.top,
                x1: first.left + first.width,
                y1: first.top + first.height,
            };
            let mut text = String::new();
            for word in words {
                // Geometry includes low-confidence fragments (a formula's
                // superscript is often misread), while the human label keeps
                // only words tesseract was confident about.
                bbox.x0 = bbox.x0.min(word.left);
                bbox.y0 = bbox.y0.min(word.top);
                bbox.x1 = bbox.x1.max(word.left + word.width);
                bbox.y1 = bbox.y1.max(word.top + word.height);
                if word.conf < 30.0 {
                    continue;
                }
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&word.text);
            }
            Some((bbox, text))
        })
        .collect()
}

async fn tesseract_regions(
    tesseract: &Path,
    page_png: &Path,
    dpi: u32,
    page_width_pt: f32,
    page_height_pt: f32,
) -> Result<Vec<Region>, CropError> {
    let output = tokio::time::timeout(
        Duration::from_secs(45),
        Command::new(tesseract)
            .arg(page_png)
            .arg("stdout")
            .arg("tsv")
            .env("LC_ALL", "C.UTF-8")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| CropError::Timeout)?
    .map_err(|_| CropError::Tool)?;
    if !output.status.success() {
        return Err(CropError::Tool);
    }
    let tsv = String::from_utf8_lossy(&output.stdout);
    let scale = 72.0 / dpi as f32;
    let mut regions: Vec<Region> = parse_tsv(&tsv)
        .into_iter()
        .filter(|(bbox, _)| bbox.width() >= 8.0 && bbox.height() >= 6.0)
        .enumerate()
        .map(|(index, (bbox, text))| Region {
            index: index as u32,
            kind: RegionKind::OcrLine,
            bbox: BBox {
                x0: (bbox.x0 * scale).max(0.0),
                y0: (bbox.y0 * scale).max(0.0),
                x1: (bbox.x1 * scale).min(page_width_pt),
                y1: (bbox.y1 * scale).min(page_height_pt),
            },
            text: (!text.is_empty()).then_some(text),
        })
        .collect();
    if regions.is_empty() {
        regions.push(Region {
            index: 0,
            kind: RegionKind::Page,
            bbox: BBox {
                x0: 0.0,
                y0: 0.0,
                x1: page_width_pt,
                y1: page_height_pt,
            },
            text: None,
        });
    }
    Ok(regions)
}

pub async fn select_regions(
    tools: &CropTools,
    layout: &PageLayout,
    route: PageRoute,
    page_png: &Path,
    dpi: u32,
) -> Result<Vec<Region>, CropError> {
    let mut regions = match route {
        PageRoute::Ocr => {
            tesseract_regions(
                &tools.tesseract,
                page_png,
                dpi,
                layout.width_pt,
                layout.height_pt,
            )
            .await?
        }
        PageRoute::Digital | PageRoute::Hybrid => {
            let mut regions = grouped_line_regions(layout);
            let images = image_regions(layout);
            for (next, mut image) in (regions.len() as u32..).zip(images) {
                image.index = next;
                regions.push(image);
            }
            regions
        }
        PageRoute::Blank => Vec::new(),
    };
    regions.sort_by(|left, right| {
        left.bbox
            .y0
            .total_cmp(&right.bbox.y0)
            .then(left.bbox.x0.total_cmp(&right.bbox.x0))
    });
    regions.truncate(MAX_CROPS_PER_PAGE);
    for (index, region) in regions.iter_mut().enumerate() {
        region.index = index as u32;
    }
    Ok(regions)
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 30 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    match &bytes[12..16] {
        b"VP8X" => {
            let width = 1
                + u32::from(bytes[24])
                + (u32::from(bytes[25]) << 8)
                + (u32::from(bytes[26]) << 16);
            let height = 1
                + u32::from(bytes[27])
                + (u32::from(bytes[28]) << 8)
                + (u32::from(bytes[29]) << 16);
            Some((width, height))
        }
        b"VP8L" => {
            if bytes[20] != 0x2f {
                return None;
            }
            let bits = u32::from(bytes[21])
                | (u32::from(bytes[22]) << 8)
                | (u32::from(bytes[23]) << 16)
                | (u32::from(bytes[24]) << 24);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        b"VP8 " => {
            if bytes.len() < 30 || &bytes[23..26] != b"\x9d\x01\x2a" {
                return None;
            }
            let width = u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3fff;
            let height = u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3fff;
            Some((u32::from(width), u32::from(height)))
        }
        _ => None,
    }
}

/// Crop one region. The temporary file keeps a `.webp` suffix so ImageMagick
/// selects the encoder, then is atomically renamed and restricted to 0600.
#[allow(clippy::too_many_arguments)]
pub async fn crop_region(
    tools: &CropTools,
    page_png: &Path,
    page_width_pt: f32,
    page_height_pt: f32,
    dpi: u32,
    output_dir: &Path,
    run_id: Uuid,
    page_number: u32,
    region: &Region,
) -> Result<CropOutput, CropError> {
    let scale = dpi as f32 / 72.0;
    let x0 = (region.bbox.x0 * scale).floor().max(0.0) as u32;
    let y0 = (region.bbox.y0 * scale).floor().max(0.0) as u32;
    let x1 = (region.bbox.x1 * scale).ceil().max(0.0) as u32;
    let y1 = (region.bbox.y1 * scale).ceil().max(0.0) as u32;
    let width = x1.saturating_sub(x0).min(MAX_CROP_DIMENSION);
    let height = y1.saturating_sub(y0).min(MAX_CROP_DIMENSION);
    if width < MIN_CROP_DIMENSION || height < MIN_CROP_DIMENSION {
        return Err(CropError::Unsafe);
    }
    tokio::fs::create_dir_all(output_dir)
        .await
        .map_err(|_| CropError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ =
            tokio::fs::set_permissions(output_dir, std::fs::Permissions::from_mode(0o700)).await;
    }
    let mut id_hasher = Sha256::new();
    id_hasher.update(run_id.as_bytes());
    id_hasher.update(page_number.to_le_bytes());
    id_hasher.update(region.index.to_le_bytes());
    id_hasher.update(x0.to_le_bytes());
    id_hasher.update(y0.to_le_bytes());
    id_hasher.update(width.to_le_bytes());
    id_hasher.update(height.to_le_bytes());
    let digest = id_hasher.finalize();
    let mut id_bytes = [0u8; 16];
    id_bytes.copy_from_slice(&digest[..16]);
    let crop_id = Uuid::from_bytes(id_bytes);
    let final_path = output_dir.join(format!("{crop_id}.webp"));
    if !final_path.exists() {
        let temp_path = output_dir.join(format!("{crop_id}.tmp.webp"));
        let crop = format!("{width}x{height}+{x0}+{y0}");
        let output = tokio::time::timeout(
            Duration::from_secs(45),
            Command::new(&tools.magick)
                .arg(page_png)
                .arg("-crop")
                .arg(&crop)
                .arg("+repage")
                .arg("-strip")
                .arg("-quality")
                .arg("82")
                .arg("-define")
                .arg("webp:method=4")
                .arg(&temp_path)
                .env("LC_ALL", "C.UTF-8")
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output(),
        )
        .await
        .map_err(|_| CropError::Timeout)?
        .map_err(|_| CropError::Tool)?;
        if !output.status.success() {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(CropError::Encode);
        }
        let metadata = tokio::fs::metadata(&temp_path)
            .await
            .map_err(|_| CropError::Io)?;
        if metadata.len() == 0 || metadata.len() > MAX_CROP_BYTES {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(CropError::Unsafe);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600))
                .await;
        }
        tokio::fs::rename(&temp_path, &final_path)
            .await
            .map_err(|_| CropError::Io)?;
    }
    let bytes = tokio::fs::read(&final_path)
        .await
        .map_err(|_| CropError::Io)?;
    let (encoded_width, encoded_height) = webp_dimensions(&bytes).ok_or(CropError::InvalidWebp)?;
    if encoded_width > MAX_CROP_DIMENSION || encoded_height > MAX_CROP_DIMENSION {
        return Err(CropError::Unsafe);
    }
    let _ = (page_width_pt, page_height_pt);
    Ok(CropOutput {
        id: crop_id,
        region_index: region.index,
        kind: region.kind,
        bbox: region.bbox,
        pixel_bbox: [x0, y0, width, height],
        width: encoded_width,
        height: encoded_height,
        dpi,
        sha256: hex::encode(Sha256::digest(&bytes)),
        bytes: bytes.len() as u64,
        relative_path: format!("crops/{crop_id}.webp"),
        text: region.text.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tesseract_lines() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t10\t20\t30\t12\t95.0\tHello\n5\t1\t1\t1\t1\t2\t45\t20\t40\t12\t92.0\tworld\n5\t1\t1\t1\t2\t1\t10\t40\t50\t12\t88.0\tSecond\n";
        let lines = parse_tsv(tsv);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].1, "Hello world");
        assert_eq!(lines[0].0.x0, 10.0);
        assert_eq!(lines[1].1, "Second");
    }

    #[test]
    fn rejects_non_webp_bytes_and_accepts_a_minimal_vp8x_header() {
        assert!(webp_dimensions(b"not a webp").is_none());
        let mut bytes = vec![0u8; 40];
        bytes[0..4].copy_from_slice(b"RIFF");
        bytes[4..8].copy_from_slice(&32u32.to_le_bytes());
        bytes[8..12].copy_from_slice(b"WEBP");
        bytes[12..16].copy_from_slice(b"VP8X");
        bytes[24] = 99;
        bytes[27] = 49;
        assert_eq!(webp_dimensions(&bytes), Some((100, 50)));
    }
}
