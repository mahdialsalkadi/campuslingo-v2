//! Barrier-aware diagram presentation consolidation.
//!
//! The frozen §7.1 vector/raster predicates are the acceptance rule. A composite
//! is only produced when its enlarged union still satisfies the predicate that
//! applies to it: vector components recheck segment count, `non_text_ink_ratio`
//! and `path_ink_ratio`; raster components stay inside the bounded figure area.
//! Text extraction actions are never replaced by a crop.
use serde::{Deserialize, Serialize};

use crate::{
    pdf::{BBox, PageLayout, PdfError, pixel_bbox},
    raster::Raster,
    regions::{UNIFORM_VARIANCE, math_or_garbled},
    vectors::{VectorEvidence, close_mask, components, union_area},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BoxClass {
    ProseBlock,
    LabelOrMathAtom,
    OtherText,
    VectorPrimitive,
    RasterImage,
}

/// Non-math text wider than 35 characters and 35% of the page is a merge
/// barrier. Short text, math fonts and math operator density are absorbable
/// labels. Long narrow text is neither a barrier nor a label.
pub fn text_class(text: &str, font: Option<&str>, bbox: BBox, page_width: f32) -> BoxClass {
    let math = math_or_garbled(text, font).is_some();
    let chars = text.chars().count();
    if !math && chars > 35 && bbox.width() > 0.35 * page_width {
        BoxClass::ProseBlock
    } else if chars <= 35 || math {
        BoxClass::LabelOrMathAtom
    } else {
        BoxClass::OtherText
    }
}

fn union(a: BBox, b: BBox) -> BBox {
    BBox {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

fn overlap(a: BBox, b: BBox) -> f64 {
    a.intersection(b).map_or(0.0, |rect| f64::from(rect.area()))
}

fn barrier(bbox: BBox, prose: &[BBox]) -> bool {
    prose
        .iter()
        .any(|block| overlap(bbox, *block) > 0.10 * f64::from(block.area()))
}

fn containment(a: BBox, b: BBox) -> f64 {
    overlap(a, b) / f64::from(a.area().min(b.area())).max(1.0e-12)
}

fn iou(a: BBox, b: BBox) -> f64 {
    let intersection = overlap(a, b);
    intersection / (f64::from(a.area()) + f64::from(b.area()) - intersection).max(1.0e-12)
}

fn near(a: BBox, b: BBox) -> bool {
    let x = (a.x0 - b.x1).max(b.x0 - a.x1).max(0.0);
    let y = (a.y0 - b.y1).max(b.y0 - a.y1).max(0.0);
    (x <= 24.0 && y <= 16.0) || containment(a, b) >= 0.80
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Composite {
    pub bbox: BBox,
    pub has_vector: bool,
    pub has_raster: bool,
    pub segments: u32,
    pub primitive_ids: Vec<u32>,
    pub absorbed_labels: Vec<usize>,
    pub source_text: Option<String>,
}

fn ink_ratio(bbox: BBox, mask: &Raster, layout: &PageLayout) -> Result<f64, PdfError> {
    let [x, y, width, height] = pixel_bbox(bbox, layout.width_pt, layout.height_pt, 150)?;
    if width == 0 || height == 0 {
        return Ok(0.0);
    }
    if x + width > mask.width || y + height > mask.height {
        return Err(PdfError::Layout);
    }
    let mut ink = 0u64;
    for row in y..y + height {
        for column in x..x + width {
            ink += u64::from(mask.gray[(row * mask.width + column) as usize] < 250);
        }
    }
    Ok(ink as f64 / (f64::from(width) * f64::from(height)))
}

fn non_text_ratio(bbox: BBox, layout: &PageLayout) -> f64 {
    let text = layout
        .lines
        .iter()
        .filter_map(|line| line.bbox.intersection(bbox))
        .collect::<Vec<_>>();
    let area = f64::from(bbox.area());
    if area <= 0.0 {
        return 0.0;
    }
    (1.0 - union_area(&text) / area).clamp(0.0, 1.0)
}

fn accepted(
    composite: &Composite,
    layout: &PageLayout,
    page_raster: &Raster,
    vector_mask: &Raster,
) -> Result<bool, PdfError> {
    let page_area = f64::from(layout.width_pt) * f64::from(layout.height_pt);
    let fraction = f64::from(composite.bbox.area()) / page_area;
    if fraction < 0.04 {
        return Ok(false);
    }
    if composite.has_raster {
        if fraction >= 0.85 {
            return Ok(false);
        }
        // A merged raster region must still satisfy the frozen non-uniform
        // predicate; two non-uniform figures can share a uniform gap.
        if page_raster.variance(composite.bbox, (layout.width_pt, layout.height_pt), 150)?
            <= UNIFORM_VARIANCE
        {
            return Ok(false);
        }
    }
    if !composite.has_raster && fraction >= 0.90 {
        return Ok(false);
    }
    if composite.has_vector {
        if composite.segments < 25 {
            return Ok(false);
        }
        if non_text_ratio(composite.bbox, layout) < 0.35 {
            return Ok(false);
        }
        if ink_ratio(composite.bbox, vector_mask, layout)? < 0.05 {
            return Ok(false);
        }
    }
    Ok(true)
}

fn absorb(
    cluster: &mut Composite,
    label: &BBox,
    layout: &PageLayout,
    page_raster: &Raster,
    vector_mask: &Raster,
    prose: &[BBox],
    index: usize,
) -> Result<bool, PdfError> {
    if cluster.absorbed_labels.contains(&index) {
        return Ok(false);
    }
    let mut proposed = cluster.clone();
    proposed.bbox = union(cluster.bbox, *label);
    if barrier(proposed.bbox, prose) || !accepted(&proposed, layout, page_raster, vector_mask)? {
        return Ok(false);
    }
    proposed.absorbed_labels.push(index);
    proposed.absorbed_labels.sort_unstable();
    proposed.absorbed_labels.dedup();
    *cluster = proposed;
    Ok(true)
}

fn merge(a: &Composite, b: &Composite) -> Composite {
    let mut primitive_ids = a.primitive_ids.clone();
    for id in &b.primitive_ids {
        if !primitive_ids.contains(id) {
            primitive_ids.push(*id);
        }
    }
    primitive_ids.sort_unstable();
    let mut absorbed = a.absorbed_labels.clone();
    absorbed.extend(b.absorbed_labels.iter().copied());
    absorbed.sort_unstable();
    absorbed.dedup();
    Composite {
        bbox: union(a.bbox, b.bbox),
        has_vector: a.has_vector || b.has_vector,
        has_raster: a.has_raster || b.has_raster,
        segments: a.segments.saturating_add(b.segments),
        primitive_ids,
        absorbed_labels: absorbed,
        source_text: None,
    }
}

pub fn consolidate(
    layout: &PageLayout,
    vector: &VectorEvidence,
    vector_mask: &Raster,
    page_raster: &Raster,
    raster_boxes: &[BBox],
) -> Result<Vec<Composite>, PdfError> {
    let prose = layout
        .lines
        .iter()
        .filter(|line| {
            text_class(
                &line.text,
                line.font_family.as_deref(),
                line.bbox,
                layout.width_pt,
            ) == BoxClass::ProseBlock
        })
        .map(|line| line.bbox)
        .collect::<Vec<_>>();
    let mut clusters = vector
        .components
        .iter()
        .filter(|component| component.eligible)
        .map(|component| Composite {
            bbox: component.bbox,
            has_vector: true,
            has_raster: false,
            segments: component.segments,
            primitive_ids: component.primitive_ids.clone(),
            absorbed_labels: Vec::new(),
            source_text: None,
        })
        .collect::<Vec<_>>();
    for bbox in raster_boxes {
        if bbox.is_valid() {
            clusters.push(Composite {
                bbox: *bbox,
                has_vector: false,
                has_raster: true,
                segments: 0,
                primitive_ids: Vec::new(),
                absorbed_labels: Vec::new(),
                source_text: None,
            });
        }
    }
    if clusters.len() > 512 {
        return Err(PdfError::UnsafeRender);
    }

    // Stage 1: barrier-aware anisotropic merge.
    loop {
        let mut merged = false;
        'pairs: for i in 0..clusters.len() {
            for j in i + 1..clusters.len() {
                if !near(clusters[i].bbox, clusters[j].bbox) {
                    continue;
                }
                let candidate = merge(&clusters[i], &clusters[j]);
                if barrier(candidate.bbox, &prose)
                    || !accepted(&candidate, layout, page_raster, vector_mask)?
                {
                    continue;
                }
                clusters[i] = candidate;
                clusters.remove(j);
                merged = true;
                break 'pairs;
            }
        }
        if !merged {
            break;
        }
    }

    // Stage 2: two-hop 14 pt halo absorption of labels and math atoms.
    for _ in 0..2 {
        for cluster in &mut clusters {
            if cluster.bbox.width() < 20.0 || cluster.bbox.height() < 20.0 {
                continue;
            }
            let halo = cluster.bbox.padded(14.0, layout.width_pt, layout.height_pt);
            for (index, line) in layout.lines.iter().enumerate() {
                if !line.bbox.is_valid()
                    || text_class(
                        &line.text,
                        line.font_family.as_deref(),
                        line.bbox,
                        layout.width_pt,
                    ) != BoxClass::LabelOrMathAtom
                    || overlap(halo, line.bbox) < 0.30 * f64::from(line.bbox.area())
                {
                    continue;
                }
                absorb(
                    cluster,
                    &line.bbox,
                    layout,
                    page_raster,
                    vector_mask,
                    &prose,
                    index,
                )?;
            }
        }
    }

    // Stage 3: NMS of overlapping clusters, still barred by prose.
    loop {
        let mut merged = false;
        'pairs: for i in 0..clusters.len() {
            for j in i + 1..clusters.len() {
                if iou(clusters[i].bbox, clusters[j].bbox) < 0.15
                    && containment(clusters[i].bbox, clusters[j].bbox) < 0.40
                {
                    continue;
                }
                let candidate = merge(&clusters[i], &clusters[j]);
                if barrier(candidate.bbox, &prose)
                    || !accepted(&candidate, layout, page_raster, vector_mask)?
                {
                    continue;
                }
                clusters[i] = candidate;
                clusters.remove(j);
                merged = true;
                break 'pairs;
            }
        }
        if !merged {
            break;
        }
    }

    let page_area = f64::from(layout.width_pt) * f64::from(layout.height_pt);
    clusters.retain(|cluster| {
        !(cluster.bbox.width() < 36.0
            && cluster.bbox.height() < 24.0
            && f64::from(cluster.bbox.area()) < 0.008 * page_area)
    });

    // Stage 4: 6 pt padding only when it stays inside the page and outside any
    // prose barrier. The frozen predicates are rechecked after padding.
    for cluster in &mut clusters {
        let padded = cluster.bbox.padded(6.0, layout.width_pt, layout.height_pt);
        let mut proposed = cluster.clone();
        proposed.bbox = padded;
        if !barrier(padded, &prose) && accepted(&proposed, layout, page_raster, vector_mask)? {
            *cluster = proposed;
        }
    }

    // The crop is a presentation composite; every absorbed label remains in the
    // digital-text stream as well. No label is removed from source text.
    for cluster in &mut clusters {
        let mut labels = cluster.absorbed_labels.clone();
        for (index, line) in layout.lines.iter().enumerate() {
            if line.bbox.is_valid()
                && cluster.bbox.intersection(line.bbox) == Some(line.bbox)
                && !labels.contains(&index)
            {
                labels.push(index);
            }
        }
        labels.sort_unstable();
        labels.dedup();
        let text = labels
            .iter()
            .filter_map(|index| layout.lines.get(*index))
            .map(|line| line.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            cluster.source_text = Some(text);
        }
    }
    clusters.sort_by(|a, b| {
        a.bbox
            .y0
            .total_cmp(&b.bbox.y0)
            .then(a.bbox.x0.total_cmp(&b.bbox.x0))
            .then(a.bbox.y1.total_cmp(&b.bbox.y1))
            .then(a.bbox.x1.total_cmp(&b.bbox.x1))
    });
    Ok(clusters)
}

/// Contrast-based layout sub-region isolation for full-page scans. Text boxes
/// from the provisional geometry pass cannot become diagram crops: a component
/// must retain at least 35% non-text area after the union of text boxes is
/// subtracted. The frozen 4%..85% and variance bounds are applied here.
pub fn scan_regions(
    raster: &Raster,
    layout: &PageLayout,
    text_boxes: &[BBox],
) -> Result<Vec<BBox>, PdfError> {
    let width = raster.width as usize;
    let height = raster.height as usize;
    if width == 0 || height == 0 || raster.gray.len() != width * height {
        return Err(PdfError::Layout);
    }
    let mut histogram = [0usize; 256];
    for y in 0..height {
        for x in 0..width {
            if x < 3 || y < 3 || x + 3 >= width || y + 3 >= height {
                histogram[raster.gray[y * width + x] as usize] += 1;
            }
        }
    }
    let background = histogram
        .iter()
        .enumerate()
        .max_by_key(|(_, count)| *count)
        .map(|(value, _)| value as i16)
        .ok_or(PdfError::Layout)?;
    let ink = raster
        .gray
        .iter()
        .map(|value| u8::from((i16::from(*value) - background).abs() > 16))
        .collect::<Vec<_>>();
    let closed = close_mask(
        &ink,
        width,
        height,
        ((width as f64) * 0.015).round().max(1.0) as usize,
    )?;
    let (_, boxes) = components(&closed, width, height);
    if boxes.len() > 2_048 {
        return Err(PdfError::UnsafeRender);
    }
    let page_area = f64::from(layout.width_pt) * f64::from(layout.height_pt);
    let mut result = Vec::new();
    for [x0, y0, x1, y1] in boxes {
        let bbox = BBox {
            x0: (x0 as f32 * 72.0 / 150.0).min(layout.width_pt),
            y0: (y0 as f32 * 72.0 / 150.0).min(layout.height_pt),
            x1: (x1 as f32 * 72.0 / 150.0).min(layout.width_pt),
            y1: (y1 as f32 * 72.0 / 150.0).min(layout.height_pt),
        };
        if !bbox.is_valid() {
            continue;
        }
        let ratio = f64::from(bbox.area()) / page_area;
        if !(0.04..0.85).contains(&ratio) {
            continue;
        }
        if non_text_ratio(bbox, layout) < 0.35 {
            continue;
        }
        let text_overlap = text_boxes
            .iter()
            .filter_map(|text| text.intersection(bbox))
            .collect::<Vec<_>>();
        let text_fraction = union_area(&text_overlap) / f64::from(bbox.area());
        if text_fraction > 0.65 {
            continue;
        }
        if raster.variance(bbox, (layout.width_pt, layout.height_pt), 150)? > UNIFORM_VARIANCE {
            result.push(bbox);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::{LayoutLine, PageLayout};

    fn layout() -> PageLayout {
        PageLayout {
            page_number: 1,
            width_pt: 100.0,
            height_pt: 100.0,
            lines: Vec::new(),
            images: Vec::new(),
            text: String::new(),
            geometry: None,
            geometry_warnings: Vec::new(),
        }
    }

    #[test]
    fn prose_and_math_classification_is_not_char_count_only() {
        let bbox = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 80.0,
            y1: 10.0,
        };
        let text = "a long prose sentence that forms a merge barrier";
        assert_eq!(text_class(text, None, bbox, 100.0), BoxClass::ProseBlock);
        assert_eq!(
            text_class(text, Some("CMMI"), bbox, 100.0),
            BoxClass::LabelOrMathAtom
        );
        assert!(barrier(
            BBox {
                x0: 0.0,
                y0: 0.0,
                x1: 20.0,
                y1: 10.0
            },
            &[bbox]
        ));
    }

    #[test]
    fn nms_and_containment_cannot_cross_prose() {
        let a = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 40.0,
            y1: 40.0,
        };
        let b = BBox {
            x0: 35.0,
            y0: 0.0,
            x1: 80.0,
            y1: 40.0,
        };
        let prose = BBox {
            x0: 42.0,
            y0: 5.0,
            x1: 75.0,
            y1: 35.0,
        };
        assert!(near(a, b));
        assert!(barrier(union(a, b), &[prose]));
    }

    #[test]
    fn long_narrow_text_is_neither_barrier_nor_label() {
        let bbox = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 20.0,
            y1: 40.0,
        };
        assert_eq!(
            text_class(
                "a long sentence without enough width to be prose",
                None,
                bbox,
                100.0
            ),
            BoxClass::OtherText
        );
    }

    #[test]
    fn absorbed_labels_keep_source_text_out_of_the_mask() {
        let mut page = layout();
        page.lines.push(LayoutLine {
            text: "x = 1".into(),
            bbox: BBox {
                x0: 10.0,
                y0: 10.0,
                x1: 30.0,
                y1: 16.0,
            },
            font_family: None,
        });
        let cluster = Composite {
            bbox: BBox {
                x0: 5.0,
                y0: 5.0,
                x1: 60.0,
                y1: 60.0,
            },
            has_vector: true,
            has_raster: false,
            segments: 30,
            primitive_ids: vec![0],
            absorbed_labels: vec![0],
            source_text: None,
        };
        assert!(
            non_text_ratio(cluster.bbox, &page) >= 0.35,
            "a small label does not erase the non-text predicate"
        );
    }

    fn page_144() -> PageLayout {
        PageLayout {
            page_number: 1,
            width_pt: 144.0,
            height_pt: 144.0,
            lines: Vec::new(),
            images: Vec::new(),
            text: String::new(),
            geometry: None,
            geometry_warnings: Vec::new(),
        }
    }

    fn white_raster() -> Raster {
        Raster {
            width: 300,
            height: 300,
            gray: vec![255; 300 * 300],
        }
    }

    fn vector_component(bbox: BBox, segments: u32) -> crate::vectors::VectorComponent {
        crate::vectors::VectorComponent {
            bbox,
            segments,
            primitive_ids: vec![0],
            non_text_ink_ratio: 1.0,
            path_ink_ratio: 1.0,
            eligible: true,
        }
    }

    fn evidence(components: Vec<crate::vectors::VectorComponent>) -> VectorEvidence {
        VectorEvidence {
            schema_version: 1,
            primitive_count: components.len() as u32,
            excluded_backgrounds: 0,
            excluded_text_glyphs: 0,
            excluded_templates: 0,
            template_status: crate::vectors::TemplateStatus::Complete,
            mask_width: 300,
            mask_height: 300,
            components,
            incomplete: Vec::new(),
        }
    }

    #[test]
    fn merged_composites_recheck_non_text_and_segment_predicates() {
        let page = page_144();
        let page_raster = white_raster();
        let mut mask_gray = vec![255; 300 * 300];
        for y in 16..88 {
            for x in 16..146 {
                mask_gray[y * 300 + x] = 0;
            }
        }
        let vector_mask = Raster {
            width: 300,
            height: 300,
            gray: mask_gray,
        };
        let components = vec![
            vector_component(
                BBox {
                    x0: 10.0,
                    y0: 10.0,
                    x1: 40.0,
                    y1: 40.0,
                },
                30,
            ),
            vector_component(
                BBox {
                    x0: 42.0,
                    y0: 10.0,
                    x1: 70.0,
                    y1: 40.0,
                },
                30,
            ),
        ];
        let merged = consolidate(
            &page,
            &evidence(components.clone()),
            &vector_mask,
            &page_raster,
            &[],
        )
        .unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].segments, 60);

        let mut covered = page.clone();
        covered.lines.push(LayoutLine {
            text: "a long prose sentence that covers the whole proposed composite".into(),
            bbox: BBox {
                x0: 8.0,
                y0: 8.0,
                x1: 72.0,
                y1: 42.0,
            },
            font_family: None,
        });
        let kept = consolidate(
            &covered,
            &evidence(components),
            &vector_mask,
            &page_raster,
            &[],
        )
        .unwrap();
        assert_eq!(
            kept.len(),
            2,
            "text coverage rejects the enlarged composite"
        );
    }

    #[test]
    fn raster_predicate_rechecks_variance_and_area_bounds() {
        let page = page_144();
        let mut gray = vec![255; 300 * 300];
        for y in 90..160 {
            for x in 90..160 {
                gray[y * 300 + x] = if (x / 3 + y / 3) % 2 == 0 { 0 } else { 255 };
            }
        }
        let page_raster = Raster {
            width: 300,
            height: 300,
            gray,
        };
        let vector_mask = white_raster();
        let mut composite = Composite {
            bbox: BBox {
                x0: 40.0,
                y0: 40.0,
                x1: 80.0,
                y1: 80.0,
            },
            has_vector: false,
            has_raster: true,
            segments: 0,
            primitive_ids: Vec::new(),
            absorbed_labels: Vec::new(),
            source_text: None,
        };
        assert!(accepted(&composite, &page, &page_raster, &vector_mask).unwrap());
        composite.bbox = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 144.0,
            y1: 144.0,
        };
        assert!(
            !accepted(&composite, &page, &page_raster, &vector_mask).unwrap(),
            "a merged raster cannot cross the 85% full-page bound"
        );
        composite.bbox = BBox {
            x0: 20.0,
            y0: 20.0,
            x1: 60.0,
            y1: 60.0,
        };
        assert!(
            !accepted(&composite, &page, &white_raster(), &vector_mask).unwrap(),
            "a uniform merged region fails the non-uniform raster predicate"
        );
    }

    #[test]
    fn scan_regions_isolate_bounded_non_text_components() {
        let mut page = page_144();
        let mut gray = vec![255; 300 * 300];
        for y in 62..187 {
            for x in 62..187 {
                gray[y * 300 + x] = if (x / 3 + y / 3) % 2 == 0 { 0 } else { 255 };
            }
        }
        let raster = Raster {
            width: 300,
            height: 300,
            gray,
        };
        let regions = scan_regions(&raster, &page, &[]).unwrap();
        assert_eq!(regions.len(), 1);
        let ratio = f64::from(regions[0].area()) / (144.0 * 144.0);
        assert!((0.04..0.85).contains(&ratio));

        page.lines.push(LayoutLine {
            text: "text covering most of the candidate region".into(),
            bbox: BBox {
                x0: 25.0,
                y0: 25.0,
                x1: 89.0,
                y1: 89.0,
            },
            font_family: None,
        });
        let text_boxes = page.lines.iter().map(|line| line.bbox).collect::<Vec<_>>();
        assert!(
            scan_regions(&raster, &page, &text_boxes)
                .unwrap()
                .is_empty(),
            "a text-dominated component is not a diagram crop"
        );
    }
}
