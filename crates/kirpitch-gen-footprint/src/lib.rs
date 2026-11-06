// SPDX-License-Identifier: GPL-3.0-or-later

//! Generates a KiCad footprint (`.kicad_mod`) from a PartSpec.
//!
//! The geometry is plain deterministic code: pads come from the land pattern of
//! the PartSpec, and the courtyard, fab outline, silkscreen and pin 1 marker are
//! derived from the pads and the package body following the KiCad Library
//! Conventions (KLC). No language model is involved. Milestone 1 writes the
//! KiCad 10 and 9 formats and surface-mount pads only.
//!
//! Conventions covered (ADR 0008), with values taken from KiCad's own
//! official footprints: courtyard 0.25 mm from the pads and body on a 0.01 mm
//! grid, silkscreen 0.12 mm wide and 0.2 mm clear of pads, fab outline 0.1 mm
//! wide with a pin 1 chamfer, text 1 mm high, reference above and value below
//! the courtyard. The rule numbers of the KLC are not cited because they have
//! not been checked against the current KLC text yet.

use kirpitch_core::partspec::{Pad, PadRole, PadShape};
use kirpitch_core::{FabProfile, Finding, PartSpec, has_errors, validate};
use kirpitch_kicad::{KicadTarget, List, list};
use thiserror::Error;

/// Distance from the pads and body to the courtyard.
const COURTYARD_MARGIN: f64 = 0.25;
/// Grid the courtyard lies on.
const COURTYARD_GRID: f64 = 0.01;
/// Line width of the courtyard.
const COURTYARD_WIDTH: f64 = 0.05;
/// Line width of the fab outline.
const FAB_WIDTH: f64 = 0.1;
/// Line width of the silkscreen.
const SILK_WIDTH: f64 = 0.12;
/// Smallest distance between silkscreen and copper.
const SILK_CLEARANCE: f64 = 0.2;
/// Distance of the silkscreen centre line from the fab outline.
const SILK_OFFSET: f64 = 0.11;
/// Height of the pin 1 chamfer on the fab outline, at most.
const CHAMFER_MAX: f64 = 1.0;
/// Rounding ratio used for rounded rectangles when the PartSpec gives none.
const DEFAULT_CORNER_RATIO: f64 = 0.25;
/// Distance of the reference and value text from the courtyard.
const TEXT_DISTANCE: f64 = 0.7;

/// Reasons a footprint cannot be generated.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum GenerateError {
    /// The PartSpec has errors; fix these fields first.
    #[error("the PartSpec has {} error(s); first: {} ({})", count_errors(.0), first_error(.0), first_path(.0))]
    Invalid(Vec<Finding>),
    /// The land pattern has no pad for the first pin.
    #[error("no pad for pin 1; cannot place the pin 1 marker")]
    NoPinOnePad,
}

fn count_errors(findings: &[Finding]) -> usize {
    findings.iter().filter(|f| f.blocks()).count()
}

fn first_error(findings: &[Finding]) -> String {
    findings
        .iter()
        .find(|f| f.blocks())
        .map(|f| f.message.clone())
        .unwrap_or_default()
}

fn first_path(findings: &[Finding]) -> String {
    findings
        .iter()
        .find(|f| f.blocks())
        .map(|f| f.path.clone())
        .unwrap_or_default()
}

/// Settings of the generator.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// KiCad version to write for.
    pub target: KicadTarget,
    /// Limits used to validate the PartSpec first.
    pub fab: FabProfile,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            target: KicadTarget::LATEST,
            fab: FabProfile::generic(),
        }
    }
}

/// The file name of the footprint: the package name plus `.kicad_mod`.
pub fn file_name(spec: &PartSpec) -> String {
    format!("{}.kicad_mod", spec.package.name)
}

