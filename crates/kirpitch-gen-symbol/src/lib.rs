// SPDX-License-Identifier: GPL-3.0-or-later

//! Generates a KiCad symbol library (`.kicad_sym`) with one symbol from a
//! PartSpec.
//!
//! The layout is deterministic code: pins are placed by electrical type, in the
//! order of the PartSpec, on a 2.54 mm (100 mil) grid. Inputs go on the left,
//! outputs and bidirectional pins on the right, positive supplies on top and grounds at the bottom.
//! Pins that share a name and are power pins (for example several GND pins) are
//! stacked on one position, with all but the first hidden. Milestone 1 writes
//! the KiCad 10, 9 and 8 formats and single-unit symbols only; multi-unit symbols come
//! later.

use kirpitch_core::partspec::Pin;
use kirpitch_core::{ElectricalType, FabProfile, Finding, PartSpec, has_errors, validate};
use kirpitch_kicad::{KicadTarget, List, list};
use thiserror::Error;

/// Distance between pins.
const GRID: f64 = 2.54;
/// Length of every pin.
const PIN_LENGTH: f64 = 2.54;
/// Distance between the body and the pin name text.
const NAME_OFFSET: f64 = 1.016;
/// Text height.
const FONT: f64 = 1.27;
/// Estimated width of one character at `FONT` size, with some air.
const CHAR_WIDTH: f64 = 0.85;
/// Smallest body width.
const MIN_WIDTH: f64 = 10.16;
/// Line width of the body outline.
const BODY_STROKE: f64 = 0.254;

/// Reasons a symbol cannot be generated.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum GenerateError {
    /// The PartSpec has errors; fix these fields first.
    #[error("the PartSpec has {} error(s); first: {} ({})", count_errors(.0), first_error(.0), first_path(.0))]
    Invalid(Vec<Finding>),
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

/// The file name of the library: the part number plus `.kicad_sym`, with
/// characters that are not allowed in file names (`/ \ : * ? " < > |`)
/// replaced by `_`.
pub fn file_name(spec: &PartSpec) -> String {
    let safe: String = spec
        .metadata
        .mpn
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    format!("{safe}.kicad_sym")
}

