// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks of generated symbols against the KiCad Library Conventions (KLC).
//!
//! Like the footprint checks (see [`crate::klc`]) these follow the official
//! KiCad libraries and do not cite rule numbers, because the KLC web site could
//! not be read when this was written (it refused automated access).

use crate::klc::{KlcFinding, child, number_of, string_of};
use crate::sexpr::{List, Node, ParseError, parse};

/// Grid for pin ends and body corners: 50 mil. Parts with 100 mil pins sit on
/// a 100 mil grid, which is a multiple.
const SYMBOL_GRID: f64 = 1.27;
/// Shortest usual pin.
const MIN_PIN_LENGTH: f64 = 2.54;
/// Text size of pin names, numbers and fields.
const SYMBOL_TEXT: f64 = 1.27;

fn on_grid(value: f64, grid: f64) -> bool {
    ((value / grid).round() * grid - value).abs() < 1e-6
}

struct SymbolPin<'a> {
    kind: &'a str,
    x: f64,
    y: f64,
    angle: f64,
    length: f64,
    name: String,
    number: String,
    text_sizes: Vec<f64>,
}

fn symbol_pins(unit: &List) -> Vec<SymbolPin<'_>> {
    unit.items
        .iter()
        .filter_map(|n| match n {
            Node::List(l) if l.name() == Some("pin") => Some(l),
            _ => None,
        })
        .filter_map(|p| {
            let at = child(p, "at")?;
            let mut sizes = Vec::new();
            for field in ["name", "number"] {
                if let Some(size) = child(p, field)
                    .and_then(|f| child(f, "effects"))
                    .and_then(|e| child(e, "font"))
                    .and_then(|f| child(f, "size"))
                    .and_then(|s| number_of(s, 1))
                {
                    sizes.push(size);
                }
            }
            Some(SymbolPin {
                kind: string_of(p, 1)?,
                x: number_of(at, 1)?,
                y: number_of(at, 2)?,
                angle: number_of(at, 3)?,
                length: child(p, "length").and_then(|l| number_of(l, 1))?,
                name: child(p, "name").and_then(|l| string_of(l, 1))?.to_string(),
                number: child(p, "number")
                    .and_then(|l| string_of(l, 1))?
                    .to_string(),
                text_sizes: sizes,
            })
        })
        .collect()
}

fn font_size(item: &List) -> Option<f64> {
    child(item, "effects")
        .and_then(|e| child(e, "font"))
        .and_then(|f| child(f, "size"))
        .and_then(|s| number_of(s, 1))
}