/// Generates the text of the `.kicad_mod` file.
///
/// The PartSpec is validated first; any error finding stops generation, so
/// problems are reported against PartSpec fields and never against the file.
/// The same PartSpec always gives the same bytes.
pub fn generate(spec: &PartSpec, options: &Options) -> Result<String, GenerateError> {
    let findings = validate(spec, &options.fab);
    if has_errors(&findings) {
        return Err(GenerateError::Invalid(findings));
    }
    let pads = &spec.land_pattern.pads;
    let pin_one = pin_one_pad(spec).ok_or(GenerateError::NoPinOnePad)?;

    let body_half_x = spec.package.body.width.get() / 2.0;
    let body_half_y = spec.package.body.length.get() / 2.0;
    let courtyard = courtyard(spec, body_half_x, body_half_y);
    let ref_y = courtyard.y_min - TEXT_DISTANCE;
    let value_y = courtyard.y_max + TEXT_DISTANCE;

    let mut fp = list("footprint")
        .string(&spec.package.name)
        .child(list("version").atom(&options.target.footprint_format_version().to_string()))
        .child(list("generator").string("kirpitch"))
        .child(list("layer").string("F.Cu"))
        .child(list("descr").string(&description(spec)))
        .child(list("tags").string(&tags(spec)))
        .child(text_property("Reference", "REF**", ref_y, "F.SilkS"))
        .child(text_property("Value", &spec.package.name, value_y, "F.Fab"))
        .child(list("attr").atom("smd"))
        .child_opt(
            options
                .target
                .footprint_has_pad_jumper_setting()
                .then(|| list("duplicate_pad_numbers_are_jumpers").atom("no")),
        );

    for (a, b) in silk_segments(pads, body_half_x + SILK_OFFSET, body_half_y + SILK_OFFSET) {
        fp = fp.child(line(a, b, SILK_WIDTH, "F.SilkS"));
    }
    fp = fp.child(pin_one_marker(pin_one));
    fp = fp.child(courtyard_rect(&courtyard));
    fp = fp.child(fab_outline(body_half_x, body_half_y, pin_one));
    fp = fp.child(
        list("fp_text")
            .atom("user")
            .string("${REFERENCE}")
            .child(
                list("at")
                    .num(0.0)
                    .num(0.0)
                    .num(if body_half_y > body_half_x { 90.0 } else { 0.0 }),
            )
            .child(list("layer").string("F.Fab"))
            .child(effects()),
    );
    for pad in pads {
        fp = fp.child(pad_node(pad));
    }
    if options.target.has_embedded_fonts_setting() {
        fp = fp.child(list("embedded_fonts").atom("no"));
    }
    Ok(fp.write())
}

fn description(spec: &PartSpec) -> String {
    match &spec.metadata.datasheet_url {
        Some(url) => format!("{}, {}", spec.metadata.description, url),
        None => spec.metadata.description.clone(),
    }
}

fn tags(spec: &PartSpec) -> String {
    let mut words = vec![spec.package.family.clone()];
    words.extend(spec.metadata.keywords.iter().cloned());
    words.join(" ")
}

/// The pad of the first pin; its centre decides where pin 1 is.
fn pin_one_pad(spec: &PartSpec) -> Option<&Pad> {
    let number = &spec.pins.first()?.number;
    spec.land_pattern
        .pads
        .iter()
        .find(|p| p.role == PadRole::Signal && &p.number == number)
}

#[derive(Debug, Clone, Copy)]
struct Box2 {
    x_min: f64,
    y_min: f64,
    x_max: f64,
    y_max: f64,
}

fn courtyard(spec: &PartSpec, body_half_x: f64, body_half_y: f64) -> Box2 {
    if let Some(r) = &spec.land_pattern.courtyard {
        return Box2 {
            x_min: r.x_min.get(),
            y_min: r.y_min.get(),
            x_max: r.x_max.get(),
            y_max: r.y_max.get(),
        };
    }
    let mut b = Box2 {
        x_min: -body_half_x,
        y_min: -body_half_y,
        x_max: body_half_x,
        y_max: body_half_y,
    };
    for pad in &spec.land_pattern.pads {
        let (x0, y0, x1, y1) = pad.bounds();
        b.x_min = b.x_min.min(x0);
        b.y_min = b.y_min.min(y0);
        b.x_max = b.x_max.max(x1);
        b.y_max = b.y_max.max(y1);
    }
    // Round outwards to the grid; the small offset absorbs float noise.
    let down = |v: f64| ((v - COURTYARD_MARGIN) / COURTYARD_GRID + 1e-6).floor() * COURTYARD_GRID;
    let up = |v: f64| ((v + COURTYARD_MARGIN) / COURTYARD_GRID - 1e-6).ceil() * COURTYARD_GRID;
    Box2 {
        x_min: down(b.x_min),
        y_min: down(b.y_min),
        x_max: up(b.x_max),
        y_max: up(b.y_max),
    }
}

