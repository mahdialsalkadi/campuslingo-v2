//! Poppler/Cairo vector evidence. Glyph definitions and raster XObjects never
//! enter `M_vec`: text is extracted separately and raster figures are cropped by
//! their own action. Only a sanitized, local SVG subset reaches rsvg-convert.
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::Path,
    time::Duration,
};

use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use svgtypes::{SimplePathSegment as Seg, Transform, TransformListToken as T};
use tokio::{io::AsyncReadExt, process::Command};

use crate::{
    pdf::{BBox, MAX_RENDER_PIXELS, PageLayout, PdfError, pixel_bbox},
    process::{HelperLimits, run_helper},
    raster::Raster,
    regions::UNIFORM_VARIANCE,
};

const MAX_SVG_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PRIMITIVES: usize = 20_000;
const MAX_SEGMENTS: usize = 200_000;
const MAX_COMPONENTS: usize = 2_048;
const MAX_TRACE_SAMPLES: usize = 4_000_000;
const DPI: f64 = 150.0 / 72.0;

#[derive(Debug, Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone)]
struct Segment {
    points: Vec<Point>,
}

impl Segment {
    fn point(&self, t: f64) -> Point {
        let mut points = self.points.clone();
        for n in (1..points.len()).rev() {
            for i in 0..n {
                points[i] = Point {
                    x: points[i].x * (1.0 - t) + points[i + 1].x * t,
                    y: points[i].y * (1.0 - t) + points[i + 1].y * t,
                };
            }
        }
        points[0]
    }

    fn steps(&self) -> usize {
        self.points
            .windows(2)
            .map(|p| (p[1].x - p[0].x).hypot(p[1].y - p[0].y))
            .sum::<f64>()
            .mul_add(DPI * 2.0, 1.0)
            .ceil()
            .clamp(1.0, 65_536.0) as usize
    }
}

#[derive(Debug, Clone)]
pub struct Primitive {
    pub bbox: BBox,
    pub key: String,
    svg: String,
    segments: Vec<Segment>,
    segment_count: usize,
    axis_rectangle: bool,
    solid_fill: bool,
    stroked: bool,
}

#[derive(Debug, Clone)]
pub struct VectorPage {
    pub page_number: u32,
    pub width: f32,
    pub height: f32,
    pub primitives: Vec<Primitive>,
    defs: String,
    pub incomplete: Vec<String>,
}

