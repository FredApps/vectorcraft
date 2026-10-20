//! Interpret the public CDR record layouts into native document objects.
use crate::{Imported, bitmap, container::Chunk, reader::Reader};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use vectorcraft_color::{Color, Gradient, GradientGeom, GradientKind, GradientPaint, GradientStop, Paint};
use vectorcraft_doc::{
    Appearance, AppearanceItem, Artboard, Dash, Document, FillLayer, ImageObject, LayerColor, LineCap, LineJoin, Node, NodeId, NodeKind, StrokeLayer,
};
use vectorcraft_geom::{Affine, BezPath, FillRule, PathData, Point, Rect, Shape};

const MAX_POINTS: usize = 1_000_000;
const MAX_COORD: f64 = 1e6;

/// Build editable pages in points from native records, retaining omission warnings.
pub(crate) fn import(chunks: &[Chunk], version: u32) -> Result<Imported, String> {
    let mut d = Drawing::new(version);
    let mut size = (612.0 / 72.0, 792.0 / 72.0);
    d.styles(chunks, &mut size, &mut BTreeSet::new())?;
    let mut pages = Vec::new();
    find_lists(chunks, b"page", &mut pages);
    if pages.is_empty() {
        return Err("CDR contains no native drawing pages".into());
    }
    if pages.len() > 1000 {
        return Err("CDR page limit exceeded".into());
    }
    let mut document = Document::new(size.0 * 72.0, size.1 * 72.0);
    document.layers.clear();
    document.artboards.clear();
    let mut x = 0.0;
    for page in pages {
        if page.child(b"flgs").and_then(|c| c.data.get(2)).is_some_and(|v| *v != 0) {
            if has_art(page) {
                d.warn("Master-page artwork is omitted");
            }
            continue;
        }
        let mut page_size = size;
        if let Some(loda) = metadata(page, b"loda").or_else(|| metadata(page, b"lobj")) {
            let (_, args) = arguments(&loda.data)?;
            if let Some(arg) = args.get(&19130) {
                let mut r = Reader::new(arg);
                page_size = valid_size(r.coord()?, r.coord()?)?;
            }
        }
        let width = page_size.0 * 72.0;
        let height = page_size.1 * 72.0;
        let index = document.artboards.len();
        document.artboards.push(Artboard {
            id: index as u32 + 1,
            name: format!("Page {}", index + 1),
            rect: Rect::new(x, 0.0, x + width, height),
            show_center_mark: false,
            show_cross_hairs: false,
        });
        // Corel coordinates are y-up, with their origin at the page centre.
        let page_xf = Affine::new([72.0, 0.0, 0.0, -72.0, x + width / 2.0, height / 2.0]);
        let nodes = d.children(&page.children, page_xf)?;
        let mut layer = Node::layer(d.id(), &format!("Page {}", index + 1), LayerColor::Preset(index as u8 % 27));
        if let Some(children) = layer.children_mut() {
            *children = nodes;
        }
        document.layers.push(Arc::new(layer));
        x += width + 36.0;
    }
    if document.artboards.is_empty() {
        return Err("CDR contains no content pages".into());
    }
    if d.objects_seen > 0 && d.objects_imported == 0 {
        return Err("CDR contains no supported editable artwork; convert text to curves or export the drawing as SVG/PDF in CorelDRAW".into());
    }
    document.fix_next_id();
    document.images = d.bitmaps.into_values().map(|b| (b.key, b.blob)).collect();
    let mut warnings = d.warnings.into_iter().collect();
    document.limit_depth_noting(&mut warnings);
    Ok(Imported { document, version, warnings })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// Extract the original generated native bitmap without using a producer fixture.
    fn bitmap_data() -> Vec<u8> {
        let (chunks, _) = crate::container::load(&crate::synth::clipped_bitmap()).unwrap();
        chunks.into_iter().find(|c| c.id == *b"bmp ").unwrap().data
    }

    /// Materialize a bitmap leaf as the container reader does for an external alias.
    fn bitmap_chunk(data: Vec<u8>) -> Chunk {
        Chunk { id: *b"bmp ", list: None, data, children: Vec::new() }
    }

    /// Repeated resource aliases inside nested lists must neither decode nor charge twice.
    #[test]
    fn exact_bitmap_aliases_share_decode_and_budget() {
        let data = bitmap_data();
        let records = [
            bitmap_chunk(data.clone()),
            Chunk { id: *b"LIST", list: Some(*b"grp "), data: Vec::new(), children: vec![bitmap_chunk(data.clone()), bitmap_chunk(data)] },
        ];
        let mut drawing = Drawing::new(1200);
        let mut seen = BTreeSet::new();
        let mut size = (1.0, 1.0);
        drawing.styles(&records[..1], &mut size, &mut seen).unwrap();
        let budget = drawing.bitmap_budget.clone();
        drawing.styles(&records[1..], &mut size, &mut seen).unwrap();
        assert_eq!(drawing.bitmaps.len(), 1);
        assert_eq!(seen.len(), 1);
        assert_eq!(drawing.bitmap_budget, budget);
    }

    /// Nonpixel metadata may vary; distinct decoding work stays charged, exact repeats do not.
    #[test]
    fn equivalent_bitmap_metadata_keeps_first_image_and_bounds_work() {
        let data = bitmap_data();
        let mut variant = data.clone();
        variant[16] ^= 1; // Outer resource metadata is not part of the RI pixel header.
        let records = [bitmap_chunk(data), bitmap_chunk(variant.clone()), bitmap_chunk(variant)];
        let mut drawing = Drawing::new(1200);
        let mut seen = BTreeSet::new();
        let mut size = (1.0, 1.0);
        drawing.styles(&records[..1], &mut size, &mut seen).unwrap();
        let first = drawing.bitmaps.get(&7).unwrap().blob.bytes.clone();
        let budget = drawing.bitmap_budget.clone();
        drawing.styles(&records[1..2], &mut size, &mut seen).unwrap();
        assert_ne!(drawing.bitmap_budget, budget);
        let budget = drawing.bitmap_budget.clone();
        drawing.styles(&records[2..], &mut size, &mut seen).unwrap();
        assert_eq!(drawing.bitmaps.len(), 1);
        assert_eq!(drawing.bitmaps.get(&7).unwrap().blob.bytes, first);
        assert_eq!(drawing.bitmap_budget, budget);
        assert_eq!(seen.len(), 2);
    }

    /// A changed actual pixel under the same resource identifier is an error, not a replacement.
    #[test]
    fn conflicting_bitmap_pixels_reject_without_replacing_first() {
        let data = bitmap_data();
        let mut variant = data.clone();
        variant[40 + 78] ^= 255; // First BGR pixel after the native RI header.
        let records = [bitmap_chunk(data), bitmap_chunk(variant)];
        let mut drawing = Drawing::new(1200);
        let mut seen = BTreeSet::new();
        let mut size = (1.0, 1.0);
        drawing.styles(&records[..1], &mut size, &mut seen).unwrap();
        let first = drawing.bitmaps.get(&7).unwrap().blob.bytes.clone();
        let error = drawing.styles(&records[1..], &mut size, &mut seen).unwrap_err();
        assert!(error.contains("conflicting CDR bitmap resource identifier"));
        assert_eq!(drawing.bitmaps.len(), 1);
        assert_eq!(drawing.bitmaps.get(&7).unwrap().blob.bytes, first);
    }

    /// A complete native drawing still opens when an external list repeats its bitmap definition.
    #[test]
    fn drawing_import_accepts_nested_bitmap_alias() {
        let (mut chunks, version) = crate::container::load(&crate::synth::clipped_bitmap()).unwrap();
        chunks.push(Chunk { id: *b"LIST", list: Some(*b"bmpf"), data: Vec::new(), children: vec![bitmap_chunk(bitmap_data())] });
        let imported = import(&chunks, version).unwrap();
        assert_eq!(imported.document.images.len(), 1);
        assert!(!imported.document.layers.is_empty());
    }
}

