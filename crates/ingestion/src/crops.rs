//! Region selection and private WebP crop generation.
//!
//! Digital text boxes come from pdftohtml. Image-only pages use tesseract TSV
//! only for line geometry; the authoritative OCR text still comes from the
//! resident PaddleOCR-VL model. Each crop records both PDF-point and rendered
//! pixel coordinates, plus a digest and byte size.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::process::Command;
use uuid::Uuid;

use crate::pdf::{BBox, PageLayout, PdfTools};
use crate::process::{HelperError, HelperLimits, run_helper};

pub const MAX_CROPS_PER_PAGE: usize = 60;
// A 1440pt lecture slide is 5000px wide at mandatory 250 DPI. Bound both
// dimensions and total pixels, rather than silently clipping such source crops.
pub const MAX_CROP_DIMENSION: u32 = 8192;
pub const MIN_CROP_DIMENSION: u32 = 8;
pub const MAX_CROP_BYTES: u64 = 8 * 1024 * 1024;
const MAGICK_POLICY_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/policy");
const MAGICK_POLICY: &str = include_str!("../policy/policy.xml");

#[derive(Debug, Error)]
pub enum CropError {
    #[error(transparent)]
    Pdf(#[from] crate::pdf::PdfError),
    #[error("the source region is below the minimum crop dimensions")]
    TooSmall,
    #[error("the source region exceeds the maximum crop dimensions")]
    TooLarge,
    #[error("the source render dimensions differ from the requested pixel rectangle")]
    SourceRasterMismatch,
    #[error(transparent)]
    Helper(#[from] HelperError),
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
    Vector,
    Text,
    Image,
    OcrLine,
    Page,
    Math,
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

impl CropTools {
    fn magick_command(&self) -> Result<Command, CropError> {
        // A relocated binary must install the policy too, never silently fall
        // back to a host's potentially permissive ImageMagick configuration.
        if std::fs::read_to_string(Path::new(MAGICK_POLICY_DIR).join("policy.xml"))
            .map_err(|_| CropError::Unsafe)?
            != MAGICK_POLICY
        {
            return Err(CropError::Unsafe);
        }
        let mut command = Command::new(&self.magick);
        command
            .env("MAGICK_CONFIGURE_PATH", MAGICK_POLICY_DIR)
            .env("MAGICK_THREAD_LIMIT", "2");
        Ok(command)
    }
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

pub(crate) async fn tesseract_regions(
    tesseract: &Path,
    page_png: &Path,
    dpi: u32,
    page_width_pt: f32,
    page_height_pt: f32,
) -> Result<Vec<Region>, CropError> {
    let output = run_helper(
        Command::new(tesseract)
            .arg(page_png)
            .arg("stdout")
            .arg("tsv")
            .env("LC_ALL", "C.UTF-8"),
        HelperLimits::pdf(Duration::from_secs(45), crate::pdf::MAX_XML_BYTES),
    )
    .await?;
    if !output.status.success() {
        return Err(CropError::Tool);
    }
    let tsv = String::from_utf8_lossy(&output.stdout);
    let scale = 72.0 / dpi as f32;
    let regions: Vec<Region> = parse_tsv(&tsv)
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
    Ok(regions)
}

/// Render the selected source rectangle at 250 DPI and encode without resizing.
/// Input SHA, original rectangle and region index are part of the identity, so
/// retrying a run with changed PDF bytes cannot reuse a stale crop.
#[allow(clippy::too_many_arguments)]
pub async fn crop_pdf_region(
    tools: &CropTools,
    source: &Path,
    layout: &PageLayout,
    run_dir: &Path,
    run_id: Uuid,
    input_sha256: &str,
    region: &Region,
) -> Result<(CropOutput, PathBuf), CropError> {
    let pixels = crate::pdf::pixel_bbox(region.bbox, layout.width_pt, layout.height_pt, 250)
        .map_err(|_| CropError::Unsafe)?;
    if pixels[2] > MAX_CROP_DIMENSION || pixels[3] > MAX_CROP_DIMENSION {
        return Err(CropError::TooLarge);
    }
    if pixels[2] < MIN_CROP_DIMENSION || pixels[3] < MIN_CROP_DIMENSION {
        return Err(CropError::TooSmall);
    }
    let mut hash = Sha256::new();
    hash.update(b"source-crop-v1");
    hash.update(run_id.as_bytes());
    hash.update(input_sha256.as_bytes());
    hash.update(layout.page_number.to_le_bytes());
    hash.update(region.index.to_le_bytes());
    for pixel in pixels {
        hash.update(pixel.to_le_bytes())
    }
    let digest = hash.finalize();
    let mut id = [0; 16];
    id.copy_from_slice(&digest[..16]);
    let id = Uuid::from_bytes(id);
    let png = tools
        .pdf
        .render_region_png(
            source,
            layout.page_number,
            region.bbox,
            (layout.width_pt, layout.height_pt),
            &run_dir.join("pages").join(format!("region-{id}")),
        )
        .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&png, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|_| CropError::Io)?;
    }
    let decoded = crate::raster::Raster::load(&png).map_err(|_| CropError::Unsafe)?;
    if decoded.width != pixels[2] || decoded.height != pixels[3] {
        return Err(CropError::SourceRasterMismatch);
    }
    let final_path = run_dir.join("crops").join(format!("{id}.webp"));
    let temporary = final_path.with_extension("tmp.webp");
    let result = run_helper(
        tools
            .magick_command()?
            .arg(&png)
            .args(["-strip", "-quality", "82", "-define", "webp:method=4"])
            .arg(&temporary),
        HelperLimits::pdf(Duration::from_secs(45), 64 * 1024),
    )
    .await;
    let output = match result {
        Ok(output) => output,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(error.into());
        }
    };
    if !output.status.success() {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(CropError::Encode);
    }
    let metadata = tokio::fs::metadata(&temporary)
        .await
        .map_err(|_| CropError::Io)?;
    if metadata.len() == 0 || metadata.len() > MAX_CROP_BYTES {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(CropError::Unsafe);
    }
    let bytes = tokio::fs::read(&temporary)
        .await
        .map_err(|_| CropError::Io)?;
    if webp_dimensions(&bytes) != Some((pixels[2], pixels[3])) {
        return Err(CropError::InvalidWebp);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|_| CropError::Io)?;
    }
    tokio::fs::rename(&temporary, &final_path)
        .await
        .map_err(|_| CropError::Io)?;
    Ok((
        CropOutput {
            id,
            region_index: region.index,
            kind: region.kind,
            bbox: region.bbox,
            pixel_bbox: pixels,
            width: pixels[2],
            height: pixels[3],
            dpi: 250,
            sha256: hex::encode(Sha256::digest(&bytes)),
            bytes: bytes.len() as u64,
            relative_path: format!("crops/{id}.webp"),
            text: region.text.clone(),
        },
        png,
    ))
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
#[cfg(test)]
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
        let output = run_helper(
            tools
                .magick_command()?
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
                .env("LC_ALL", "C.UTF-8"),
            HelperLimits::pdf(Duration::from_secs(45), 64 * 1024),
        )
        .await?;
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