/// Generates the text of the `.kicad_sym` file.
///
/// The PartSpec is validated first; any error finding stops generation. The
/// same PartSpec always gives the same bytes.
pub fn generate(spec: &PartSpec, options: &Options) -> Result<String, GenerateError> {
    let findings = validate(spec, &options.fab);
    if has_errors(&findings) {
        return Err(GenerateError::Invalid(findings));
    }
    let target = options.target;
    let layout = Layout::new(&spec.pins);
    let name = &spec.metadata.mpn;

    let ref_y = layout.half_height + 1.27 + if layout.top.is_empty() { 0.0 } else { GRID };
    let value_y = -(layout.half_height + 1.27) - if layout.bottom.is_empty() { 0.0 } else { GRID };
    let x_left = -layout.half_width;

    let mut body = list("symbol").string(&format!("{name}_1_1")).child(
        list("rectangle")
            .child(
                list("start")
                    .num(-layout.half_width)
                    .num(layout.half_height),
            )
            .child(list("end").num(layout.half_width).num(-layout.half_height))
            .child(
                list("stroke")
                    .child(list("width").num(BODY_STROKE))
                    .child(list("type").atom("default")),
            )
            .child(list("fill").child(list("type").atom("background"))),
    );
    // KiCad itself stores the pins of a unit in a fixed order, so the file is
    // written that way too (see the canonical-layout test): by pin number, or,
    // in KiCad 9, from left to right and then from top to bottom.
    let mut placed = layout.placed();
    if target.symbol_pins_sorted_by_position() {
        placed.sort_by(|a, b| {
            a.x.total_cmp(&b.x)
                .then(b.y.total_cmp(&a.y))
                .then_with(|| natural_cmp(&a.pin.number, &b.pin.number))
        });
    } else {
        placed.sort_by(|a, b| natural_cmp(&a.pin.number, &b.pin.number));
    }
    for p in &placed {
        body = body.child(pin_node(p, target));
    }

    // KiCad stores "no datasheet" as an empty string and rewrites "~" to it.
    let datasheet = spec.metadata.datasheet_url.as_deref().unwrap_or("");
    let hidden = |name: &str, value: &str| property(target, name, value, (0.0, 0.0), true, false);
    let mut symbol = list("symbol")
        .string(name)
        .child(list("pin_names").child(list("offset").num(NAME_OFFSET)))
        .child(list("exclude_from_sim").atom("no"))
        .child(list("in_bom").atom("yes"))
        .child(list("on_board").atom("yes"))
        .child_opt(
            target
                .symbol_has_pos_files_and_jumper_settings()
                .then(|| list("in_pos_files").atom("yes")),
        )
        .child_opt(
            target
                .symbol_has_pos_files_and_jumper_settings()
                .then(|| list("duplicate_pin_numbers_are_jumpers").atom("no")),
        )
        .child(property(
            target,
            "Reference",
            "U",
            (x_left, ref_y),
            false,
            true,
        ))
        .child(property(
            target,
            "Value",
            name,
            (x_left, value_y),
            false,
            true,
        ))
        .child(hidden("Footprint", ""))
        .child(hidden("Datasheet", datasheet))
        .child(hidden("Description", &spec.metadata.description))
        .child(hidden("MPN", name))
        .child(hidden("Manufacturer", &spec.metadata.manufacturer));
    if !spec.metadata.keywords.is_empty() {
        symbol = symbol.child(hidden("ki_keywords", &spec.metadata.keywords.join(" ")));
    }
    symbol = symbol
        .child(hidden(
            "ki_fp_filters",
            &format!("{}*", spec.package.family),
        ))
        .child(body)
        .child_opt(
            target
                .has_embedded_fonts_setting()
                .then(|| list("embedded_fonts").atom("no")),
        );

    Ok(list("kicad_symbol_lib")
        .child(list("version").atom(&options.target.symbol_format_version().to_string()))
        .child(list("generator").string("kirpitch"))
        .child(list("generator_version").string(options.target.generator_version()))
        .child(symbol)
        .write())
}

fn effects(justify_left: bool, hide: bool) -> List {
    let mut e = list("effects").child(list("font").child(list("size").num(FONT).num(FONT)));
    if justify_left {
        e = e.child(list("justify").atom("left"));
    }
    if hide {
        e = e.child(list("hide").atom("yes"));
    }
    e
}

/// A field of the symbol. Newer KiCad versions write `show_name`,
/// `do_not_autoplace` and a `hide` setting on the property itself; older ones
/// put `hide` inside the text effects.
fn property(
    target: KicadTarget,
    name: &str,
    value: &str,
    at: (f64, f64),
    hide: bool,
    justify_left: bool,
) -> List {
    let mut p = list("property")
        .string(name)
        .string(value)
        .child(list("at").num(at.0).num(at.1).num(0.0));
    if target.symbol_property_has_show_name() {
        p = p
            .child(list("show_name").atom("no"))
            .child(list("do_not_autoplace").atom("no"));
        if hide {
            p = p.child(list("hide").atom("yes"));
        }
        p.child(effects(justify_left, false))
    } else {
        p.child(effects(justify_left, hide))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// A pin with its place on the symbol.
struct Placed<'a> {
    pin: &'a Pin,
    x: f64,
    y: f64,
    /// Direction the pin points to, in degrees (towards the body).
    angle: f64,
    hidden: bool,
}

fn pin_node(p: &Placed<'_>, target: KicadTarget) -> List {
    let mut pin = list("pin")
        .atom(p.pin.electrical_type.as_str())
        .atom("line")
        .child(list("at").num(p.x).num(p.y).num(p.angle))
        .child(list("length").num(PIN_LENGTH));
    if p.hidden {
        pin = if target.symbol_pin_hide_is_bare_word() {
            pin.atom("hide")
        } else {
            pin.child(list("hide").atom("yes"))
        };
    }
    pin.child(
        list("name")
            .string(&p.pin.name)
            .child(effects(false, false)),
    )
    .child(
        list("number")
            .string(&p.pin.number)
            .child(effects(false, false)),
    )
}

/// Which side of the symbol a pin belongs on.
fn side_of(pin: &Pin) -> Side {
    match pin.electrical_type {
        ElectricalType::PowerIn if kirpitch_core::names::looks_like_ground(&pin.name) => {
            Side::Bottom
        }
        ElectricalType::PowerIn => Side::Top,
        ElectricalType::PowerOut
        | ElectricalType::Output
        | ElectricalType::TriState
        | ElectricalType::Bidirectional
        | ElectricalType::OpenCollector
        | ElectricalType::OpenEmitter => Side::Right,
        _ => Side::Left,
    }
}

/// Number of visible characters; the overbar markup `~{...}` is not counted.
fn visible_chars(name: &str) -> usize {
    name.replace("~{", "").replace('}', "").chars().count()
}

/// A slot on one side: a pin, or an empty gap between groups.
#[derive(Clone, Copy)]
enum Slot<'a> {
    Pin(&'a Pin),
    Gap,
}

struct Layout<'a> {
    left: Vec<Slot<'a>>,
    right: Vec<Slot<'a>>,
    top: Vec<&'a Pin>,
    bottom: Vec<&'a Pin>,
    half_width: f64,
    half_height: f64,
}