impl VectorPage {
    pub fn estimated_bytes(&self) -> usize {
        self.defs.len()
            + self
                .primitives
                .iter()
                .map(|p| {
                    p.svg.len()
                        + p.segments
                            .iter()
                            .map(|s| s.points.len() * std::mem::size_of::<Point>())
                            .sum::<usize>()
                })
                .sum::<usize>()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateStatus {
    #[default]
    NotApplicable,
    Complete,
    Incomplete,
}

#[derive(Default)]
pub struct VectorTemplates {
    keys: HashSet<String>,
    pub status: TemplateStatus,
}

/// Streaming full-deck recurrence counter. It never retains page geometry, so a
/// large deck cannot make template detection unbounded in memory.
#[derive(Default)]
pub struct TemplateCounter {
    seen: HashMap<String, HashSet<u32>>,
    pages: u32,
    failed: u32,
}

impl TemplateCounter {
    pub fn observe(&mut self, page: &VectorPage) {
        let keys = page
            .primitives
            .iter()
            .map(|primitive| primitive.key.clone())
            .collect::<HashSet<_>>();
        self.observe_keys(page.page_number, keys);
    }

    pub fn observe_keys(&mut self, page_number: u32, keys: HashSet<String>) {
        self.pages += 1;
        for key in keys {
            self.seen.entry(key).or_default().insert(page_number);
        }
    }

    pub fn fail(&mut self) {
        self.pages += 1;
        self.failed += 1;
    }

    pub fn finish(self) -> VectorTemplates {
        // Frozen §7.1: multi-page template exclusion applies only when the deck
        // has at least five pages. A partial pass cannot authorize exclusion.
        if self.pages < 5 {
            return VectorTemplates {
                keys: HashSet::new(),
                status: TemplateStatus::NotApplicable,
            };
        }
        if self.failed > 0 {
            return VectorTemplates {
                keys: HashSet::new(),
                status: TemplateStatus::Incomplete,
            };
        }
        let total_pages = self.pages as usize;
        let keys = self
            .seen
            .into_iter()
            .filter(|(_, pages)| pages.len() >= 3 && pages.len() * 100 >= total_pages * 60)
            .map(|(key, _)| key)
            .collect();
        VectorTemplates {
            keys,
            status: TemplateStatus::Complete,
        }
    }
}

impl VectorTemplates {
    pub fn detect(pages: &[VectorPage]) -> Self {
        let mut counter = TemplateCounter::default();
        for page in pages {
            counter.observe(page);
        }
        counter.finish()
    }

    pub fn excluded(&self, key: &str) -> bool {
        self.keys.contains(key)
    }
}

fn multiply(a: Transform, b: Transform) -> Transform {
    Transform::new(
        a.a * b.a + a.c * b.b,
        a.b * b.a + a.d * b.b,
        a.a * b.c + a.c * b.d,
        a.b * b.c + a.d * b.d,
        a.a * b.e + a.c * b.f + a.e,
        a.b * b.e + a.d * b.f + a.f,
    )
}

fn transform(text: &str) -> Result<Transform, PdfError> {
    let mut matrix = Transform::default();
    for token in svgtypes::TransformListParser::from(text) {
        let next = match token.map_err(|_| PdfError::Layout)? {
            T::Matrix { a, b, c, d, e, f } => Transform::new(a, b, c, d, e, f),
            T::Translate { tx, ty } => Transform::new(1.0, 0.0, 0.0, 1.0, tx, ty),
            T::Scale { sx, sy } => Transform::new(sx, 0.0, 0.0, sy, 0.0, 0.0),
            T::Rotate { angle } => {
                let angle = angle.to_radians();
                Transform::new(
                    angle.cos(),
                    angle.sin(),
                    -angle.sin(),
                    angle.cos(),
                    0.0,
                    0.0,
                )
            }
            T::SkewX { angle } => Transform::new(1.0, 0.0, angle.to_radians().tan(), 1.0, 0.0, 0.0),
            T::SkewY { angle } => Transform::new(1.0, angle.to_radians().tan(), 0.0, 1.0, 0.0, 0.0),
        };
        matrix = multiply(matrix, next);
    }
    if ![matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f]
        .iter()
        .all(|value| value.is_finite())
    {
        return Err(PdfError::Layout);
    }
    Ok(matrix)
}

fn point(matrix: Transform, x: f64, y: f64) -> Point {
    Point {
        x: matrix.a * x + matrix.c * y + matrix.e,
        y: matrix.b * x + matrix.d * y + matrix.f,
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn attrs(node: Node<'_, '_>, include_id: bool) -> Result<String, PdfError> {
    let mut values = BTreeMap::new();
    for attr in node.attributes() {
        let name = attr.name();
        let value = attr.value();
        if name.starts_with("on")
            || ["href", "filter", "mask"].contains(&name)
            || (name == "id" && !include_id)
        {
            if name == "id" {
                continue;
            }
            return Err(PdfError::Layout);
        }
        if name == "style" {
            for pair in value.split(';').filter(|pair| !pair.trim().is_empty()) {
                let (key, value) = pair.split_once(':').ok_or(PdfError::Layout)?;
                let key = key.trim();
                if !allowed_attr(key) {
                    return Err(PdfError::Layout);
                }
                validate_value(key, value.trim())?;
                values.insert(key.to_owned(), paint(key, value.trim()));
            }
        } else if allowed_attr(name) {
            validate_value(name, value)?;
            values.insert(name.to_owned(), paint(name, value));
        }
    }
    Ok(values
        .into_iter()
        .map(|(key, value)| format!(" {key}=\"{}\"", escape(&value)))
        .collect())
}

fn allowed_attr(name: &str) -> bool {
    matches!(
        name,
        "id" | "d"
            | "transform"
            | "x"
            | "y"
            | "width"
            | "height"
            | "points"
            | "x1"
            | "y1"
            | "x2"
            | "y2"
            | "rx"
            | "ry"
            | "cx"
            | "cy"
            | "r"
            | "fill"
            | "fill-rule"
            | "fill-opacity"
            | "stroke"
            | "stroke-width"
            | "stroke-opacity"
            | "stroke-linecap"
            | "stroke-linejoin"
            | "stroke-miterlimit"
            | "stroke-dasharray"
            | "stroke-dashoffset"
            | "opacity"
            | "clip-path"
            | "clip-rule"
            | "clipPathUnits"
    )
}

fn validate_value(name: &str, value: &str) -> Result<(), PdfError> {
    if value.len() > 8 * 1024 * 1024 || value.contains(['<', '>']) {
        return Err(PdfError::Layout);
    }
    if value.contains("url(") {
        let local_clip = name == "clip-path"
            && value.starts_with("url(#")
            && value.ends_with(')')
            && !value[5..value.len() - 1].contains(['(', ')', '/']);
        if !local_clip {
            return Err(PdfError::Layout);
        }
    }
    Ok(())
}

fn paint(name: &str, value: &str) -> String {
    if ["fill", "stroke"].contains(&name) && value != "none" {
        "black".into()
    } else {
        value.into()
    }
}

fn number(node: Node<'_, '_>, key: &str, default: f64) -> Result<f64, PdfError> {
    let value = node
        .attribute(key)
        .map(|value| value.parse::<f64>())
        .transpose()
        .map_err(|_| PdfError::Layout)?
        .unwrap_or(default);
    if !value.is_finite() {
        return Err(PdfError::Layout);
    }
    Ok(value)
}

fn inherited_value(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.ancestors()
        .filter(Node::is_element)
        .find_map(|ancestor| ancestor.attribute(name))
        .map(str::to_owned)
}

fn inherited_number(node: Node<'_, '_>, name: &str, default: f64) -> Result<f64, PdfError> {
    match inherited_value(node, name) {
        Some(value) => {
            let value = value.parse::<f64>().map_err(|_| PdfError::Layout)?;
            value.is_finite().then_some(value).ok_or(PdfError::Layout)
        }
        None => Ok(default),
    }
}

fn shape_path(node: Node<'_, '_>) -> Result<String, PdfError> {
    Ok(match node.tag_name().name() {
        "path" => node.attribute("d").unwrap_or("").into(),
        "rect" => {
            let x = number(node, "x", 0.0)?;
            let y = number(node, "y", 0.0)?;
            let width = number(node, "width", 0.0)?;
            let height = number(node, "height", 0.0)?;
            if node.has_attribute("rx") || node.has_attribute("ry") {
                return Err(PdfError::Layout);
            }
            format!(
                "M{x} {y}L{} {y}L{} {}L{x} {}Z",
                x + width,
                x + width,
                y + height,
                y + height
            )
        }
        "line" => format!(
            "M{} {}L{} {}",
            number(node, "x1", 0.0)?,
            number(node, "y1", 0.0)?,
            number(node, "x2", 0.0)?,
            number(node, "y2", 0.0)?
        ),
        "polygon" | "polyline" => {
            let raw = node.attribute("points").ok_or(PdfError::Layout)?;
            let values = raw
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|value| !value.is_empty())
                .map(|value| value.parse::<f64>().map_err(|_| PdfError::Layout))
                .collect::<Result<Vec<_>, _>>()?;
            if values.len() < 4 || values.len() % 2 != 0 || values.len() > 20_000 {
                return Err(PdfError::Layout);
            }
            let mut path = format!("M{} {}", values[0], values[1]);
            for pair in values[2..].chunks_exact(2) {
                path.push_str(&format!("L{} {}", pair[0], pair[1]));
            }
            if node.tag_name().name() == "polygon" {
                path.push('Z');
            }
            path
        }
        "circle" => {
            let cx = number(node, "cx", 0.0)?;
            let cy = number(node, "cy", 0.0)?;
            let r = number(node, "r", 0.0)?;
            format!(
                "M{} {cy}A{r} {r} 0 1 0 {} {cy}A{r} {r} 0 1 0 {} {cy}Z",
                cx + r,
                cx - r,
                cx + r
            )
        }
        "ellipse" => {
            let cx = number(node, "cx", 0.0)?;
            let cy = number(node, "cy", 0.0)?;
            let rx = number(node, "rx", 0.0)?;
            let ry = number(node, "ry", 0.0)?;
            format!(
                "M{} {cy}A{rx} {ry} 0 1 0 {} {cy}A{rx} {ry} 0 1 0 {} {cy}Z",
                cx + rx,
                cx - rx,
                cx + rx
            )
        }
        _ => return Err(PdfError::Layout),
    })
}

fn segments(path: &str, matrix: Transform) -> Result<Vec<Segment>, PdfError> {
    let mut result = Vec::new();
    let mut current = point(matrix, 0.0, 0.0);
    let mut start = current;
    for segment in svgtypes::SimplifyingPathParser::from(path) {
        let points = match segment.map_err(|_| PdfError::Layout)? {
            Seg::MoveTo { x, y } => {
                current = point(matrix, x, y);
                start = current;
                continue;
            }
            Seg::LineTo { x, y } => vec![current, point(matrix, x, y)],
            Seg::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => vec![
                current,
                point(matrix, x1, y1),
                point(matrix, x2, y2),
                point(matrix, x, y),
            ],
            Seg::Quadratic { x1, y1, x, y } => {
                vec![current, point(matrix, x1, y1), point(matrix, x, y)]
            }
            Seg::ClosePath => {
                let close = vec![current, start];
                current = start;
                result.push(Segment { points: close });
                if result.len() > MAX_SEGMENTS {
                    return Err(PdfError::Layout);
                }
                continue;
            }
        };
        if points.iter().any(|point| {
            !point.x.is_finite()
                || !point.y.is_finite()
                || point.x.abs() > 1e7
                || point.y.abs() > 1e7
        }) {
            return Err(PdfError::Layout);
        }
        current = *points.last().ok_or(PdfError::Layout)?;
        result.push(Segment { points });
        if result.len() > MAX_SEGMENTS {
            return Err(PdfError::Layout);
        }
    }
    Ok(result)
}

fn safe_clip(node: Node<'_, '_>, depth: usize) -> Result<String, PdfError> {
    if depth > 32 || !matches!(node.tag_name().name(), "clipPath" | "g" | "path" | "rect") {
        return Err(PdfError::Layout);
    }
    let name = node.tag_name().name();
    let mut svg = format!("<{name}{}>", attrs(node, true)?);
    for child in node.children().filter(Node::is_element) {
        svg.push_str(&safe_clip(child, depth + 1)?);
    }
    svg.push_str(&format!("</{name}>"));
    Ok(svg)
}

fn hash_geometry(hash: &mut Sha256, segments: &[Segment], width: f32, height: f32) {
    for segment in segments {
        hash.update([segment.points.len() as u8]);
        for point in &segment.points {
            hash.update(
                (point.x / f64::from(width) * 10_000.0)
                    .round_ties_even()
                    .to_le_bytes(),
            );
            hash.update(
                (point.y / f64::from(height) * 10_000.0)
                    .round_ties_even()
                    .to_le_bytes(),
            );
        }
    }
}

fn hash_paint(hash: &mut Sha256, node: Node<'_, '_>, ids: &HashMap<&str, Node<'_, '_>>) {
    for ancestor in node.ancestors().filter(Node::is_element) {
        for attr in ancestor.attributes() {
            if matches!(
                attr.name(),
                "fill"
                    | "stroke"
                    | "fill-opacity"
                    | "stroke-opacity"
                    | "opacity"
                    | "stroke-width"
                    | "stroke-linecap"
                    | "stroke-linejoin"
                    | "stroke-miterlimit"
                    | "stroke-dasharray"
                    | "stroke-dashoffset"
                    | "fill-rule"
                    | "clip-rule"
            ) {
                hash.update(attr.name().as_bytes());
                hash.update(attr.value().as_bytes());
            }
            if attr.name() == "clip-path"
                && let Some(id) = attr
                    .value()
                    .strip_prefix("url(#")
                    .and_then(|value| value.strip_suffix(')'))
                && let Some(clip) = ids.get(id)
            {
                for child in clip.descendants().filter(Node::is_element) {
                    if let Some(path) = child.attribute("d") {
                        hash.update(path.as_bytes());
                    }
                    if let Some(transform) = child.attribute("transform") {
                        hash.update(transform.as_bytes());
                    }
                }
            }
        }
    }
}

fn href_id<'a, 'input>(node: Node<'a, 'input>) -> Option<&'a str> {
    node.attributes()
        .find(|attr| attr.name() == "href")
        .map(|attr| attr.value())
        .and_then(|value| value.strip_prefix('#'))
}

fn is_raster(node: Node<'_, '_>) -> bool {
    node.tag_name().name() == "image"
}

fn inline_svg(node: Node<'_, '_>, depth: usize) -> Result<String, PdfError> {
    if depth > 32 {
        return Err(PdfError::Layout);
    }
    let name = node.tag_name().name();
    Ok(match name {
        "path" | "rect" | "line" | "circle" | "ellipse" | "polygon" | "polyline" => {
            format!("<{name}{}/>", attrs(node, false)?)
        }
        "g" => {
            let mut svg = format!("<g{}>", attrs(node, false)?);
            for child in node.children().filter(Node::is_element) {
                if name_is_skippable(child) {
                    continue;
                }
                svg.push_str(&inline_svg(child, depth + 1)?);
            }
            svg.push_str("</g>");
            svg
        }
        _ => return Err(PdfError::Layout),
    })
}

fn name_is_skippable(node: Node<'_, '_>) -> bool {
    [
        "defs", "clipPath", "image", "symbol", "title", "desc", "filter", "mask",
    ]
    .contains(&node.tag_name().name())
}

struct Walk<'doc, 'input> {
    ids: HashMap<&'doc str, Node<'doc, 'input>>,
    visits: usize,
    segment_total: usize,
}

impl<'doc, 'input> Walk<'doc, 'input> {
    fn vector_content(&self, node: Node<'doc, 'input>) -> bool {
        node.descendants()
            .filter(Node::is_element)
            .any(|child| match child.tag_name().name() {
                "path" | "rect" | "line" | "circle" | "ellipse" | "polygon" | "polyline" => true,
                "use" => href_id(child).is_some_and(|id| {
                    !id.starts_with("glyph")
                        && self
                            .ids
                            .get(id)
                            .is_some_and(|referenced| !is_raster(*referenced))
                }),
                _ => false,
            })
    }
}

fn collect_geometry(
    node: Node<'_, '_>,
    matrix: Transform,
    depth: usize,
    walk: &mut Walk<'_, '_>,
    output: &mut Vec<Segment>,
    incomplete: &mut Vec<String>,
) -> Result<(), PdfError> {
    if depth > 32 {
        return Err(PdfError::Layout);
    }
    let name = node.tag_name().name();
    if matches!(
        name,
        "path" | "rect" | "line" | "circle" | "ellipse" | "polygon" | "polyline"
    ) {
        let local = transform(node.attribute("transform").unwrap_or(""))?;
        output.extend(segments(&shape_path(node)?, multiply(matrix, local))?);
        return Ok(());
    }
    if name == "g" {
        let local = transform(node.attribute("transform").unwrap_or(""))?;
        let matrix = multiply(matrix, local);
        for child in node.children().filter(Node::is_element) {
            if name_is_skippable(child) {
                continue;
            }
            collect_geometry(child, matrix, depth + 1, walk, output, incomplete)?;
        }
        return Ok(());
    }
    if name == "use" {
        if let Some(id) = href_id(node)
            && !id.starts_with("glyph")
            && let Some(referenced) = walk.ids.get(id).copied()
            && !is_raster(referenced)
        {
            let tx = number(node, "x", 0.0)?;
            let ty = number(node, "y", 0.0)?;
            let local = transform(node.attribute("transform").unwrap_or(""))?;
            let use_matrix = multiply(
                matrix,
                multiply(local, Transform::new(1.0, 0.0, 0.0, 1.0, tx, ty)),
            );
            collect_geometry(referenced, use_matrix, depth + 1, walk, output, incomplete)?;
            return Ok(());
        }
        return Ok(());
    }
    incomplete.push(format!("unsupported_vector_use_content_{name}"));
    Ok(())
}

pub fn parse_svg(
    svg: &str,
    expected: Option<(f32, f32)>,
    page_number: u32,
) -> Result<VectorPage, PdfError> {
    if svg.len() > MAX_SVG_BYTES as usize {
        return Err(PdfError::Layout);
    }
    let doc = Document::parse(svg).map_err(|_| PdfError::Layout)?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(PdfError::Layout);
    }
    let view = root
        .attribute("viewBox")
        .ok_or(PdfError::Layout)?
        .split_whitespace()
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PdfError::Layout)?;
    if view.len() != 4 || view[0] != 0.0 || view[1] != 0.0 || !view.iter().all(|v| v.is_finite()) {
        return Err(PdfError::Layout);
    }
    if let Some((width, height)) = expected
        && ((view[2] - width).abs() > 0.05 || (view[3] - height).abs() > 0.05)
    {
        return Err(PdfError::Layout);
    }
    let width = view[2];
    let height = view[3];
    if width <= 0.0 || height <= 0.0 {
        return Err(PdfError::Layout);
    }