fn effects() -> List {
    list("effects").child(
        list("font")
            .child(list("size").num(1.0).num(1.0))
            .child(list("thickness").num(0.15)),
    )
}

fn text_property(name: &str, value: &str, y: f64, layer: &str) -> List {
    list("property")
        .string(name)
        .string(value)
        .child(list("at").num(0.0).num(y).num(0.0))
        .child(list("layer").string(layer))
        .child(effects())
}

fn stroke(width: f64) -> List {
    list("stroke")
        .child(list("width").num(width))
        .child(list("type").atom("solid"))
}

fn line(a: (f64, f64), b: (f64, f64), width: f64, layer: &str) -> List {
    list("fp_line")
        .child(list("start").num(a.0).num(a.1))
        .child(list("end").num(b.0).num(b.1))
        .child(stroke(width))
        .child(list("layer").string(layer))
}

fn courtyard_rect(c: &Box2) -> List {
    list("fp_rect")
        .child(list("start").num(c.x_min).num(c.y_min))
        .child(list("end").num(c.x_max).num(c.y_max))
        .child(stroke(COURTYARD_WIDTH))
        .child(list("fill").atom("no"))
        .child(list("layer").string("F.CrtYd"))
}

/// Body outline on the fab layer with the corner nearest to pin 1 cut off.
fn fab_outline(half_x: f64, half_y: f64, pin_one: &Pad) -> List {
    let sx = if pin_one.x.get() > 0.0 { 1.0 } else { -1.0 };
    let sy = if pin_one.y.get() > 0.0 { 1.0 } else { -1.0 };
    let c = CHAMFER_MAX.min(half_x * 2.0 / 4.0).min(half_y * 2.0 / 4.0);
    let points = [
        (sx * (half_x - c), sy * half_y),
        (-sx * half_x, sy * half_y),
        (-sx * half_x, -sy * half_y),
        (sx * half_x, -sy * half_y),
        (sx * half_x, sy * (half_y - c)),
    ];
    let mut pts = list("pts");
    for (x, y) in points {
        pts = pts.child(list("xy").num(x).num(y));
    }
    list("fp_poly")
        .child(pts)
        .child(stroke(FAB_WIDTH))
        .child(list("fill").atom("no"))
        .child(list("layer").string("F.Fab"))
}

/// A filled triangle just outside pin 1, pointing at the pad.
fn pin_one_marker(pad: &Pad) -> List {
    let outward = if pad.y.get() > 0.0 { 1.0 } else { -1.0 };
    let (_, y0, _, y1) = pad.bounds();
    let edge = if outward > 0.0 { y1 } else { y0 };
    let apex_y = edge + outward * (SILK_CLEARANCE + SILK_WIDTH / 2.0);
    let base_y = apex_y + outward * 0.33;
    let x = pad.x.get();
    list("fp_poly")
        .child(
            list("pts")
                .child(list("xy").num(x).num(apex_y))
                .child(list("xy").num(x - 0.24).num(base_y))
                .child(list("xy").num(x + 0.24).num(base_y)),
        )
        .child(stroke(SILK_WIDTH))
        .child(list("fill").atom("yes"))
        .child(list("layer").string("F.SilkS"))
}