impl<'a> Layout<'a> {
    fn new(pins: &'a [Pin]) -> Layout<'a> {
        // Pins keep the PartSpec order, except that pins of one group are kept
        // together (the group order is the order of first appearance).
        let mut order: Vec<&Pin> = pins.iter().collect();
        let mut groups: Vec<Option<&str>> = Vec::new();
        for p in &order {
            let g = p.group.as_deref();
            if !groups.contains(&g) {
                groups.push(g);
            }
        }
        order.sort_by_key(|p| groups.iter().position(|g| *g == p.group.as_deref()));

        let mut left: Vec<Slot> = Vec::new();
        let mut right: Vec<Slot> = Vec::new();
        let mut top: Vec<&Pin> = Vec::new();
        let mut bottom: Vec<&Pin> = Vec::new();
        let mut last_group: [Option<Option<&str>>; 2] = [None, None];
        for pin in order {
            let g = pin.group.as_deref();
            match side_of(pin) {
                Side::Left => push_slot(&mut left, &mut last_group[0], pin, g),
                Side::Right => push_slot(&mut right, &mut last_group[1], pin, g),
                Side::Top => top.push(pin),
                Side::Bottom => bottom.push(pin),
            }
        }

        // Stacked power pins take one position each, so count positions.
        let top_cols = stacked_positions(&top);
        let bottom_cols = stacked_positions(&bottom);

        let name_len = |slots: &[Slot]| {
            slots
                .iter()
                .filter_map(|s| match s {
                    Slot::Pin(p) => Some(visible_chars(&p.name)),
                    Slot::Gap => None,
                })
                .max()
                .unwrap_or(0)
        };
        let side_len = |pins: &[&Pin]| {
            pins.iter()
                .map(|p| visible_chars(&p.name))
                .max()
                .unwrap_or(0)
        };

        let text_width =
            (name_len(&left) + name_len(&right)) as f64 * CHAR_WIDTH + 2.0 * NAME_OFFSET + 1.27;
        let column_width = (top_cols.max(bottom_cols) as f64 + 1.0) * GRID;
        let width = round_up(text_width.max(column_width).max(MIN_WIDTH), 2.0 * GRID);

        let rows = left.len().max(right.len()).max(1);
        let y_top = ((rows - 1) / 2) as f64 * GRID;
        let rows_half = y_top.max((rows - 1) as f64 * GRID - y_top) + GRID;
        let vertical_text = |n: usize| {
            if n == 0 {
                0.0
            } else {
                n as f64 * CHAR_WIDTH + NAME_OFFSET
            }
        };
        let names_height = vertical_text(side_len(&top)) + vertical_text(side_len(&bottom)) + GRID;
        let half_height = rows_half.max(round_up(names_height / 2.0, GRID));

        Layout {
            left,
            right,
            top,
            bottom,
            half_width: width / 2.0,
            half_height,
        }
    }

    fn placed(&self) -> Vec<Placed<'a>> {
        let mut out = Vec::new();
        let edge_x = self.half_width + PIN_LENGTH;
        let edge_y = self.half_height + PIN_LENGTH;
        for (slots, x, angle) in [(&self.left, -edge_x, 0.0), (&self.right, edge_x, 180.0)] {
            let rows = self.left.len().max(self.right.len()).max(1);
            let y_top = ((rows - 1) / 2) as f64 * GRID;
            for (i, slot) in slots.iter().enumerate() {
                if let Slot::Pin(pin) = slot {
                    out.push(Placed {
                        pin,
                        x,
                        y: y_top - i as f64 * GRID,
                        angle,
                        hidden: false,
                    });
                }
            }
        }
        place_row(&self.top, edge_y, 270.0, &mut out);
        place_row(&self.bottom, -edge_y, 90.0, &mut out);
        out
    }
}