    let mut ids = HashMap::new();
    let mut defs = String::new();
    for node in root.descendants().filter(Node::is_element) {
        if let Some(id) = node.attribute("id")
            && ids.insert(id, node).is_some()
        {
            return Err(PdfError::Layout);
        }
        if node.tag_name().name() == "clipPath" {
            defs.push_str(&safe_clip(node, 0)?);
        }
        if matches!(node.tag_name().name(), "script" | "foreignObject") {
            return Err(PdfError::Layout);
        }
    }
    if defs.len() > 8 * 1024 * 1024 {
        return Err(PdfError::Layout);
    }

    let mut page = VectorPage {
        page_number,
        width,
        height,
        primitives: Vec::new(),
        defs,
        incomplete: Vec::new(),
    };
    let mut walk = Walk {
        ids,
        visits: 0,
        segment_total: 0,
    };

    fn visit(
        node: Node<'_, '_>,
        matrix: Transform,
        groups: &str,
        depth: usize,
        walk: &mut Walk<'_, '_>,
        page: &mut VectorPage,
    ) -> Result<(), PdfError> {
        if depth > 32 || walk.visits > 100_000 {
            return Err(PdfError::Layout);
        }
        walk.visits += 1;
        let name = node.tag_name().name();
        if name_is_skippable(node) {
            return Ok(());
        }
        let local = transform(node.attribute("transform").unwrap_or(""))?;
        let matrix = multiply(matrix, local);

        if name == "use" {
            let Some(id) = href_id(node) else {
                page.incomplete.push("vector_use_without_local_href".into());
                return Ok(());
            };
            if id.starts_with("glyph") {
                return Ok(());
            }
            let Some(referenced) = walk.ids.get(id).copied() else {
                page.incomplete.push("vector_use_unresolved".into());
                return Ok(());
            };
            if is_raster(referenced) {
                return Ok(());
            }
            if node.attribute("filter").is_some() || node.attribute("mask").is_some() {
                if walk.vector_content(referenced) {
                    page.incomplete.push("filtered_vector_use".into());
                }
                return Ok(());
            }
            let tx = number(node, "x", 0.0)?;
            let ty = number(node, "y", 0.0)?;
            let use_local = multiply(local, Transform::new(1.0, 0.0, 0.0, 1.0, tx, ty));
            let use_matrix = multiply(matrix, Transform::new(1.0, 0.0, 0.0, 1.0, tx, ty));
            let mut data = Vec::new();
            let mut nested = Vec::new();
            collect_geometry(
                referenced,
                use_matrix,
                depth + 1,
                walk,
                &mut data,
                &mut nested,
            )?;
            page.incomplete.extend(nested);
            if data.is_empty() {
                return Ok(());
            }
            walk.segment_total += data.len();
            if walk.segment_total > MAX_SEGMENTS || page.primitives.len() >= MAX_PRIMITIVES {
                return Err(PdfError::Layout);
            }
            let Some(bbox) = stroke_bbox(&data, node, matrix, page)? else {
                return Ok(());
            };
            let mut hash = Sha256::new();
            hash_geometry(&mut hash, &data, page.width, page.height);
            hash_paint(&mut hash, node, &walk.ids);
            for child in referenced.descendants().filter(Node::is_element) {
                for attr in child.attributes() {
                    if matches!(
                        attr.name(),
                        "fill" | "stroke" | "fill-rule" | "stroke-width"
                    ) {
                        hash.update(attr.name().as_bytes());
                        hash.update(attr.value().as_bytes());
                    }
                }
            }
            let key = hex::encode(hash.finalize());
            let inline = match inline_svg(referenced, depth + 1) {
                Ok(inline) => inline,
                Err(_) => {
                    page.incomplete.push("unsupported_vector_use_inline".into());
                    return Ok(());
                }
            };
            let mut encoded = groups.to_owned();
            encoded.push_str(&format!(
                "<g transform=\"matrix({},{},{},{},{},{})\">{inline}</g>",
                use_local.a, use_local.b, use_local.c, use_local.d, use_local.e, use_local.f
            ));
            for _ in 0..depth {
                encoded.push_str("</g>");
            }
            page.primitives.push(Primitive {
                bbox,
                key,
                svg: encoded,
                segment_count: data.len(),
                segments: data,
                axis_rectangle: false,
                solid_fill: inherited_value(node, "fill").as_deref() != Some("none"),
                stroked: inherited_value(node, "stroke").as_deref() != Some("none"),
            });
            return Ok(());
        }

        if name == "svg" || name == "g" {
            let open = if name == "g" {
                match attrs(node, false) {
                    Ok(value) => format!("{groups}<g{value}>"),
                    Err(_) => {
                        if walk.vector_content(node) {
                            page.incomplete.push("unsupported_vector_group".into());
                        }
                        return Ok(());
                    }
                }
            } else {
                groups.into()
            };
            let next_depth = depth + usize::from(name == "g");
            for child in node.children().filter(Node::is_element) {
                visit(child, matrix, &open, next_depth, walk, page)?;
            }
            return Ok(());
        }

        if !matches!(
            name,
            "path" | "rect" | "line" | "circle" | "ellipse" | "polygon" | "polyline"
        ) {
            page.incomplete
                .push(format!("unsupported_vector_element_{name}"));
            return Ok(());
        }
        let path = shape_path(node)?;
        if path.is_empty() {
            return Ok(());
        }
        let data = segments(&path, matrix)?;
        if data.is_empty() {
            return Ok(());
        }
        walk.segment_total += data.len();
        if walk.segment_total > MAX_SEGMENTS || page.primitives.len() >= MAX_PRIMITIVES {
            return Err(PdfError::Layout);
        }
        let Some(bbox) = stroke_bbox(&data, node, matrix, page)? else {
            return Ok(());
        };
        let encoded_attrs = match attrs(node, false) {
            Ok(value) => value,
            Err(_) => {
                page.incomplete.push("unsupported_vector_paint".into());
                return Ok(());
            }
        };
        let mut encoded = groups.to_owned();
        encoded.push_str(&format!("<{name}{encoded_attrs}/>"));
        for _ in 0..depth {
            encoded.push_str("</g>");
        }
        let mut hash = Sha256::new();
        hash_geometry(&mut hash, &data, page.width, page.height);
        hash_paint(&mut hash, node, &walk.ids);
        let key = hex::encode(hash.finalize());
        let solid_fill = {
            let fill = inherited_value(node, "fill").unwrap_or_else(|| "black".into());
            inherited_number(node, "fill-opacity", 1.0)? > 0.0
                && inherited_number(node, "opacity", 1.0)? > 0.0
                && fill != "none"
        };
        let stroke_value = inherited_value(node, "stroke").unwrap_or_else(|| "none".into());
        let stroke_width = inherited_number(node, "stroke-width", 1.0)?;
        let stroked = stroke_value != "none"
            && stroke_width > 0.0
            && inherited_number(node, "stroke-opacity", 1.0)? > 0.0
            && inherited_number(node, "opacity", 1.0)? > 0.0;
        if !solid_fill && !stroked {
            return Ok(());
        }
        let axis_rectangle = data.len() == 4
            && data.iter().all(|segment| segment.points.len() == 2)
            && data.iter().all(|segment| {
                let a = segment.points[0];
                let b = segment.points[1];
                (a.x - b.x).abs() < 0.01 || (a.y - b.y).abs() < 0.01
            });
        page.primitives.push(Primitive {
            bbox,
            key,
            svg: encoded,
            segment_count: data.len(),
            segments: data,
            axis_rectangle,
            solid_fill,
            stroked,
        });
        Ok(())
    }