/// Detect object records even inside containers whose artwork is unsupported.
fn has_art(c: &Chunk) -> bool {
    c.list.as_ref().is_some_and(|k| k == b"obj ") || c.children.iter().any(has_art)
}
/// Find this object's property, descending through `lgob`/`trfl` wrappers only.
fn metadata<'a>(c: &'a Chunk, id: &[u8; 4]) -> Option<&'a Chunk> {
    c.child(id).or_else(|| c.children.iter().filter(|c| matches!(c.list.as_ref(), Some(b"lgob" | b"trfl"))).find_map(|c| metadata(c, id)))
}
/// Collect matching lists without descending into a match a second time.
fn find_lists<'a>(chunks: &'a [Chunk], kind: &[u8; 4], out: &mut Vec<&'a Chunk>) {
    for c in chunks {
        if c.list.as_ref() == Some(kind) {
            out.push(c);
        } else {
            find_lists(&c.children, kind, out);
        }
    }
}
/// Validate positive page dimensions in inches against the document point limit.
fn valid_size(w: f64, h: f64) -> Result<(f64, f64), String> {
    if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 && w * 72.0 <= MAX_COORD && h * 72.0 <= MAX_COORD {
        Ok((w, h))
    } else {
        Err("invalid CDR page dimensions".into())
    }
}

/// Read an object kind and bounded borrowed arguments from its `loda` table.
/// Offsets are stored in forward order and argument types in reverse order.
fn arguments(data: &[u8]) -> Result<(u32, BTreeMap<u32, &[u8]>), String> {
    let mut r = Reader::new(data);
    let len = r.u32()? as usize;
    let count = r.u32()? as usize;
    let offsets = r.u32()? as usize;
    let types = r.u32()? as usize;
    let kind = r.u32()?;
    if len > data.len() || count > 4096 {
        return Err("invalid CDR object argument table".into());
    }
    let mut ro = r.at(offsets)?;
    let mut rt = r.at(types)?;
    let mut off = Vec::new();
    let mut typ = Vec::new();
    for _ in 0..count {
        off.push(ro.u32()? as usize);
        typ.push(rt.u32()?);
    }
    // Some files omit the sentinel offset. End each argument at the next offset,
    // or at the first following table/end boundary.
    let mut boundaries = off.clone();
    boundaries.extend([offsets, types, len]);
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut result = BTreeMap::new();
    for (i, &start) in off.iter().enumerate() {
        let end = *boundaries.get(boundaries.partition_point(|v| *v <= start)).ok_or("invalid CDR argument offset")?;
        if end > len {
            return Err("CDR argument exceeds its declared object length".into());
        }
        let id = typ.get(count - 1 - i).copied().ok_or("missing CDR argument type")?;
        result.insert(id, data.get(start..end).ok_or("CDR argument outside record")?);
    }
    Ok((kind, result))
}