fn push_slot<'a>(
    side: &mut Vec<Slot<'a>>,
    last_group: &mut Option<Option<&'a str>>,
    pin: &'a Pin,
    group: Option<&'a str>,
) {
    if let Some(prev) = *last_group
        && prev != group
    {
        side.push(Slot::Gap);
    }
    *last_group = Some(group);
    side.push(Slot::Pin(pin));
}

/// Number of positions a row needs when equal-named power pins share one.
fn stacked_positions(pins: &[&Pin]) -> usize {
    let mut names: Vec<&str> = Vec::new();
    for p in pins {
        if !names.contains(&p.name.as_str()) {
            names.push(p.name.as_str());
        }
    }
    names.len()
}

/// Places a row of pins along the top or bottom edge, centred on x = 0, with
/// equal-named power pins stacked on one position (all but the first hidden).
fn place_row<'a>(pins: &[&'a Pin], y: f64, angle: f64, out: &mut Vec<Placed<'a>>) {
    let mut columns: Vec<&str> = Vec::new();
    for p in pins {
        if !columns.contains(&p.name.as_str()) {
            columns.push(p.name.as_str());
        }
    }
    let first_x = -(((columns.len().saturating_sub(1)) / 2) as f64) * GRID;
    let mut seen: Vec<&str> = Vec::new();
    for pin in pins {
        let col = columns
            .iter()
            .position(|n| *n == pin.name.as_str())
            .unwrap_or(0);
        let hidden = seen.contains(&pin.name.as_str());
        seen.push(pin.name.as_str());
        out.push(Placed {
            pin,
            x: first_x + col as f64 * GRID,
            y,
            angle,
            hidden,
        });
    }
}

/// Compares pin numbers the way people expect: digits as numbers, so `2` comes
/// before `10`, and `A2` before `A10`.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn chunks(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((d, text)) if *d == digit => text.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    }
    let (ca, cb) = (chunks(a), chunks(b));
    for (x, y) in ca.iter().zip(cb.iter()) {
        let ord = if x.0 && y.0 {
            let (nx, ny) = (x.1.trim_start_matches('0'), y.1.trim_start_matches('0'));
            nx.len().cmp(&ny.len()).then_with(|| nx.cmp(ny))
        } else {
            x.1.cmp(&y.1)
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    ca.len().cmp(&cb.len()).then_with(|| a.cmp(b))
}

fn round_up(value: f64, step: f64) -> f64 {
    (value / step - 1e-9).ceil() * step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overbar_is_not_counted() {
        assert_eq!(visible_chars("~{CS}"), 2);
        assert_eq!(visible_chars("CLK"), 3);
    }

    #[test]
    fn pin_numbers_sort_naturally() {
        let mut v = vec!["10", "2", "A10", "A2", "1", "EP"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["1", "2", "10", "A2", "A10", "EP"]);
    }

    #[test]
    fn rounding_up() {
        assert_eq!(round_up(10.16, 5.08), 10.16);
        assert_eq!(round_up(10.2, 5.08), 15.24);
    }
}