    visit(root, Transform::default(), "", 0, &mut walk, &mut page)?;
    page.incomplete.sort_unstable();
    page.incomplete.dedup();
    Ok(page)
}

fn stroke_bbox(
    data: &[Segment],
    node: Node<'_, '_>,
    matrix: Transform,
    page: &VectorPage,
) -> Result<Option<BBox>, PdfError> {
    let all = data
        .iter()
        .flat_map(|segment| segment.points.iter())
        .collect::<Vec<_>>();
    if all.is_empty() {
        return Ok(None);
    }
    let mut bbox = BBox {
        x0: all.iter().map(|p| p.x as f32).fold(f32::INFINITY, f32::min),
        y0: all.iter().map(|p| p.y as f32).fold(f32::INFINITY, f32::min),
        x1: all
            .iter()
            .map(|p| p.x as f32)
            .fold(f32::NEG_INFINITY, f32::max),
        y1: all
            .iter()
            .map(|p| p.y as f32)
            .fold(f32::NEG_INFINITY, f32::max),
    };
    if !bbox.is_valid() {
        return Ok(None);
    }
    if inherited_value(node, "stroke").as_deref() != Some("none") {
        let scale = matrix.a.hypot(matrix.b).max(matrix.c.hypot(matrix.d)) as f32;
        let stroke = inherited_number(node, "stroke-width", 1.0)? as f32 * scale;
        if stroke.is_finite() && stroke > 0.0 {
            bbox = BBox {
                x0: bbox.x0 - stroke / 2.0,
                y0: bbox.y0 - stroke / 2.0,
                x1: bbox.x1 + stroke / 2.0,
                y1: bbox.y1 + stroke / 2.0,
            };
        }
    }
    Ok(bbox.intersection(BBox {
        x0: 0.0,
        y0: 0.0,
        x1: page.width,
        y1: page.height,
    }))
}

