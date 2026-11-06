// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks of generated footprints against the KiCad Library Conventions (KLC).
//!
//! This is the Rust port started in milestone 2a (ADR 0017). The KLC web site
//! could not be read when this was written (it refused automated access), so
//! the checks do **not** cite KLC rule numbers yet. The values come from
//! KiCad's own footprint generator configuration and from the official
//! libraries, which pass these checks (see the tests): courtyard on the
//! `F.CrtYd` layer with 0.05 mm lines on a 0.01 mm grid, an outline on `F.Fab`,
//! silkscreen at least 0.12 mm wide, the reference on `F.SilkS`, the value and
//! `${REFERENCE}` on `F.Fab`. Rule numbers will be added once the current text
//! has been read.

use crate::sexpr::{List, Node, ParseError, parse};

/// A deviation from the conventions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KlcFinding {
    /// Short identifier of the check.
    pub check: &'static str,
    /// What is wrong.
    pub message: String,
}

fn child<'a>(list: &'a List, name: &str) -> Option<&'a List> {
    list.items.iter().find_map(|n| match n {
        Node::List(l) if l.name() == Some(name) => Some(l),
        _ => None,
    })
}

fn string_of(list: &List, index: usize) -> Option<&str> {
    match list.items.get(index) {
        Some(Node::Str(s)) | Some(Node::Atom(s)) => Some(s),
        _ => None,
    }
}

fn number_of(list: &List, index: usize) -> Option<f64> {
    string_of(list, index)?.parse().ok()
}

fn layer(item: &List) -> Option<&str> {
    child(item, "layer").and_then(|l| string_of(l, 1))
}

fn stroke_width(item: &List) -> Option<f64> {
    child(child(item, "stroke")?, "width").and_then(|w| number_of(w, 1))
}

/// All coordinates of the points of a graphic item.
fn coordinates(item: &List, out: &mut Vec<f64>) {
    for n in &item.items {
        if let Node::List(l) = n {
            match l.name() {
                Some("start" | "end" | "mid" | "center" | "xy") => {
                    out.extend((1..l.items.len()).filter_map(|i| number_of(l, i)));
                }
                Some("pts") => coordinates(l, out),
                _ => {}
            }
        }
    }
}

const GRAPHICS: [&str; 6] = [
    "fp_line",
    "fp_rect",
    "fp_poly",
    "fp_arc",
    "fp_circle",
    "fp_curve",
];

/// Checks the text of a `.kicad_mod` file. An `Err` means it cannot be parsed.
pub fn check_footprint(text: &str) -> Result<Vec<KlcFinding>, ParseError> {
    let fp = parse(text)?;
    let mut found = Vec::new();
    let mut add = |check: &'static str, message: String| found.push(KlcFinding { check, message });

    let graphics: Vec<&List> = fp
        .items
        .iter()
        .filter_map(|n| match n {
            Node::List(l) if l.name().is_some_and(|n| GRAPHICS.contains(&n)) => Some(l),
            _ => None,
        })
        .collect();
    let on = |layer_name: &str| -> Vec<&List> {
        graphics
            .iter()
            .copied()
            .filter(|g| layer(g) == Some(layer_name))
            .collect()
    };

    let courtyard = on("F.CrtYd");
    if courtyard.is_empty() {
        add("courtyard_present", "no courtyard on F.CrtYd".into());
    }
    for g in &courtyard {
        if stroke_width(g).is_none_or(|w| (w - 0.05).abs() > 1e-9) {
            add(
                "courtyard_width",
                "courtyard lines must be 0.05 mm wide".into(),
            );
            break;
        }
    }
    let mut coords = Vec::new();
    for g in &courtyard {
        coordinates(g, &mut coords);
    }
    if let Some(c) = coords
        .iter()
        .find(|c| ((**c / 0.01).round() * 0.01 - **c).abs() > 1e-6)
    {
        add(
            "courtyard_grid",
            format!("courtyard coordinate {c} is not on the 0.01 mm grid"),
        );
    }

    if on("F.Fab").is_empty() {
        add("fab_present", "no outline on F.Fab".into());
    }
    for g in on("F.SilkS") {
        if stroke_width(g).is_none_or(|w| w < 0.12 - 1e-9) {
            add(
                "silk_width",
                "silkscreen lines must be at least 0.12 mm wide".into(),
            );
            break;
        }
    }

    let property = |name: &str| {
        fp.items.iter().find_map(|n| match n {
            Node::List(l) if l.name() == Some("property") && string_of(l, 1) == Some(name) => {
                Some(l)
            }
            _ => None,
        })
    };
    match property("Reference") {
        Some(p) if layer(p) == Some("F.SilkS") => {}
        _ => add(
            "reference_layer",
            "the Reference text must be on F.SilkS".into(),
        ),
    }
    match property("Value") {
        Some(p) if layer(p) == Some("F.Fab") => {}
        _ => add("value_layer", "the Value text must be on F.Fab".into()),
    }
    let has_reference_text = fp.items.iter().any(|n| {
        matches!(n, Node::List(l)
            if l.name() == Some("fp_text")
                && string_of(l, 1) == Some("user")
                && string_of(l, 2) == Some("${REFERENCE}")
                && layer(l) == Some("F.Fab"))
    });
    if !has_reference_text {
        add(
            "reference_text",
            "a ${REFERENCE} text on F.Fab is missing".into(),
        );
    }
    let pads = fp
        .items
        .iter()
        .filter(|n| matches!(n, Node::List(l) if l.name() == Some("pad")))
        .count();
    if pads == 0 {
        add("pads_present", "the footprint has no pads".into());
    }
    Ok(found)
}