/// The parts of the silkscreen rectangle that keep their distance from pads.
fn silk_segments(pads: &[Pad], half_x: f64, half_y: f64) -> Vec<((f64, f64), (f64, f64))> {
    let grow = SILK_CLEARANCE + SILK_WIDTH / 2.0;
    let blocks: Vec<Box2> = pads
        .iter()
        .map(|p| {
            let (x0, y0, x1, y1) = p.bounds();
            Box2 {
                x_min: x0 - grow,
                y_min: y0 - grow,
                x_max: x1 + grow,
                y_max: y1 + grow,
            }
        })
        .collect();
    let eps = 1e-9;
    let mut out = Vec::new();

    // Horizontal edges (top, then bottom), then vertical edges (left, right).
    for y in [-half_y, half_y] {
        let cuts: Vec<(f64, f64)> = blocks
            .iter()
            .filter(|b| b.y_min + eps < y && y < b.y_max - eps)
            .map(|b| (b.x_min, b.x_max))
            .collect();
        for (a, b) in free_intervals(-half_x, half_x, cuts) {
            out.push(((a, y), (b, y)));
        }
    }
    for x in [-half_x, half_x] {
        let cuts: Vec<(f64, f64)> = blocks
            .iter()
            .filter(|b| b.x_min + eps < x && x < b.x_max - eps)
            .map(|b| (b.y_min, b.y_max))
            .collect();
        for (a, b) in free_intervals(-half_y, half_y, cuts) {
            out.push(((x, a), (x, b)));
        }
    }
    out
}

/// The parts of `[from, to]` not covered by any of the `cuts`.
fn free_intervals(from: f64, to: f64, mut cuts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut free = Vec::new();
    let mut at = from;
    for (c0, c1) in cuts {
        if c0 > at + 1e-9 {
            free.push((at, c0.min(to)));
        }
        at = at.max(c1);
        if at >= to {
            break;
        }
    }
    if at < to - 1e-9 {
        free.push((at, to));
    }
    // Pieces in the middle of an edge, between two pads, are not drawn: only
    // pieces that reach an end of the edge (corner stubs, or the whole edge).
    free.retain(|(a, b)| b - a > 1e-9 && (*a <= from + 1e-9 || *b >= to - 1e-9));
    free
}

fn pad_node(pad: &Pad) -> List {
    let shape = match pad.shape {
        PadShape::Rect => "rect",
        PadShape::RoundRect => "roundrect",
        PadShape::Oval => "oval",
        PadShape::Circle => "circle",
    };
    let mut at = list("at").num(pad.x.get()).num(pad.y.get());
    if pad.rotation_deg != 0.0 {
        at = at.num(pad.rotation_deg);
    }
    let mut layers = list("layers").string("F.Cu").string("F.Mask");
    if pad.paste {
        layers = layers.string("F.Paste");
    }
    let mut node = list("pad")
        .string(&pad.number)
        .atom("smd")
        .atom(shape)
        .child(at)
        .child(list("size").num(pad.size_x.get()).num(pad.size_y.get()))
        .child(layers);
    if pad.shape == PadShape::RoundRect {
        node = node
            .child(list("roundrect_rratio").num(pad.corner_ratio.unwrap_or(DEFAULT_CORNER_RATIO)));
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_intervals_subtract_cuts() {
        assert_eq!(free_intervals(0.0, 10.0, vec![]), vec![(0.0, 10.0)]);
        assert_eq!(
            free_intervals(0.0, 10.0, vec![(2.0, 4.0), (3.0, 6.0)]),
            vec![(0.0, 2.0), (6.0, 10.0)]
        );
        // A piece between two cuts is dropped.
        assert_eq!(
            free_intervals(0.0, 10.0, vec![(1.0, 2.0), (3.0, 4.0)]),
            vec![(0.0, 1.0), (4.0, 10.0)]
        );
        // A piece between two cuts is dropped.
        assert_eq!(
            free_intervals(0.0, 10.0, vec![(1.0, 2.0), (3.0, 4.0)]),
            vec![(0.0, 1.0), (4.0, 10.0)]
        );
        assert_eq!(free_intervals(0.0, 10.0, vec![(-5.0, 20.0)]), vec![]);
        assert_eq!(
            free_intervals(0.0, 10.0, vec![(-5.0, 3.0)]),
            vec![(3.0, 10.0)]
        );
    }
}