pub async fn extract_svg(source: &Path, page: u32, output: &Path) -> Result<(), PdfError> {
    let page_text = page.to_string();
    let output_status = run_helper(
        Command::new("pdftocairo")
            .args(["-svg", "-origpagesizes", "-f", &page_text, "-l", &page_text])
            .arg(source)
            .arg(output),
        HelperLimits::pdf(Duration::from_secs(45), 64 * 1024),
    )
    .await?;
    if !output_status.status.success() {
        return Err(PdfError::Tool);
    }
    let metadata = tokio::fs::metadata(output)
        .await
        .map_err(|_| PdfError::Io)?;
    if metadata.len() == 0 || metadata.len() > MAX_SVG_BYTES {
        return Err(PdfError::Layout);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(output, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|_| PdfError::Io)?;
    }
    Ok(())
}

pub async fn load_svg(path: &Path, layout: &PageLayout) -> Result<VectorPage, PdfError> {
    let mut input = tokio::fs::File::open(path)
        .await
        .map_err(|_| PdfError::Io)?;
    if input.metadata().await.map_err(|_| PdfError::Io)?.len() > MAX_SVG_BYTES {
        return Err(PdfError::Layout);
    }
    let mut svg = String::new();
    input
        .read_to_string(&mut svg)
        .await
        .map_err(|_| PdfError::Layout)?;
    parse_svg(
        &svg,
        Some((layout.width_pt, layout.height_pt)),
        layout.page_number,
    )
}

