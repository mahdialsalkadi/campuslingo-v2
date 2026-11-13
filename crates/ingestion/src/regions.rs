//! Independent region actions. PageRoute is only a summary, never a switch
//! deciding which kinds of evidence a hybrid page is allowed to contain.
use crate::{
    pdf::{BBox, PageLayout, PageRoute, PdfError},
    raster::Raster,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const ANALYSIS_DPI: u32 = 150;
pub const CROP_DPI: u32 = 250;
pub const UNIFORM_VARIANCE: f64 = 0.00001;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionKind {
    #[serde(rename = "ACTION_CROP_VECTOR_CLUSTERS")]
    CropVectorClusters,
    #[serde(rename = "ACTION_OCR_FULL_PAGE_SCAN")]
    OcrFullPageScan,
    #[serde(rename = "ACTION_CROP_RASTER_FIGURES")]
    CropRasterFigures,
    #[serde(rename = "ACTION_OCR_MATH_OR_GARBLED_REGIONS")]
    OcrMathOrGarbledRegions,
    #[serde(rename = "ACTION_EXTRACT_DIGITAL_TEXT")]
    ExtractDigitalText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionAction {
    pub kind: ActionKind,
    pub bbox: BBox,
    pub source_text: Option<String>,
    pub ocr_required: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageActions {
    pub schema_version: u32,
    pub actions: Vec<RegionAction>,
    pub excluded_uniform_images: u32,
    pub excluded_template_images: u32,
    /// Missing analysis is explicit, never silently treated as zero vectors.
    pub incomplete: Vec<String>,
}

impl PageActions {
    pub fn route(&self, non_uniform: bool) -> PageRoute {
        let digital = self
            .actions
            .iter()
            .any(|a| a.kind == ActionKind::ExtractDigitalText);
        let ocr = self.actions.iter().any(|a| a.ocr_required);
        match (digital, ocr) {
            (true, true) => PageRoute::Hybrid,
            (true, false) => PageRoute::Digital,
            (false, true) => PageRoute::Ocr,
            (false, false) if non_uniform => PageRoute::Hybrid,
            _ => PageRoute::Blank,
        }
    }
}

#[derive(Default)]
pub struct TemplateImages {
    keys: HashSet<String>,
}

fn image_key(layout: &PageLayout, bbox: BBox, identity: &str) -> String {
    let ratios = [
        bbox.x0 / layout.width_pt,
        bbox.y0 / layout.height_pt,
        bbox.x1 / layout.width_pt,
        bbox.y1 / layout.height_pt,
    ];
    format!(
        "{}:{:?}",
        identity,
        ratios.map(|v| (f64::from(v) * 10000.0).round_ties_even() as i64)
    )
}

impl TemplateImages {
    pub fn detect(layouts: &[PageLayout]) -> Self {
        if layouts.len() < 5 {
            return Self::default();
        }
        let mut seen: HashMap<String, HashSet<u32>> = HashMap::new();
        for layout in layouts {
            for image in &layout.images {
                if image.source_sha256.is_empty() {
                    continue;
                }
                seen.entry(image_key(layout, image.bbox, &image.source_sha256))
                    .or_default()
                    .insert(layout.page_number);
            }
        }
        let keys = seen
            .into_iter()
            .filter(|(_, pages)| pages.len() >= 3 && pages.len() * 100 >= layouts.len() * 60)
            .map(|(key, _)| key)
            .collect();
        Self { keys }
    }
}

pub fn math_or_garbled(text: &str, font: Option<&str>) -> Option<&'static str> {
    if let Some(font) = font {
        let font = font.to_ascii_lowercase();
        if [
            "cmmi",
            "cmsy",
            "cmex",
            "msam",
            "msbm",
            "symbol",
            "mt extra",
            "euclid",
            "latinmodernmath",
            "mathjax",
        ]
        .iter()
        .any(|name| font.contains(name))
        {
            return Some("math_font");
        }
    }
    let n = text.chars().filter(|c| !c.is_whitespace()).count();
    if n == 0 {
        return None;
    }
    let garbage = text
        .chars()
        .filter(|c| *c == '\u{fffd}' || c.is_control() && !['\n', '\r', '\t'].contains(c))
        .count();
    if garbage as f64 / n as f64 > 0.08 {
        return Some("garbled_glyph_ratio");
    }
    let operators = text
        .chars()
        .filter(|c| {
            matches!(
                *c,
                '=' | '≠'
                    | '≤'
                    | '≥'
                    | '¬'
                    | '∧'
                    | '∨'
                    | '→'
                    | '↔'
                    | '∀'
                    | '∃'
                    | '∑'
                    | '∫'
                    | '∈'
                    | '∉'
                    | '⊂'
                    | '⊆'
                    | '∪'
                    | '∩'
                    | '√'
                    | '±'
                    | '×'
                    | '÷'
                    | '∞'
            )
        })
        .count();
    if operators as f64 / n as f64 >= 0.12 {
        Some("unicode_operator_density")
    } else {
        None
    }
}

pub fn plan_page(
    layout: &PageLayout,
    raster: &Raster,
    templates: &TemplateImages,
) -> Result<PageActions, PdfError> {
    plan_page_with_vectors(layout, raster, templates, None)
}

pub fn plan_page_with_vectors(
    layout: &PageLayout,
    raster: &Raster,
    templates: &TemplateImages,
    vector: Option<&crate::vectors::VectorEvidence>,
) -> Result<PageActions, PdfError> {
    let size = (layout.width_pt, layout.height_pt);
    let mut plan = PageActions {
        schema_version: 1,
        actions: Vec::new(),
        excluded_uniform_images: 0,
        excluded_template_images: 0,
        incomplete: vector
            .map(|evidence| evidence.incomplete.clone())
            .unwrap_or_else(|| {
                vec![
                    "vector_mask_and_clusters_pending".into(),
                    "raster_60_percent_fallback_requires_vector_analysis".into(),
                ]
            }),
    };
    if vector.is_some_and(|evidence| {
        evidence.template_status == crate::vectors::TemplateStatus::Incomplete
    }) {
        plan.incomplete
            .push("vector_template_deck_incomplete".into());
    }
    plan.incomplete
        .extend(layout.geometry_warnings.iter().cloned());
    plan.incomplete.sort_unstable();
    plan.incomplete.dedup();
    let mut scan = false;
    let digital_chars = layout.text.chars().filter(|c| !c.is_whitespace()).count();
    let mut raster_boxes = Vec::new();
    for image in &layout.images {
        let fraction = image.bbox.area() / (size.0 * size.1);
        if !image.bbox.is_valid() {
            return Err(PdfError::Layout);
        }
        if raster.variance(image.bbox, size, ANALYSIS_DPI)? <= UNIFORM_VARIANCE {
            plan.excluded_uniform_images += 1;
            continue;
        }
        if templates
            .keys
            .contains(&image_key(layout, image.bbox, &image.source_sha256))
        {
            plan.excluded_template_images += 1;
            continue;
        }
        // Every non-template image contributes to the raster coverage ratio,
        // including a full-page scan handled by ACTION_OCR_FULL_PAGE_SCAN.
        raster_boxes.push(image.bbox);
        if fraction >= 0.85 {
            if !scan {
                plan.actions.push(RegionAction {
                    kind: ActionKind::OcrFullPageScan,
                    bbox: BBox {
                        x0: 0.0,
                        y0: 0.0,
                        x1: size.0,
                        y1: size.1,
                    },
                    source_text: None,
                    ocr_required: true,
                    reason: "single_raster_covers_85_percent".into(),
                });
                scan = true;
            }
        } else if fraction >= 0.04 {
            plan.actions.push(RegionAction {
                kind: ActionKind::CropRasterFigures,
                bbox: image.bbox,
                source_text: None,
                ocr_required: digital_chars == 0,
                reason: if digital_chars == 0 {
                    "image_region_without_digital_text"
                } else {
                    "bounded_non_uniform_raster"
                }
                .into(),
            });
        }
    }
    // Frozen §7.1 fallback: raster_ratio >= 0.60 AND digital_chars < 80 AND zero
    // vector candidate_cluster. Missing vector analysis cannot authorize it.
    if !scan
        && digital_chars < 80
        && vector.is_some_and(|evidence| {
            evidence.incomplete.is_empty()
                && !evidence
                    .components
                    .iter()
                    .any(|component| component.eligible)
        })
        && crate::vectors::union_area(&raster_boxes) / f64::from(size.0 * size.1) >= 0.60
    {
        plan.actions.push(RegionAction {
            kind: ActionKind::OcrFullPageScan,
            bbox: BBox {
                x0: 0.0,
                y0: 0.0,
                x1: size.0,
                y1: size.1,
            },
            source_text: None,
            ocr_required: true,
            reason: "raster_60_percent_and_no_vector_candidate".into(),
        });
    }
    if let Some(vector) = vector {
        for component in vector
            .components
            .iter()
            .filter(|component| component.eligible)
        {
            plan.actions.push(RegionAction {
                kind: ActionKind::CropVectorClusters,
                bbox: component.bbox,
                source_text: None,
                ocr_required: false,
                reason: "frozen_vector_mask_predicates".into(),
            });
        }
    }
    for line in &layout.lines {
        if !line.bbox.is_valid() {
            continue;
        }
        let math = math_or_garbled(&line.text, line.font_family.as_deref());
        plan.actions.push(RegionAction {
            kind: if math.is_some() {
                ActionKind::OcrMathOrGarbledRegions
            } else {
                ActionKind::ExtractDigitalText
            },
            bbox: line.bbox,
            source_text: Some(line.text.clone()),
            ocr_required: math.is_some(),
            reason: math.unwrap_or("clean_digital_text").into(),
        });
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::{LayoutImage, LayoutLine};
    fn layout() -> PageLayout {
        PageLayout {
            page_number: 1,
            width_pt: 100.0,
            height_pt: 100.0,
            lines: vec![],
            images: vec![],
            text: String::new(),
            geometry: None,
            geometry_warnings: vec![],
        }
    }
    fn raster() -> Raster {
        Raster {
            width: 209,
            height: 209,
            gray: (0..209 * 209)
                .map(|i| if i % 2 == 0 { 0 } else { 255 })
                .collect(),
        }
    }
    #[test]
    fn font_first_math_protects_ascii_and_garbage_threshold_is_strict() {
        assert_eq!(
            math_or_garbled("abc", Some("ABCDEF+CMMI10")),
            Some("math_font")
        );
        assert_eq!(math_or_garbled("abc", Some("MT Extra")), Some("math_font"));
        assert_eq!(
            math_or_garbled("¬p ∨ q", None),
            Some("unicode_operator_density")
        );
        assert_eq!(
            math_or_garbled("abcdefghijk\u{fffd}", None),
            Some("garbled_glyph_ratio")
        );
        assert_eq!(
            math_or_garbled(&format!("{}\u{fffd}\u{fffd}", "a".repeat(23)), None),
            None
        );
        assert_eq!(math_or_garbled("ordinary title", None), None);
    }
    #[test]
    fn sparse_digital_cover_never_becomes_full_page_scan() {
        let mut l = layout();
        l.text = "Short title".into();
        l.lines.push(LayoutLine {
            text: l.text.clone(),
            bbox: BBox {
                x0: 2.0,
                y0: 2.0,
                x1: 50.0,
                y1: 10.0,
            },
            font_family: None,
        });
        let plan = plan_page(&l, &raster(), &TemplateImages::default()).unwrap();
        assert_eq!(plan.route(true), PageRoute::Digital);
    }

    fn vector_evidence(eligible: bool) -> crate::vectors::VectorEvidence {
        crate::vectors::VectorEvidence {
            schema_version: 1,
            primitive_count: u32::from(eligible),
            excluded_backgrounds: 0,
            excluded_text_glyphs: 0,
            excluded_templates: 0,
            template_status: crate::vectors::TemplateStatus::Complete,
            mask_width: 209,
            mask_height: 209,
            components: eligible
                .then(|| crate::vectors::VectorComponent {
                    bbox: BBox {
                        x0: 10.0,
                        y0: 10.0,
                        x1: 60.0,
                        y1: 40.0,
                    },
                    segments: 40,
                    primitive_ids: vec![0],
                    non_text_ink_ratio: 0.8,
                    path_ink_ratio: 0.2,
                    eligible: true,
                })
                .into_iter()
                .collect(),
            incomplete: Vec::new(),
        }
    }

    #[test]
    fn raster_60_percent_fallback_requires_complete_vector_analysis_and_no_candidate() {
        let mut l = layout();
        l.images.push(LayoutImage {
            bbox: BBox {
                x0: 0.0,
                y0: 0.0,
                x1: 75.0,
                y1: 100.0,
            },
            source_sha256: String::new(),
        });
        // No vector evidence: the fallback stays unavailable and explicit.
        let pending = plan_page(&l, &raster(), &TemplateImages::default()).unwrap();
        assert!(
            !pending
                .actions
                .iter()
                .any(|action| action.kind == ActionKind::OcrFullPageScan)
        );
        assert!(
            pending
                .incomplete
                .iter()
                .any(|code| code == "raster_60_percent_fallback_requires_vector_analysis")
        );

        // Complete analysis with no eligible vector cluster authorizes scan.
        let empty = vector_evidence(false);
        let scan = plan_page_with_vectors(&l, &raster(), &TemplateImages::default(), Some(&empty))
            .unwrap();
        assert!(
            scan.actions
                .iter()
                .any(|action| action.kind == ActionKind::OcrFullPageScan)
        );

        // A real vector candidate blocks the raster-only fallback.
        let candidate = vector_evidence(true);
        let hybrid =
            plan_page_with_vectors(&l, &raster(), &TemplateImages::default(), Some(&candidate))
                .unwrap();
        assert!(
            !hybrid
                .actions
                .iter()
                .any(|action| action.kind == ActionKind::OcrFullPageScan)
        );
        assert!(
            hybrid
                .actions
                .iter()
                .any(|action| action.kind == ActionKind::CropVectorClusters)
        );
    }
    #[test]
    fn simultaneous_math_text_and_raster_actions_and_exact_area_bounds() {
        let mut l = layout();
        l.text = "clean text".into();
        l.lines = vec![
            LayoutLine {
                text: "clean text".into(),
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 30.0,
                    y1: 5.0,
                },
                font_family: None,
            },
            LayoutLine {
                text: "a".into(),
                bbox: BBox {
                    x0: 0.0,
                    y0: 10.0,
                    x1: 3.0,
                    y1: 15.0,
                },
                font_family: Some("Symbol".into()),
            },
        ];
        for (x1, y1) in [(20.0, 20.0), (10.0, 39.0), (100.0, 85.0)] {
            l.images.push(LayoutImage {
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1,
                    y1,
                },
                source_sha256: String::new(),
            });
        }
        let p = plan_page(&l, &raster(), &TemplateImages::default()).unwrap();
        assert_eq!(
            p.actions
                .iter()
                .filter(|a| a.kind == ActionKind::CropRasterFigures)
                .count(),
            1
        );
        assert_eq!(
            p.actions
                .iter()
                .filter(|a| a.kind == ActionKind::OcrFullPageScan)
                .count(),
            1
        );
        assert!(
            p.actions
                .iter()
                .any(|a| a.kind == ActionKind::ExtractDigitalText)
        );
        assert!(
            p.actions
                .iter()
                .any(|a| a.kind == ActionKind::OcrMathOrGarbledRegions)
        );
        assert_eq!(p.route(true), PageRoute::Hybrid);
    }
    #[test]
    fn recurrence_counts_distinct_pages_and_requires_both_deck_thresholds() {
        let mut l = layout();
        l.images = vec![
            LayoutImage {
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 25.0,
                    y1: 25.0
                },
                source_sha256: "same".into()
            };
            10
        ];
        assert!(TemplateImages::detect(&vec![l.clone(); 4]).keys.is_empty());
        let mut deck = vec![l.clone(); 5];
        for (i, page) in deck.iter_mut().enumerate() {
            page.page_number = i as u32 + 1;
        }
        deck[3].images.clear();
        deck[4].images.clear();
        assert_eq!(TemplateImages::detect(&deck).keys.len(), 1);
        deck[2].images.clear();
        assert!(TemplateImages::detect(&deck).keys.is_empty());
    }
    #[test]
    fn uniform_full_page_background_is_excluded_even_in_a_single_page() {
        let mut l = layout();
        l.images.push(LayoutImage {
            bbox: BBox {
                x0: 0.0,
                y0: 0.0,
                x1: 100.0,
                y1: 100.0,
            },
            source_sha256: "background".into(),
        });
        let mut r = raster();
        r.gray.fill(255);
        let plan = plan_page(&l, &r, &TemplateImages::default()).unwrap();
        assert!(plan.actions.is_empty());
        assert_eq!(plan.excluded_uniform_images, 1);
    }
}