struct Drawing {
    version: u32,
    fills: BTreeMap<u32, Fill>,
    strokes: BTreeMap<u32, Option<StrokeLayer>>,
    warnings: BTreeSet<String>,
    points: usize,
    next_id: u64,
    objects_seen: usize,
    objects_imported: usize,
    paint_values: usize,
    bitmaps: BTreeMap<u32, bitmap::Bitmap>,
    bitmap_budget: bitmap::BitmapBudget,
}

#[derive(Clone)]
enum Fill {
    Plain(Paint),
    Fountain(Fountain),
}

#[derive(Clone)]
struct Fountain {
    kind: GradientKind,
    stops: Vec<GradientStop>,
    angle: f64,
    edge: f64,
    center: (f64, f64),
    /// Modern gradient centre offset and axis vector, relative to object bounds.
    tail: Option<[f64; 4]>,
}
impl Fill {
    /// Place a solid or fountain paint in the object's bounds and document transform.
    fn paint(&self, bounds: Rect, xf: Affine) -> Result<Paint, String> {
        match self {
            Self::Plain(paint) => Ok(paint.clone()),
            Self::Fountain(f) => {
                let mut geom = GradientGeom::fit(f.kind, bounds, -f.angle);
                let center = bounds.center();
                let offset = vectorcraft_geom::Vec2::new(bounds.width() * f.center.0 / 100.0, bounds.height() * f.center.1 / 100.0);
                geom.start += offset;
                geom.end += offset;
                if f.kind == GradientKind::Linear {
                    let mid = geom.start.midpoint(geom.end);
                    let v = (geom.end - geom.start) * (1.0 - 2.0 * f.edge);
                    geom.start = mid - v * 0.5;
                    geom.end = mid + v * 0.5;
                } else {
                    geom.end = geom.start + (geom.end - geom.start) * (1.0 - 2.0 * f.edge);
                }
                if let Some([cx, cy, ax, ay]) = f.tail {
                    let c = center + vectorcraft_geom::Vec2::new(cx * bounds.width(), -cy * bounds.height());
                    let v = vectorcraft_geom::Vec2::new(ax * bounds.width(), -ay * bounds.height());
                    if v.hypot() > 1e-12 {
                        if f.kind == GradientKind::Linear {
                            geom.start = c - v * 0.5;
                            geom.end = c + v * 0.5;
                        } else {
                            geom.start = c;
                            geom.end = c + v * 0.5;
                            geom.aspect = if bounds.width().abs() > 1e-12 { bounds.height() / bounds.width() } else { 1.0 };
                        }
                    }
                }
                geom.transform(xf, f.kind);
                if ![geom.start.x, geom.start.y, geom.end.x, geom.end.y, geom.aspect].iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD) {
                    return Err("CDR gradient placement exceeds document limits".into());
                }
                let mut gradient = GradientPaint::new(Gradient { kind: f.kind, stops: f.stops.clone() });
                gradient.geom = Some(geom);
                Ok(Paint::Gradient(Box::new(gradient)))
            }
        }
    }
}
impl Drawing {
    /// Start one import with independent resource maps and cumulative work budgets.
    fn new(version: u32) -> Self {
        Self {
            version,
            fills: BTreeMap::new(),
            strokes: BTreeMap::new(),
            warnings: BTreeSet::new(),
            points: 0,
            next_id: 1,
            objects_seen: 0,
            objects_imported: 0,
            paint_values: 0,
            bitmaps: BTreeMap::new(),
            bitmap_budget: bitmap::BitmapBudget::default(),
        }
    }
    /// Retain at most 100 distinct warnings about omitted or approximated features.
    fn warn(&mut self, text: &str) {
        if self.warnings.len() < 100 {
            self.warnings.insert(text.to_owned());
        }
    }
    /// Allocate a unique document node ID within the bounded record traversal.
    fn id(&mut self) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        NodeId(id)
    }
    /// Collect fill/outline definitions and page defaults before resolving objects.
    fn styles<'a>(&mut self, chunks: &'a [Chunk], size: &mut (f64, f64), bitmap_sources: &mut BTreeSet<&'a [u8]>) -> Result<(), String> {
        for c in chunks {
            if self.fills.len() + self.strokes.len() > 20_000 {
                return Err("CDR style count limit exceeded".into());
            }
            match &c.id {
                b"fild" | b"fill" => {
                    let (id, p) = self.fill(&c.data)?;
                    self.fills.insert(id, p);
                }
                b"outl" => {
                    let (id, s) = self.stroke(&c.data)?;
                    self.strokes.insert(id, s);
                }
                b"mcfg" => {
                    let mut r = Reader::new(&c.data);
                    r.skip(if self.version >= 1300 {
                        12
                    } else if self.version >= 900 {
                        4
                    } else if self.version < 700 {
                        28
                    } else {
                        0
                    })?;
                    *size = valid_size(r.coord()?, r.coord()?)?;
                }
                b"txsm" => self.warn("Text objects are omitted; convert text to curves in CorelDRAW for editable outlines"),
                b"bmp " => {
                    // External LIST aliases materialize identical leaves. Compare borrowed bytes
                    // before decoding so those aliases consume neither image quota nor another PNG.
                    if !bitmap_sources.insert(c.data.as_slice()) {
                        continue;
                    }
                    if let Some(bitmap) = bitmap::decode(&c.data, &mut self.bitmap_budget)? {
                        if bitmap.cmyk_converted {
                            self.warn("CMYK bitmap colours are approximated in RGB without the embedded colour profile");
                        }
                        if let Some(first) = self.bitmaps.get(&bitmap.id) {
                            // Different metadata/padding may encode the same pixels. Compare the
                            // full deterministic PNG, never just its content-derived key. This
                            // distinct decode remains charged to bound actual conversion work.
                            if first.width != bitmap.width
                                || first.height != bitmap.height
                                || first.blob.bytes.as_slice() != bitmap.blob.bytes.as_slice()
                            {
                                return Err("conflicting CDR bitmap resource identifier".into());
                            }
                        } else {
                            self.bitmaps.insert(bitmap.id, bitmap);
                        }
                    } else {
                        self.warn("An unsupported CDR bitmap encoding was omitted");
                    }
                }
                b"bmpf" => self.warn("Bitmap pattern fills are omitted"),
                b"iccd" => self.warn("Embedded colour profiles are not imported; numeric colours are preserved"),
                _ => {}
            }
            self.styles(&c.children, size, bitmap_sources)?;
        }
        Ok(())
    }
    /// Decode supported numeric colour models and warn when substituting black.
    fn color(&mut self, r: &mut Reader<'_>) -> Result<Color, String> {
        let model = r.u16()?;
        let palette = r.u16()?;
        r.skip(4)?;
        let b = r.take(4)?;
        let a = b.first().copied().unwrap_or(0);
        let c = b.get(1).copied().unwrap_or(0);
        let d = b.get(2).copied().unwrap_or(0);
        let e = b.get(3).copied().unwrap_or(0);
        let color = match model {
            2 => Color::cmyk((a as f32 / 100.0).min(1.0), (c as f32 / 100.0).min(1.0), (d as f32 / 100.0).min(1.0), (e as f32 / 100.0).min(1.0)),
            3 | 17 => Color::cmyk(a as f32 / 255.0, c as f32 / 255.0, d as f32 / 255.0, e as f32 / 255.0),
            4 => Color::cmyk(a as f32 / 255.0, c as f32 / 255.0, d as f32 / 255.0, 0.0),
            5 | 21 => Color::rgb8(d, c, a),
            8 => {
                if a == 0 {
                    Color::WHITE
                } else {
                    Color::BLACK
                }
            }
            9 => Color::gray(1.0 - a as f32 / 255.0),
            12 => Color::lab(a as f32 * 100.0 / 255.0, c as i8 as f32, d as i8 as f32),
            18 => Color::lab(a as f32 * 100.0 / 255.0, c as f32 - 128.0, d as f32 - 128.0),
            20 => {
                self.warn("Registration colour is approximated with black");
                Color::BLACK
            }
            _ => {
                self.warn(&format!("Colour model {model} (palette {palette}) is unsupported and uses black"));
                Color::BLACK
            }
        };
        Ok(color)
    }
    /// Read a versioned fill definition, omitting unsupported fill kinds with a warning.
    fn fill(&mut self, bytes: &[u8]) -> Result<(u32, Fill), String> {
        let mut r = Reader::new(bytes);
        let id = r.u32()?;
        if self.version >= 1300 {
            loop {
                let tag = r.u32()?;
                let len = r.u32()? as usize;
                if tag == 1300 {
                    break;
                }
                r.skip(len)?;
            }
        }
        let kind = r.u16()?;
        if kind == 0 {
            return Ok((id, Fill::Plain(Paint::None)));
        }
        if kind == 2 {
            return Ok((id, Fill::Fountain(self.fountain(&mut r)?)));
        }
        if kind != 1 {
            self.warn(&format!("CDR fill type {kind} is omitted (solid, linear and radial fills are supported)"));
            return Ok((id, Fill::Plain(Paint::None)));
        }
        let color = if self.version < 1300 {
            r.skip(2)?;
            self.color(&mut r)?
        } else {
            r.skip(4)?;
            let n = r.u32()? as usize;
            let mut props = Reader::new(r.take(n)?);
            let mut found = None;
            for _ in 0..1024 {
                let kind = props.u8()?;
                let n = props.u32()? as usize;
                let data = props.take(n)?;
                if kind == 0 {
                    break;
                }
                if kind == 1 {
                    found = Some(self.color(&mut Reader::new(data))?);
                }
            }
            found.ok_or("CDR solid fill has no colour property")?
        };
        Ok((id, Fill::Plain(Paint::solid(color))))
    }
    /// Read bounded fountain stops and placement, recording interpolation approximations.
    fn fountain(&mut self, r: &mut Reader<'_>) -> Result<Fountain, String> {
        r.skip(if self.version >= 1300 { 8 } else { 2 })?;
        let kind = r.u8()?;
        let kind = match kind {
            1 => GradientKind::Linear,
            2 => GradientKind::Radial,
            _ => {
                self.warn("Conical or square fountain fills are approximated with a radial gradient");
                GradientKind::Radial
            }
        };
        r.skip(if self.version >= 1300 { 17 } else { 19 })?;
        let edge = if self.version >= 1300 { r.u16()? as i16 as f64 } else { r.u32()? as i32 as f64 } / 100.0;
        let angle = r.u32()? as i32 as f64 / 1_000_000.0;
        let center = (r.u32()? as i32 as f64, r.u32()? as i32 as f64);
        r.skip(2)?;
        let mode = r.u32()? & 0xff;
        let midpoint = r.u8()? as f32 / 100.0;
        r.skip(1)?;
        let count = (r.u32()? & 0xffff) as usize;
        if count > 4096 {
            return Err("CDR gradient stop limit exceeded".into());
        }
        if self.version >= 1300 {
            r.skip(3)?;
        }
        let mut stops = Vec::new();
        self.add_paint_values(count)?;
        for _ in 0..count {
            let color = self.color(r)?;
            r.skip(if self.version >= 1500 {
                26
            } else if self.version >= 1300 {
                5
            } else {
                0
            })?;
            let offset = (r.u32()? & 0xffff) as f32 / 100.0;
            if offset > 1.0 {
                return Err("invalid CDR gradient stop position".into());
            }
            let mut stop = GradientStop::new(if kind == GradientKind::Radial { 1.0 - offset } else { offset }, color);
            stop.midpoint = midpoint.clamp(0.01, 0.99);
            stops.push(stop);
            if self.version >= 1300 {
                r.skip(3)?;
            }
        }
        if stops.len() < 2 {
            return Err("CDR gradient contains fewer than two stops".into());
        }
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        if mode != 0 {
            self.warn("Fountain colour interpolation modes are approximated in the document colour model");
        }
        if self.version >= 1300 {
            r.skip(3)?;
        }
        let tail =
            if self.version >= 1600 && r.bytes.len().saturating_sub(r.pos) >= 32 { Some([r.f64()?, r.f64()?, r.f64()?, r.f64()?]) } else { None };
        if edge.abs() > 0.5 || center.0.abs() > 1000.0 || center.1.abs() > 1000.0 || tail.is_some_and(|v| v.iter().any(|v| v.abs() > 1000.0)) {
            return Err("invalid CDR gradient placement".into());
        }
        if tail.is_some_and(|v| v.first().is_some_and(|v| *v != 0.0) || v.get(1).is_some_and(|v| *v != 0.0) || v.get(3).is_some_and(|v| *v != 0.0)) {
            self.warn("Advanced fountain centre offsets and rotated axes may be approximated");
        }
        Ok(Fountain { kind, stops, angle, edge, center, tail })
    }
    /// Decode outline width, colour, caps, joins and dashes, or an explicit no-outline style.
    fn stroke(&mut self, bytes: &[u8]) -> Result<(u32, Option<StrokeLayer>), String> {
        let mut r = Reader::new(bytes);
        let id = r.u32()?;
        if self.version >= 1300 {
            loop {
                let kind = r.u32()?;
                let len = r.u32()? as usize;
                if kind == 1 {
                    break;
                }
                r.skip(len)?;
            }
        }
        let kind = r.u16()?;
        let cap = r.u16()?;
        let join = r.u16()?;
        if self.version < 1300 {
            r.skip(2)?;
        }
        let width = r.coord()?.abs() * 72.0;
        let stretch = r.u16()?;
        if self.version < 1300 {
            r.skip(2)?;
        }
        let angle = r.u32()?;
        r.skip(if self.version >= 1300 { 48 } else { 52 })?;
        let color = self.color(&mut r)?;
        if kind & 1 != 0 {
            return Ok((id, None));
        }
        let mut stroke = StrokeLayer::new(Paint::solid(color), width.max(0.01));
        stroke.cap = match cap {
            1 => LineCap::Round,
            2 => LineCap::Square,
            _ => LineCap::Butt,
        };
        stroke.join = match join {
            1 => LineJoin::Round,
            2 => LineJoin::Bevel,
            _ => LineJoin::Miter,
        };
        if stretch != 100 || angle != 0 {
            self.warn("Calligraphic outline stretch and angle are approximated with a circular pen");
        }
        if kind & 0x20 != 0 {
            self.warn("Outlines that scale with objects use their stored widths");
        }
        if kind & 0x10 != 0 {
            self.warn("Outlines behind the fill are drawn above it");
        }
        r.skip(16)?;
        let count = r.u16()? as usize;
        if count > 1024 {
            return Err("CDR outline dash limit exceeded".into());
        }
        self.add_paint_values(count)?;
        let dash_start = r.pos;
        if count > 0 && kind & 4 != 0 {
            let mut pattern = Vec::new();
            for _ in 0..count {
                pattern.push(r.u16()? as f64 * stroke.width);
            }
            stroke.dash = Some(Dash { pattern, ..Dash::default() });
        }
        let mut arrows = r.at(dash_start + if self.version >= 1300 { 20 } else { 22 })?;
        if arrows.u32()? != 0 || arrows.u32()? != 0 {
            self.warn("Outline arrowheads are omitted");
        }
        Ok((id, Some(stroke)))
    }
    /// Compose this object's affine transforms, converting their translations to inches.
    fn transform(&mut self, c: &Chunk) -> Result<Affine, String> {
        let Some(c) = metadata(c, b"trfd") else { return Ok(Affine::IDENTITY) };
        let mut r = Reader::new(&c.data);
        let len = r.u32()? as usize;
        let count = r.u32()? as usize;
        let table = r.u32()? as usize;
        if count > 4096 || len > c.data.len() {
            return Err("invalid CDR transform table".into());
        }
        let mut offsets = r.at(table)?;
        let mut xf = Affine::IDENTITY;
        for _ in 0..count {
            let offset = offsets.u32()? as usize;
            let mut value = r.at(offset)?;
            if self.version >= 1300 {
                value.skip(8)?;
            }
            if value.u16()? != 8 {
                self.warn("A non-affine CDR transform is omitted");
                continue;
            }
            value.skip(6)?;
            let a = value.f64()?;
            let c = value.f64()?;
            let tx = value.f64()? / 254000.0;
            let b = value.f64()?;
            let d = value.f64()?;
            let ty = value.f64()? / 254000.0;
            xf *= Affine::new([a, b, c, d, tx, ty]);
        }
        Ok(xf)
    }
    /// Assemble layers, groups and objects in document paint order under the parent transform.
    fn children(&mut self, chunks: &[Chunk], parent: Affine) -> Result<Vec<Arc<Node>>, String> {
        let mut nodes = Vec::new();
        for c in chunks {
            let Some(kind) = c.list.as_ref() else { continue };
            if kind == b"obj " {
                if let Some(n) = self.object(c, parent)? {
                    nodes.push(Arc::new(n));
                }
            } else if kind == b"grp " || kind == b"lnkg" || kind == b"layr" {
                // Ordinary group children already carry the complete transform.
                // Linked brush groups place children through the group's matrix.
                let xf = if kind == b"lnkg" { parent * self.transform(c)? } else { parent };
                let children = self.children(&c.children, xf)?;
                let mut group = if kind == b"layr" {
                    let mut n = Node::layer(self.id(), "Layer", LayerColor::Preset(0));
                    if let Some(c) = n.children_mut() {
                        *c = children;
                    }
                    n
                } else {
                    Node::group(self.id(), children)
                };
                if let Some(loda) = metadata(c, b"loda").or_else(|| metadata(c, b"lobj")) {
                    let (_, args) = arguments(&loda.data)?;
                    group.name = self.name(args.get(&1000).copied());
                    group.opacity = self.opacity(args.get(&8000).copied())?;
                }
                if kind == b"layr" {
                    let layer_type = c.child(b"flgs").and_then(|c| c.data.first()).copied().unwrap_or(0);
                    if layer_type & 0x0f == 0x0a {
                        group.visible = false;
                        if let vectorcraft_doc::NodeKind::Layer { printable, .. } = &mut group.kind {
                            *printable = false;
                        }
                    }
                }
                if group.children().is_some_and(|c| !c.is_empty()) {
                    nodes.push(Arc::new(group));
                }
            } else if kind == b"clpt" {
                self.warn("Clipping containers are omitted");
                if has_art(c) {
                    self.objects_seen += 1;
                }
            } else if kind != b"vect" && kind != b"stlt" {
                nodes.extend(self.children(&c.children, parent)?);
            }
        }
        // Native records store the frontmost object/layer first.
        nodes.reverse();
        Ok(nodes)
    }
    /// Decode a bounded optional name as UTF-16 for CDR 12+, or lossy UTF-8 for older files.
    fn name(&self, bytes: Option<&[u8]>) -> Option<String> {
        let bytes = bytes?;
        let name = if self.version >= 1200 {
            let words: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .take(4096)
                .map(|p| u16::from_le_bytes([p.first().copied().unwrap_or(0), p.get(1).copied().unwrap_or(0)]))
                .take_while(|v| *v != 0)
                .collect();
            String::from_utf16_lossy(&words)
        } else {
            String::from_utf8_lossy(bytes.split(|b| *b == 0).next().unwrap_or_default()).chars().take(4096).collect()
        };
        if name.is_empty() { None } else { Some(name) }
    }
    /// Convert optional uniform transparency to opacity, rejecting values above 1000.
    fn opacity(&mut self, bytes: Option<&[u8]>) -> Result<f32, String> {
        let Some(bytes) = bytes else { return Ok(1.0) };
        let mut r = Reader::new(bytes);
        r.skip(if self.version >= 1300 { 14 } else { 10 })?;
        let v = r.u16()?;
        if v > 1000 {
            return Err("invalid CDR opacity".into());
        }
        Ok(1.0 - v as f32 / 1000.0)
    }
    /// Resolve one supported object into an editable path, appearance and optional name.
    /// Unsupported geometry returns `None` with a warning; malformed data returns an error.
    fn object(&mut self, c: &Chunk, parent: Affine) -> Result<Option<Node>, String> {
        self.objects_seen += 1;
        let Some(loda) = metadata(c, b"loda").or_else(|| metadata(c, b"lobj")) else {
            self.warn("An object without native geometry was omitted");
            return Ok(None);
        };
        let (kind, args) = arguments(&loda.data)?;
        let Some(coords) = args.get(&30) else {
            self.warn("An object without supported coordinate data was omitted");
            return Ok(None);
        };
        if kind == 5 {
            return self.bitmap_object(c, coords, parent, &args);
        }
        let path = match kind {
            1 => self.rectangle(coords)?,
            2 => self.ellipse(coords)?,
            3 | 0x25 => self.curve(coords, kind == 0x25)?,
            _ => {
                self.warn(&format!("Native CDR object type {kind} is omitted"));
                return Ok(None);
            }
        };
        let local_bounds = path.bounding_box();
        let xf = parent * self.transform(c)?;
        let path = PathData::from_bezpath(&(xf * path));
        let bounds = path.to_bezpath().bounding_box();
        if ![bounds.x0, bounds.y0, bounds.x1, bounds.y1].iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD) {
            return Err("CDR geometry is non-finite or exceeds document coordinate limits".into());
        }
        let mut items = Vec::new();
        if let Some(arg) = args.get(&20) {
            let id = Reader::new(arg).u32()?;
            if id != 0 {
                let n = self.fills.get(&id).map_or(0, |f| match f {
                    Fill::Fountain(f) => f.stops.len(),
                    _ => 0,
                });
                self.add_paint_values(n)?;
                if let Some(fill) = self.fills.get(&id) {
                    let paint = fill.paint(local_bounds, xf)?;
                    if !matches!(paint, Paint::None) {
                        items.push(AppearanceItem::Fill(FillLayer::new(paint)));
                    }
                } else {
                    self.warn("An unresolved CDR fill style was omitted");
                }
            }
        }
        if let Some(arg) = args.get(&10) {
            let id = Reader::new(arg).u32()?;
            if id != 0 {
                let n = self.strokes.get(&id).and_then(|s| s.as_ref()).and_then(|s| s.dash.as_ref()).map_or(0, |d| d.pattern.len());
                self.add_paint_values(n)?;
                if let Some(Some(stroke)) = self.strokes.get(&id) {
                    items.push(AppearanceItem::Stroke(stroke.clone()));
                } else if !self.strokes.contains_key(&id) {
                    self.warn("An unresolved CDR outline style was omitted");
                }
            }
        }
        self.note_properties(&args);
        let mut node = Node::path(self.id(), path, Appearance { items, ..Appearance::default() });
        node.name = self.name(args.get(&1000).copied());
        node.opacity = self.opacity(args.get(&8000).copied())?;
        self.objects_imported += 1;
        Ok(Some(node))
    }
    /// Retain warnings for object appearance properties that are not yet interpreted.
    fn note_properties(&mut self, args: &BTreeMap<u32, &[u8]>) {
        if args.contains_key(&200) {
            self.warn("Inherited CDR object styles are not resolved");
        }
        if args.contains_key(&201) {
            self.warn("Native appearance and transparency overrides in JSON styles are omitted");
        }
        for id in args.keys() {
            if ![10, 20, 30, 100, 200, 201, 1000, 8000, 19130, 40050].contains(id) {
                self.warn(&format!("Unsupported CDR object property {id} was omitted"));
            }
        }
    }
    /// Place an embedded image in its native coordinates and retain its editable crop outline.
    fn bitmap_object(&mut self, c: &Chunk, coords: &[u8], parent: Affine, args: &BTreeMap<u32, &[u8]>) -> Result<Option<Node>, String> {
        let mut r = Reader::new(coords);
        let (x0, y0, x1, y1) = (r.coord()?, r.coord()?, r.coord()?, r.coord()?);
        r.skip(32)?;
        let image_id = r.u32()?;
        r.skip(if (800..900).contains(&self.version) { 12 } else { 20 })?;
        let crop_start = r.pos;
        let crop_count = r.u32()? as usize;
        let Some(bitmap) = self.bitmaps.get(&image_id) else {
            self.warn("An image object with an unavailable CDR bitmap resource was omitted");
            return Ok(None);
        };
        let (width, height, key) = (bitmap.width, bitmap.height, bitmap.key.clone());
        let xf = parent * self.transform(c)?;
        let image_xf = xf * Affine::new([(x1 - x0) / width as f64, 0.0, 0.0, (y1 - y0) / height as f64, x0, y0]);
        let bounds = image_xf.transform_rect_bbox(Rect::new(0.0, 0.0, width as f64, height as f64));
        if x0 == x1 || y0 == y1 || ![bounds.x0, bounds.y0, bounds.x1, bounds.y1].iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD) {
            return Err("CDR bitmap placement is empty or exceeds document coordinate limits".into());
        }
        let image = Node::new(
            self.id(),
            NodeKind::Image(Box::new(ImageObject { key, width, height, xf: image_xf, link: None, placement: Default::default() })),
        );
        let mut node = if crop_count == 0 {
            image
        } else {
            let crop = self.curve(coords.get(crop_start..).ok_or("truncated CDR bitmap crop")?, false)?;
            let crop = xf * crop;
            let bounds = crop.bounding_box();
            if ![bounds.x0, bounds.y0, bounds.x1, bounds.y1].iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD) {
                return Err("CDR bitmap crop exceeds document coordinate limits".into());
            }
            // Corel's bitmap crop uses even-odd coverage, including same-winding holes.
            // Keep its appearance empty: a painted clipping path is rendered underneath the
            // image and would replace transparent pixels with the clipping path's fill.
            let mask = Node::new(
                self.id(),
                NodeKind::Path { path: PathData::from_bezpath(&crop), rule: FillRule::EvenOdd, live: None, clipping: true, guide: false },
            );
            Node::new(self.id(), NodeKind::Group { children: vec![Arc::new(mask), Arc::new(image)], clip: true })
        };
        self.note_properties(args);
        node.name = self.name(args.get(&1000).copied());
        node.opacity = self.opacity(args.get(&8000).copied())?;
        self.objects_imported += 1;
        Ok(Some(node))
    }
    /// Charge geometry before allocation against the cumulative one-million-point limit.
    fn add_points(&mut self, n: usize) -> Result<(), String> {
        self.points = self.points.checked_add(n).ok_or("CDR point count overflow")?;
        if self.points > MAX_POINTS {
            return Err("CDR geometry point limit exceeded".into());
        }
        Ok(())
    }
    /// Charge both style definitions and per-object gradient/dash copies against their limit.
    fn add_paint_values(&mut self, n: usize) -> Result<(), String> {
        self.paint_values = self.paint_values.checked_add(n).ok_or("CDR paint count overflow")?;
        if self.paint_values > 250_000 {
            return Err("CDR paint materialization limit exceeded".into());
        }
        Ok(())
    }
    /// Decode rectangle dimensions in inches, warning when importing treated corners square.
    fn rectangle(&mut self, bytes: &[u8]) -> Result<BezPath, String> {
        self.add_points(16)?;
        let mut r = Reader::new(bytes);
        let (w, h) = if self.version >= 1500 { (r.f64()? / 254000.0, r.f64()? / 254000.0) } else { (r.coord()?, r.coord()?) };
        // Preserve the rectangle itself even where corner treatment isn't yet decoded.
        let corners = if self.version >= 1500 {
            r.skip(24)?;
            let mut any = r.f64()? != 0.0;
            for _ in 0..3 {
                r.skip(16)?;
                any |= r.f64()? != 0.0;
            }
            any
        } else {
            let mut any = r.coord()? != 0.0;
            if self.version >= 900 {
                for _ in 0..3 {
                    any |= r.coord()? != 0.0;
                }
            }
            any
        };
        if corners {
            self.warn("Rounded, scalloped or chamfered rectangle corners are imported square");
        }
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((w, 0.0));
        p.line_to((w, h));
        p.line_to((0.0, h));
        p.close_path();
        Ok(p)
    }
    /// Approximate an ellipse, arc or pie with up to four cubic segments in local inches.
    fn ellipse(&mut self, bytes: &[u8]) -> Result<BezPath, String> {
        self.add_points(16)?;
        let mut r = Reader::new(bytes);
        let w = r.coord()?;
        let h = r.coord()?;
        let a1 = (r.u32()? as i32 as f64 / 1_000_000.0).to_radians();
        let a2 = (r.u32()? as i32 as f64 / 1_000_000.0).to_radians();
        let pie = r.u32()? != 0;
        let tau = std::f64::consts::TAU;
        let start = a1.rem_euclid(tau);
        let sweep = (a2 - a1).rem_euclid(tau);
        let full = sweep.abs() < 1e-10;
        let sweep = if full { tau } else { sweep };
        let steps = (sweep / std::f64::consts::FRAC_PI_2).ceil().clamp(1.0, 4.0) as usize;
        let step = sweep / steps as f64;
        let (cx, cy, rx, ry) = (w / 2.0, h / 2.0, w.abs() / 2.0, h.abs() / 2.0);
        let point = |a: f64| Point::new(cx + rx * a.cos(), cy - ry * a.sin());
        let mut p = BezPath::new();
        p.move_to(point(start));
        for i in 0..steps {
            let a = start + i as f64 * step;
            let b = a + step;
            let k = 4.0 / 3.0 * (step / 4.0).tan();
            let p0 = point(a);
            let p3 = point(b);
            p.curve_to((p0.x - k * rx * a.sin(), p0.y - k * ry * a.cos()), (p3.x + k * rx * b.sin(), p3.y + k * ry * b.cos()), (p3.x, p3.y));
        }
        if full {
            p.close_path();
        } else if pie {
            p.line_to((cx, cy));
            p.close_path();
        }
        Ok(p)
    }
    /// Decode bounded polycurve points and node flags into straight and cubic subpaths.
    fn curve(&mut self, bytes: &[u8], path: bool) -> Result<BezPath, String> {
        let mut r = Reader::new(bytes);
        let count = if path {
            r.skip(4)?;
            let n = r.u16()? as usize + r.u16()? as usize;
            r.skip(16)?;
            n
        } else {
            r.u32()? as usize
        };
        self.add_points(count)?;
        if count > bytes.len().saturating_sub(r.pos) / 9 {
            return Err("truncated CDR curve points".into());
        }
        let mut points = Vec::new();
        for _ in 0..count {
            points.push(Point::new(r.coord()?, r.coord()?));
        }
        let types = r.take(count)?;
        let mut p = BezPath::new();
        let mut controls = Vec::new();
        let mut started = false;
        for (point, typ) in points.into_iter().zip(types) {
            match typ >> 6 {
                0 => {
                    p.move_to(point);
                    controls.clear();
                    started = true;
                }
                1 if started => {
                    p.line_to(point);
                    controls.clear();
                    if typ & 8 != 0 {
                        p.close_path();
                    }
                }
                2 if started => {
                    if let (Some(a), Some(b)) = (controls.first(), controls.get(1)) {
                        p.curve_to(*a, *b, point);
                    } else {
                        return Err("CDR cubic curve is missing its control points".into());
                    }
                    controls.clear();
                    if typ & 8 != 0 {
                        p.close_path();
                    }
                }
                3 if started => {
                    if controls.len() >= 2 {
                        return Err("too many CDR cubic control points".into());
                    }
                    controls.push(point);
                }
                _ => return Err("CDR curve segment precedes its move point".into()),
            }
        }
        if !controls.is_empty() {
            return Err("CDR curve ends with unused control points".into());
        }
        Ok(p)
    }
}