pub async fn extract_page(
    source: &Path,
    layout: &PageLayout,
    output: &Path,
) -> Result<VectorPage, PdfError> {
    extract_svg(source, layout.page_number, output).await?;
    load_svg(output, layout).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorComponent {
    pub bbox: BBox,
    pub segments: u32,
    pub primitive_ids: Vec<u32>,
    pub non_text_ink_ratio: f64,
    pub path_ink_ratio: f64,
    pub eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEvidence {
    pub schema_version: u32,
    pub primitive_count: u32,
    pub excluded_backgrounds: u32,
    pub excluded_text_glyphs: u32,
    pub excluded_templates: u32,
    pub template_status: TemplateStatus,
    pub mask_width: u32,
    pub mask_height: u32,
    pub components: Vec<VectorComponent>,
    pub incomplete: Vec<String>,
}

pub struct VectorAnalysis {
    pub evidence: VectorEvidence,
    pub mask: Raster,
}

fn edt_1d(f: &[f64], d: &mut [f64]) {
    let n = f.len();
    let mut v = vec![0usize; n];
    let mut z = vec![0.0; n + 1];
    let mut k = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let mut s = ((f[q] + (q * q) as f64) - (f[v[k]] + (v[k] * v[k]) as f64))
            / (2.0 * (q as f64 - v[k] as f64));
        while s <= z[k] {
            k -= 1;
            s = ((f[q] + (q * q) as f64) - (f[v[k]] + (v[k] * v[k]) as f64))
                / (2.0 * (q as f64 - v[k] as f64));
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, value) in d.iter_mut().enumerate() {
        while z[k + 1] < (q as f64) {
            k += 1;
        }
        *value = (q as f64 - v[k] as f64).powi(2) + f[v[k]];
    }
}

fn distance(mask: &[u8], width: usize, height: usize, target: u8) -> Vec<f64> {
    let far = (width * width + height * height + 1) as f64;
    let mut result = vec![0.0; mask.len()];
    let mut f = vec![0.0; width.max(height)];
    let mut d = f.clone();
    for y in 0..height {
        for x in 0..width {
            f[x] = if mask[y * width + x] == target {
                0.0
            } else {
                far
            };
        }
        edt_1d(&f[..width], &mut d[..width]);
        result[y * width..(y + 1) * width].copy_from_slice(&d[..width]);
    }
    for x in 0..width {
        for y in 0..height {
            f[y] = result[y * width + x];
        }
        edt_1d(&f[..height], &mut d[..height]);
        for y in 0..height {
            result[y * width + x] = d[y];
        }
    }
    result
}

/// Exact Euclidean disk closing; padding implements zero background outside the
/// viewport without falsely eroding source ink at the page boundary.
pub fn close_mask(
    mask: &[u8],
    width: usize,
    height: usize,
    radius: usize,
) -> Result<Vec<u8>, PdfError> {
    if width == 0
        || height == 0
        || mask.len() != width.checked_mul(height).ok_or(PdfError::Layout)?
        || width * height > MAX_RENDER_PIXELS as usize
        || radius > 4096
    {
        return Err(PdfError::Layout);
    }
    let w = width + 2 * radius + 2;
    let h = height + 2 * radius + 2;
    if w.checked_mul(h)
        .is_none_or(|n| n > MAX_RENDER_PIXELS as usize * 2)
    {
        return Err(PdfError::UnsafeRender);
    }
    let offset = radius + 1;
    let mut padded = vec![0u8; w * h];
    for y in 0..height {
        padded[(y + offset) * w + offset..(y + offset) * w + offset + width]
            .copy_from_slice(&mask[y * width..(y + 1) * width]);
    }
    let r2 = (radius * radius) as f64;
    let dilated = distance(&padded, w, h, 1)
        .into_iter()
        .map(|d| u8::from(d <= r2))
        .collect::<Vec<_>>();
    let eroded = distance(&dilated, w, h, 0);
    let mut result = vec![0; mask.len()];
    for y in 0..height {
        for x in 0..width {
            result[y * width + x] = u8::from(eroded[(y + offset) * w + x + offset] > r2);
        }
    }
    Ok(result)
}

pub(crate) fn components(mask: &[u8], w: usize, h: usize) -> (Vec<u32>, Vec<[usize; 4]>) {
    let mut labels = vec![0; mask.len()];
    let mut boxes = Vec::new();
    let mut queue = VecDeque::new();
    for at in 0..mask.len() {
        if mask[at] == 0 || labels[at] != 0 {
            continue;
        }
        let id = boxes.len() as u32 + 1;
        labels[at] = id;
        queue.push_back(at);
        let mut bbox = [at % w, at / w, at % w + 1, at / w + 1];
        while let Some(at) = queue.pop_front() {
            let x = at % w;
            let y = at / w;
            bbox = [
                bbox[0].min(x),
                bbox[1].min(y),
                bbox[2].max(x + 1),
                bbox[3].max(y + 1),
            ];
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let next = yy * w + xx;
                    if mask[next] != 0 && labels[next] == 0 {
                        labels[next] = id;
                        queue.push_back(next);
                    }
                }
            }
        }
        boxes.push(bbox);
    }
    (labels, boxes)
}

/// Continuous union area, not a sum that double-counts overlapping text boxes.
pub fn union_area(boxes: &[BBox]) -> f64 {
    let mut xs = boxes
        .iter()
        .filter(|bbox| bbox.is_valid())
        .flat_map(|bbox| [bbox.x0, bbox.x1])
        .collect::<Vec<_>>();
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    let mut area = 0.0;
    for x in xs.windows(2) {
        let mut intervals = boxes
            .iter()
            .filter(|bbox| bbox.is_valid() && bbox.x0 < x[1] && bbox.x1 > x[0])
            .map(|bbox| (bbox.y0, bbox.y1))
            .collect::<Vec<_>>();
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut end = f32::NEG_INFINITY;
        let mut covered = 0.0;
        for (y0, y1) in intervals {
            if y1 > end {
                covered += f64::from(y1 - y0.max(end));
                end = y1;
            }
        }
        area += f64::from(x[1] - x[0]) * covered;
    }
    area
}

fn interior(bbox: BBox, inset: f32) -> Option<BBox> {
    let candidate = BBox {
        x0: bbox.x0 + inset,
        y0: bbox.y0 + inset,
        x1: bbox.x1 - inset,
        y1: bbox.y1 - inset,
    };
    candidate.is_valid().then_some(candidate)
}