    #[tokio::test]
    #[ignore = "requires local Poppler/ImageMagick and delegated helper cgroup"]
    async fn raster_crop_policy_allows_png_to_webp_and_denies_other_coders() {
        let directory = tempfile::tempdir().unwrap();
        let tools = CropTools::default();
        let source = crate::project_root().join("tests/fixtures/m2/annotated-english.pdf");
        let png = tools
            .pdf
            .render_png(&source, 1, 150, &directory.path().join("page"))
            .await
            .unwrap();
        let geometry = tesseract_regions(&tools.tesseract, &png, 150, 612.0, 792.0)
            .await
            .unwrap();
        assert!(
            geometry
                .iter()
                .any(|region| region.kind == RegionKind::OcrLine)
        );
        let probe = run_helper(
            tools
                .magick_command()
                .unwrap()
                .arg(&png)
                .arg("-crop")
                .arg("376x168+41+41")
                .arg("+repage")
                .arg(directory.path().join("probe.webp")),
            HelperLimits::pdf(Duration::from_secs(5), 64 * 1024),
        )
        .await
        .unwrap();
        assert!(
            probe.status.success(),
            "synthetic PNG probe: {}",
            String::from_utf8_lossy(&probe.stderr)
        );
        let output = crop_region(
            &tools,
            &png,
            612.0,
            792.0,
            150,
            directory.path(),
            Uuid::new_v4(),
            1,
            &Region {
                index: 0,
                kind: RegionKind::Text,
                bbox: BBox {
                    x0: 20.0,
                    y0: 20.0,
                    x1: 200.0,
                    y1: 100.0,
                },
                text: None,
            },
        )
        .await
        .unwrap();
        assert!(output.bytes > 0);
        assert_eq!(output.width, 376);
        let output = run_helper(
            tools
                .magick_command()
                .unwrap()
                .arg("label:forbidden")
                .arg(directory.path().join("denied.webp")),
            HelperLimits::pdf(Duration::from_secs(5), 64 * 1024),
        )
        .await
        .unwrap();
        assert!(!output.status.success());
    }
}