/// Checks the text of a `.kicad_sym` file with one symbol. An `Err` means it
/// cannot be parsed.
pub fn check_symbol(text: &str) -> Result<Vec<KlcFinding>, ParseError> {
    let lib = parse(text)?;
    let mut found = Vec::new();
    let mut add = |check: &'static str, message: String| found.push(KlcFinding { check, message });

    let Some(symbol) = lib.items.iter().find_map(|n| match n {
        Node::List(l) if l.name() == Some("symbol") => Some(l),
        _ => None,
    }) else {
        add("symbol_present", "the library has no symbol".into());
        return Ok(found);
    };
    let name = string_of(symbol, 1).unwrap_or("").to_string();

    let property = |wanted: &str| {
        symbol.items.iter().find_map(|n| match n {
            Node::List(l) if l.name() == Some("property") && string_of(l, 1) == Some(wanted) => {
                Some(l)
            }
            _ => None,
        })
    };
    for field in [
        "Reference",
        "Value",
        "Footprint",
        "Datasheet",
        "Description",
    ] {
        if property(field).is_none() {
            add("symbol_fields", format!("the {field} field is missing"));
        }
    }
    if let Some(v) = property("Value")
        && string_of(v, 2) != Some(name.as_str())
    {
        add(
            "symbol_value",
            "the Value field must equal the symbol name".into(),
        );
    }
    if property("Reference").is_some_and(|r| string_of(r, 2).is_none_or(str::is_empty)) {
        add("symbol_reference", "the Reference field is empty".into());
    }
    for field in ["Reference", "Value"] {
        if let Some(p) = property(field)
            && font_size(p).is_none_or(|s| (s - SYMBOL_TEXT).abs() > 1e-9)
        {
            add(
                "symbol_text_size",
                format!("the {field} text must be {SYMBOL_TEXT} mm high"),
            );
        }
    }

    // The drawing parts are named symbol_unit_style.
    let units: Vec<&List> = symbol
        .items
        .iter()
        .filter_map(|n| match n {
            Node::List(l) if l.name() == Some("symbol") => Some(l),
            _ => None,
        })
        .collect();
    let mut all_pins: Vec<SymbolPin> = Vec::new();
    let mut bodies: Vec<(f64, f64, f64, f64)> = Vec::new();
    for unit in &units {
        all_pins.extend(symbol_pins(unit));
        for n in &unit.items {
            let Node::List(r) = n else { continue };
            if r.name() != Some("rectangle") {
                continue;
            }
            let corner =
                |key: &str| child(r, key).and_then(|c| Some((number_of(c, 1)?, number_of(c, 2)?)));
            if let (Some(a), Some(b)) = (corner("start"), corner("end")) {
                bodies.push((a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)));
            }
            let width = child(r, "stroke")
                .and_then(|s| child(s, "width"))
                .and_then(|w| number_of(w, 1));
            if width.is_none_or(|w| w < 0.254 - 1e-9) {
                add(
                    "symbol_body_stroke",
                    "the body outline must be at least 0.254 mm wide".into(),
                );
            }
            let fill = child(r, "fill")
                .and_then(|f| child(f, "type"))
                .and_then(|t| string_of(t, 1));
            if fill != Some("background") {
                add(
                    "symbol_body_fill",
                    "the body must be filled with the background colour".into(),
                );
            }
        }
    }
    if all_pins.is_empty() {
        add("symbol_pins_present", "the symbol has no pins".into());
    }
    if bodies.is_empty() {
        add(
            "symbol_body_present",
            "the symbol has no body rectangle".into(),
        );
    }
    if let Some(&(x0, y0, x1, y1)) = bodies.first() {
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        if cx.abs() > SYMBOL_GRID + 1e-6 || cy.abs() > SYMBOL_GRID + 1e-6 {
            add(
                "symbol_origin",
                "the origin must be at the centre of the symbol".into(),
            );
        }
        for (edge, v) in [("left", x0), ("top", y0), ("right", x1), ("bottom", y1)] {
            if !on_grid(v, SYMBOL_GRID) {
                add(
                    "symbol_body_grid",
                    format!("the {edge} edge of the body is not on the 1.27 mm grid"),
                );
            }
        }
    }

    let mut numbers: Vec<&str> = Vec::new();
    for p in &all_pins {
        if !on_grid(p.x, SYMBOL_GRID) || !on_grid(p.y, SYMBOL_GRID) {
            add(
                "symbol_pin_grid",
                format!("pin {} is not on the 1.27 mm grid", p.number),
            );
        }
        if p.length < MIN_PIN_LENGTH - 1e-9 {
            add(
                "symbol_pin_length",
                format!("pin {} is shorter than {MIN_PIN_LENGTH} mm", p.number),
            );
        }
        if p.text_sizes.iter().any(|s| (s - SYMBOL_TEXT).abs() > 1e-9) {
            add(
                "symbol_text_size",
                format!(
                    "the name and number of pin {} must be {SYMBOL_TEXT} mm high",
                    p.number
                ),
            );
        }
        if p.name.trim() != p.name || p.name.chars().any(char::is_whitespace) {
            add(
                "symbol_pin_name",
                format!("the name of pin {} has spaces", p.number),
            );
        }
        if numbers.contains(&p.number.as_str()) {
            add(
                "symbol_pin_number",
                format!("pin number {} is used twice", p.number),
            );
        }
        numbers.push(&p.number);
        // The inner end of the pin must touch the body outline.
        if let Some(&(x0, y0, x1, y1)) = bodies.first() {
            let rad = p.angle.to_radians();
            let (ix, iy) = (p.x + p.length * rad.cos(), p.y + p.length * rad.sin());
            let on_side = ((ix - x0).abs() < 1e-6 || (ix - x1).abs() < 1e-6)
                && iy >= y0 - 1e-6
                && iy <= y1 + 1e-6;
            let on_end = ((iy - y0).abs() < 1e-6 || (iy - y1).abs() < 1e-6)
                && ix >= x0 - 1e-6
                && ix <= x1 + 1e-6;
            if !(on_side || on_end) {
                add(
                    "symbol_pin_body",
                    format!("pin {} does not touch the body outline", p.number),
                );
            }
        }
    }
    // Pins at the same place are only allowed as stacked power pins of one name.
    for (i, a) in all_pins.iter().enumerate() {
        for b in &all_pins[i + 1..] {
            if (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6 {
                let stacked = a.kind == "power_in" && b.kind == "power_in" && a.name == b.name;
                if !stacked {
                    add(
                        "symbol_pin_overlap",
                        format!("pins {} and {} are at the same place", a.number, b.number),
                    );
                }
            }
        }
    }
    Ok(found)
}