pub async fn analyze(
    page: &VectorPage,
    layout: &PageLayout,
    templates: &VectorTemplates,
    page_raster: &Raster,
    output: &Path,
) -> Result<VectorAnalysis, PdfError> {
    let page_area = f64::from(page.width) * f64::from(page.height);
    let text_boxes = layout
        .lines
        .iter()
        .map(|line| line.bbox)
        .filter(|bbox| bbox.is_valid())
        .collect::<Vec<_>>();
    let mut excluded_backgrounds = 0u32;
    let mut excluded_text_glyphs = 0u32;
    let mut excluded_templates = 0u32;
    let kept = page
        .primitives
        .iter()
        .filter(|primitive| {
            let fraction = f64::from(primitive.bbox.area()) / page_area;
            if fraction >= 0.85 && primitive.axis_rectangle {
                if primitive.solid_fill {
                    excluded_backgrounds += 1;
                    return false;
                }
                if primitive.stroked
                    && let Some(inner) = interior(primitive.bbox, 2.0)
                    && page_raster
                        .variance(inner, (page.width, page.height), 150)
                        .is_ok_and(|variance| variance <= UNIFORM_VARIANCE)
                {
                    excluded_backgrounds += 1;
                    return false;
                }
            }
            // Some PDFs outline text as direct paths instead of glyph `<use>`
            // references. Digital text already carries that content, so an
            // almost entirely text-covered primitive never enters `M_vec`.
            let covered = text_boxes
                .iter()
                .filter_map(|text| text.intersection(primitive.bbox))
                .collect::<Vec<_>>();
            if union_area(&covered) >= 0.90 * f64::from(primitive.bbox.area()) {
                excluded_text_glyphs += 1;
                return false;
            }
            if templates.excluded(&primitive.key) {
                excluded_templates += 1;
                return false;
            }
            true
        })
        .collect::<Vec<_>>();

    let [_, _, width_px, height_px] = pixel_bbox(
        BBox {
            x0: 0.0,
            y0: 0.0,
            x1: page.width,
            y1: page.height,
        },
        page.width,
        page.height,
        150,
    )?;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {} {}\"><defs>{}</defs>{}</svg>",
        page.width,
        page.height,
        page.width,
        page.height,
        page.defs,
        kept.iter()
            .map(|primitive| primitive.svg.as_str())
            .collect::<String>()
    );
    let svg_path = output.with_extension("svg");
    tokio::fs::write(&svg_path, svg)
        .await
        .map_err(|_| PdfError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&svg_path, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|_| PdfError::Io)?;
    }
    let status = run_helper(
        Command::new("rsvg-convert")
            .args([
                "-w",
                &width_px.to_string(),
                "-h",
                &height_px.to_string(),
                "-o",
            ])
            .arg(output)
            .arg(&svg_path),
        HelperLimits::pdf(Duration::from_secs(45), 64 * 1024),
    )
    .await?;
    if !status.status.success() {
        return Err(PdfError::Tool);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&svg_path, output] {
            tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .await
                .map_err(|_| PdfError::Io)?;
        }
    }
    let mask_raster = Raster::load(output)?;
    if mask_raster.width != width_px || mask_raster.height != height_px {
        return Err(PdfError::Layout);
    }
    let w = width_px as usize;
    let h = height_px as usize;
    let mask = mask_raster
        .gray
        .iter()
        .map(|value| u8::from(*value < 250))
        .collect::<Vec<_>>();
    let radius = ((width_px as f64) * 0.015).round().max(1.0) as usize;
    let closed = close_mask(&mask, w, h, radius)?;
    let (labels, boxes) = components(&closed, w, h);
    if boxes.len() > MAX_COMPONENTS {
        return Err(PdfError::UnsafeRender);
    }

    let mut per_component = vec![HashMap::<u32, u32>::new(); boxes.len() + 1];
    let mut traced = 0usize;
    for (index, primitive) in kept.iter().enumerate() {
        let mut touched = HashSet::new();
        for segment in &primitive.segments {
            let steps = segment.steps();
            for step in 0..=steps {
                traced += 1;
                if traced > MAX_TRACE_SAMPLES {
                    return Err(PdfError::UnsafeRender);
                }
                let point = segment.point(step as f64 / steps as f64);
                let x = (point.x * DPI).floor() as i32;
                let y = (point.y * DPI).floor() as i32;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let xx = x + dx;
                        let yy = y + dy;
                        if xx >= 0 && yy >= 0 && xx < width_px as i32 && yy < height_px as i32 {
                            let at = (yy as usize * w) + xx as usize;
                            if labels[at] != 0 {
                                touched.insert(labels[at]);
                            }
                        }
                    }
                }
            }
        }
        for label in touched {
            per_component[label as usize].insert(index as u32, primitive.segment_count as u32);
        }
    }

    let mut result = Vec::new();
    for (index, [x0, y0, x1, y1]) in boxes.into_iter().enumerate() {
        let bbox = BBox {
            x0: (x0 as f32 * 72.0 / 150.0).min(page.width),
            y0: (y0 as f32 * 72.0 / 150.0).min(page.height),
            x1: (x1 as f32 * 72.0 / 150.0).min(page.width),
            y1: (y1 as f32 * 72.0 / 150.0).min(page.height),
        };
        if !bbox.is_valid() {
            continue;
        }
        let bbox_area = f64::from(bbox.area());
        let fraction = bbox_area / page_area;
        let mut ink = 0u64;
        for y in y0..y1 {
            for x in x0..x1 {
                ink += u64::from(mask[y * w + x]);
            }
        }
        let pixel_area = ((x1 - x0) * (y1 - y0)) as f64;
        let clipped = text_boxes
            .iter()
            .filter_map(|text| text.intersection(bbox))
            .collect::<Vec<_>>();
        let non_text = (1.0 - union_area(&clipped) / bbox_area).clamp(0.0, 1.0);
        let path_ink = if pixel_area > 0.0 {
            ink as f64 / pixel_area
        } else {
            0.0
        };
        let entries = &per_component[index + 1];
        let primitive_ids = entries.keys().copied().collect::<Vec<_>>();
        let segments = entries.values().sum::<u32>();
        let eligible = segments >= 25
            && (0.04..0.90).contains(&fraction)
            && non_text >= 0.35
            && path_ink >= 0.05;
        result.push(VectorComponent {
            bbox,
            segments,
            primitive_ids,
            non_text_ink_ratio: non_text,
            path_ink_ratio: path_ink,
            eligible,
        });
    }
    let mut incomplete = page.incomplete.clone();
    if templates.status == TemplateStatus::Incomplete {
        incomplete.push("vector_template_deck_incomplete".into());
    }
    incomplete.sort_unstable();
    incomplete.dedup();
    Ok(VectorAnalysis {
        evidence: VectorEvidence {
            schema_version: 1,
            primitive_count: page.primitives.len() as u32,
            excluded_backgrounds,
            excluded_text_glyphs,
            excluded_templates,
            template_status: templates.status,
            mask_width: width_px,
            mask_height: height_px,
            components: result,
            incomplete,
        },
        mask: mask_raster,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_joins_small_gaps_and_preserves_page_edge_ink() {
        let mut mask = vec![0; 100 * 50];
        for y in 0..30 {
            for x in 0..25 {
                mask[y * 100 + x] = 1;
            }
            for x in 28..50 {
                mask[y * 100 + x] = 1;
            }
        }
        let closed = close_mask(&mask, 100, 50, 3).unwrap();
        assert_eq!(closed[0], 1);
        assert_eq!(components(&closed, 100, 50).1.len(), 1);
    }

    #[test]
    fn closing_and_component_connectivity_have_executable_oracles() {
        for radius in [0usize, 1, 2, 3] {
            let mask = (0..20 * 15)
                .map(|i| u8::from((i * 17 + i / 20 * 7) % 23 < 5))
                .collect::<Vec<_>>();
            let actual = close_mask(&mask, 20, 15, radius).unwrap();
            let pad = radius + 1;
            let w = 20 + 2 * pad;
            let h = 15 + 2 * pad;
            let mut dilation = vec![0; w * h];
            let offsets = (-(radius as isize)..=radius as isize)
                .flat_map(|dy| {
                    (-(radius as isize)..=radius as isize).filter_map(move |dx| {
                        (dx * dx + dy * dy <= radius as isize * radius as isize).then_some((dx, dy))
                    })
                })
                .collect::<Vec<_>>();
            for y in 0..15 {
                for x in 0..20 {
                    if mask[y * 20 + x] != 0 {
                        for &(dx, dy) in &offsets {
                            dilation[((y + pad) as isize + dy) as usize * w
                                + ((x + pad) as isize + dx) as usize] = 1;
                        }
                    }
                }
            }
            for y in 0..15 {
                for x in 0..20 {
                    let expected = u8::from(offsets.iter().all(|&(dx, dy)| {
                        dilation[((y + pad) as isize + dy) as usize * w
                            + ((x + pad) as isize + dx) as usize]
                            != 0
                    }));
                    assert_eq!(actual[y * 20 + x], expected, "r={radius}, x={x}, y={y}");
                }
            }
        }
        assert_eq!(components(&[1, 0, 0, 1], 2, 2).1.len(), 1, "8-connectivity");
    }

    #[test]
    fn glyph_text_is_not_part_of_the_vector_mask() {
        let svg = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100pt" height="100pt" viewBox="0 0 100 100">
<defs><g id="glyph-0-0"><path d="M 0 0 L 10 10 L 0 20 Z"/></g></defs>
<g fill="black"><use xlink:href="#glyph-0-0" x="5" y="5"/></g>
<rect x="20" y="20" width="30" height="20" fill="black"/>
</svg>"##;
        let page = parse_svg(svg, Some((100.0, 100.0)), 1).unwrap();
        assert_eq!(page.primitives.len(), 1);
        assert!(page.primitives[0].axis_rectangle);
        assert_eq!(page.primitives[0].segment_count, 4);
    }

    #[test]
    fn template_counter_requires_both_frozen_thresholds_and_a_complete_deck() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100pt" height="100pt" viewBox="0 0 100 100">
<rect x="1" y="1" width="10" height="10" fill="black"/></svg>"#;
        let mut deck = Vec::new();
        for page in 1..=5 {
            deck.push(parse_svg(svg, Some((100.0, 100.0)), page).unwrap());
        }
        assert_eq!(
            VectorTemplates::detect(&deck).status,
            TemplateStatus::Complete
        );
        assert_eq!(VectorTemplates::detect(&deck).keys.len(), 1);

        let mut counter = TemplateCounter::default();
        for page in &deck {
            counter.observe(page);
        }
        counter.fail();
        let incomplete = counter.finish();
        assert_eq!(incomplete.status, TemplateStatus::Incomplete);
        assert!(incomplete.keys.is_empty());
    }

    #[test]
    fn raster_soft_mask_is_partial_instead_of_unmasked_vector_ink() {
        // Original synthetic structure: a gradient-painted rectangle with a
        // raster alpha mask, like Cairo's mixed raster/vector output. Neither
        // the image nor an unmasked replacement rectangle may enter M_vec.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 100 100">
<defs>
  <image id="alpha" width="100" height="100" xlink:href="data:image/png;base64,AA=="/>
  <linearGradient id="gradient"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient>
  <mask id="soft"><g><use xlink:href="#alpha"/></g></mask>
</defs>
<g mask="url(#soft)"><rect width="100" height="100" fill="url(#gradient)"/></g>
<rect x="20" y="20" width="30" height="20" fill="black" stroke="none"/>
</svg>"##;
        let page = parse_svg(svg, Some((100.0, 100.0)), 1).unwrap();
        assert_eq!(page.incomplete, ["unsupported_vector_group"]);
        assert_eq!(page.primitives.len(), 1, "safe siblings still survive");
        assert_eq!(page.primitives[0].bbox.x0, 20.0);
        assert!(!page.defs.contains("image"));
        assert!(!page.primitives[0].svg.contains("mask"));
    }

    #[test]
    fn masked_vector_use_is_not_silently_expanded() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 100 100">
<defs><g id="diagram"><rect x="10" y="10" width="60" height="60"/></g></defs>
<use xlink:href="#diagram" mask="url(#unhandled)"/>
</svg>"##;
        let page = parse_svg(svg, None, 1).unwrap();
        assert!(page.primitives.is_empty());
        assert_eq!(page.incomplete, ["filtered_vector_use"]);
    }

    #[test]
    fn unsafe_svg_content_is_rejected() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><script>alert(1)</script></svg>"#;
        assert!(parse_svg(svg, None, 1).is_err());
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path onload="x" d="M0 0L1 1"/></svg>"#;
        let page = parse_svg(svg, None, 1).unwrap();
        assert!(page.primitives.is_empty());
        assert!(
            page.incomplete
                .iter()
                .any(|note| note.contains("unsupported"))
        );
    }
}
