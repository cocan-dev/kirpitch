// SPDX-License-Identifier: GPL-3.0-or-later

//! Reads design rules that a PCB maker or a designer exported, into a
//! [`FabProfile`] (ADR 0032).
//!
//! There is no universal format. The two that exist as files are KiCad's
//! `.kicad_dru` (custom rules) and Eagle's `.dru` (design rules); both are read
//! here. Neither carries every limit that Kirpitch checks, and `.kicad_dru`
//! rules can apply to some objects only, so the result says what it used, what
//! it skipped and what it left at the default, and the user can correct the
//! numbers before saving.

use crate::fab::FabProfile;
use crate::units::Mm;

/// The result of reading a rules file.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The profile; limits the file does not give keep the generic values.
    pub profile: FabProfile,
    /// The limits that came from the file, one line each.
    pub used: Vec<String>,
    /// Things in the file that were not used, with the reason.
    pub skipped: Vec<String>,
}

/// Which format a rules file is in, from its name or, failing that, its text.
pub fn detect(file_name: &str, text: &str) -> Option<&'static str> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".kicad_dru") {
        Some("kicad")
    } else if lower.ends_with(".dru") {
        Some("eagle")
    } else if lower.ends_with(".json") || text.trim_start().starts_with('{') {
        Some("json")
    } else if text.contains("(rule ") || text.trim_start().starts_with("(version") {
        Some("kicad")
    } else if text.contains("mdWireWire") || text.contains("msWidth") {
        Some("eagle")
    } else {
        None
    }
}

/// Reads a rules file of any of the supported formats. `file_name` gives the
/// profile its name and, with the text, the format.
pub fn import(file_name: &str, text: &str) -> Result<Imported, String> {
    let name = std::path::Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("imported")
        .to_string();
    match detect(file_name, text) {
        Some("kicad") => from_kicad_dru(&name, text),
        Some("eagle") => from_eagle_dru(&name, text),
        Some("json") => {
            let profile = FabProfile::from_json_str(text)?;
            Ok(Imported {
                used: profile
                    .limits()
                    .iter()
                    .map(|(_, label, v)| format!("{label}: {} mm", crate::units::format_number(*v)))
                    .collect(),
                profile,
                skipped: Vec::new(),
            })
        }
        _ => Err(format!(
            "`{file_name}` is not a KiCad (.kicad_dru), Eagle (.dru) or Kirpitch (.json) rules file"
        )),
    }
}

// ---- KiCad ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Sexp {
    Atom(String),
    Text(String),
    List(Vec<Sexp>),
}

impl Sexp {
    fn atom(&self) -> Option<&str> {
        match self {
            Sexp::Atom(s) | Sexp::Text(s) => Some(s),
            Sexp::List(_) => None,
        }
    }

    fn items(&self) -> &[Sexp] {
        match self {
            Sexp::List(v) => v,
            _ => &[],
        }
    }

    /// The first item of a list, when it is a name.
    fn head(&self) -> Option<&str> {
        self.items().first().and_then(Sexp::atom)
    }

    /// The sub-lists with this name.
    fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Sexp> {
        self.items()
            .iter()
            .filter(move |c| matches!(c, Sexp::List(_)) && c.head() == Some(name))
    }
}

fn parse_sexp(text: &str) -> Result<Vec<Sexp>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut stack: Vec<Vec<Sexp>> = vec![Vec::new()];
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '#' => {
                // A comment runs to the end of the line.
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '(' => {
                stack.push(Vec::new());
                i += 1;
            }
            ')' => {
                if stack.len() < 2 {
                    return Err("unbalanced `)` in the rules file".to_string());
                }
                let done = stack.pop().expect("checked");
                stack.last_mut().expect("checked").push(Sexp::List(done));
                i += 1;
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    s.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return Err("a quoted text is not closed in the rules file".to_string());
                }
                i += 1;
                stack.last_mut().expect("never empty").push(Sexp::Text(s));
            }
            _ => {
                let mut s = String::new();
                while i < chars.len() && !chars[i].is_whitespace() && !"()\"".contains(chars[i]) {
                    s.push(chars[i]);
                    i += 1;
                }
                stack.last_mut().expect("never empty").push(Sexp::Atom(s));
            }
        }
    }
    if stack.len() != 1 {
        return Err("a `(` is not closed in the rules file".to_string());
    }
    Ok(stack.pop().expect("one level"))
}

/// A length with its unit (`0.1mm`, `4mil`, `0.004in`); without a unit it is
/// millimetres, as in KiCad's rules.
fn parse_length(text: &str) -> Option<f64> {
    let t = text.trim();
    let split = t
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
        })
        .unwrap_or(t.len());
    // `e` may begin a unit such as "em"; none of ours does, but "in" would not
    // be mistaken either way because it starts with `i`.
    let (number, unit) = t.split_at(split);
    let value: f64 = number.parse().ok()?;
    let factor = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "mm" => 1.0,
        "in" | "inch" | "\"" => 25.4,
        "mil" | "thou" => 0.0254,
        "mic" | "um" | "µm" => 0.001,
        _ => return None,
    };
    let mm = value * factor;
    (mm.is_finite() && mm > 0.0).then_some(mm)
}

/// Words in a rule's condition or layer that mean it is about only some
/// objects, so its limit does not describe an ordinary surface-mount pad.
const NARROW: [&str; 9] = [
    "inner",
    "npth",
    "non-plated",
    "castell",
    "slot",
    "microvia",
    "blind",
    "buried",
    "netclass",
];

fn mm_of(value: f64) -> Mm {
    Mm::new((value * 1e6).round() / 1e6).expect("finite")
}

/// Reads KiCad custom rules (`.kicad_dru`).
///
/// The constraints that map to a limit are `clearance`, `hole_size`,
/// `annular_width`, `hole_to_hole`, `silk_clearance`, `text_height` and
/// `text_thickness`. A rule that is limited to inner layers, non-plated holes,
/// slots, castellated holes, vias or net classes does not describe an ordinary
/// pad and is skipped. Where several rules remain for a limit, the smallest
/// minimum is used, which is the least the maker can do. `track_width`,
/// `edge_clearance` and the like are not used for footprints and are listed
/// as skipped.
pub fn from_kicad_dru(name: &str, text: &str) -> Result<Imported, String> {
    let tree = parse_sexp(text)?;
    let mut profile = FabProfile::generic();
    profile.name = name.to_string();
    let mut found: std::collections::BTreeMap<&'static str, (f64, String)> = Default::default();
    let mut skipped = Vec::new();
    let mut rules = 0;
    for rule in tree.iter().filter(|r| r.head() == Some("rule")) {
        rules += 1;
        let rule_name = rule
            .items()
            .get(1)
            .and_then(Sexp::atom)
            .unwrap_or("(unnamed)")
            .to_string();
        let condition = rule
            .children("condition")
            .filter_map(|c| c.items().get(1).and_then(Sexp::atom))
            .collect::<Vec<_>>()
            .join(" ");
        let layer = rule
            .children("layer")
            .filter_map(|c| c.items().get(1).and_then(Sexp::atom))
            .collect::<Vec<_>>()
            .join(" ");
        let scope = format!("{condition} {layer}").to_ascii_lowercase();
        for constraint in rule.children("constraint") {
            let Some(kind) = constraint.items().get(1).and_then(Sexp::atom) else {
                continue;
            };
            let minimum = constraint
                .children("min")
                .filter_map(|m| m.items().get(1).and_then(Sexp::atom))
                .find_map(parse_length);
            let field = match kind {
                "clearance" => Some(("min_copper_gap", "Copper to copper")),
                "hole_size" => Some(("min_drill", "Smallest drill")),
                "annular_width" => Some(("min_annular_ring", "Annular ring")),
                "hole_to_hole" => Some(("min_hole_to_hole", "Hole to hole")),
                "silk_clearance" => Some(("min_silk_pad_gap", "Silkscreen to copper")),
                "text_height" => Some(("min_silk_text_height", "Silkscreen text height")),
                "text_thickness" => Some(("min_silk_width", "Silkscreen line width")),
                _ => None,
            };
            let Some((field, label)) = field else {
                skipped.push(format!(
                    "rule `{rule_name}`: `{kind}` is not a limit of a footprint"
                ));
                continue;
            };
            let Some(value) = minimum else {
                skipped.push(format!(
                    "rule `{rule_name}`: `{kind}` has no minimum that could be read"
                ));
                continue;
            };
            if let Some(word) = NARROW.iter().find(|w| scope.contains(*w)) {
                skipped.push(format!(
                    "rule `{rule_name}`: `{kind}` is limited to `{word}`, not to ordinary pads"
                ));
                continue;
            }
            // Pad to pad: a clearance between a track and something else
            // says nothing about two pads.
            if kind == "clearance" && (scope.contains("track") || scope.contains("zone")) {
                skipped.push(format!(
                    "rule `{rule_name}`: a clearance for tracks or zones, not between pads"
                ));
                continue;
            }
            let entry = found
                .entry(field)
                .or_insert_with(|| (f64::INFINITY, String::new()));
            if value < entry.0 {
                *entry = (
                    value,
                    format!(
                        "{label}: {} mm, from rule `{rule_name}`",
                        crate::units::format_number(value)
                    ),
                );
            }
        }
    }
    if rules == 0 {
        return Err("no `(rule …)` found; this does not look like a .kicad_dru file".to_string());
    }
    let mut used = Vec::new();
    for (field, (value, line)) in found {
        let value = mm_of(value);
        match field {
            "min_copper_gap" => profile.min_copper_gap = value,
            "min_drill" => profile.min_drill = value,
            "min_annular_ring" => profile.min_annular_ring = value,
            "min_hole_to_hole" => profile.min_hole_to_hole = value,
            "min_silk_pad_gap" => profile.min_silk_pad_gap = value,
            "min_silk_text_height" => profile.min_silk_text_height = value,
            "min_silk_width" => profile.min_silk_width = value,
            _ => unreachable!("every mapped field is listed"),
        }
        used.push(line);
    }
    skipped.push(
        "The file has no smallest SMD pad and no solder mask web; those keep the generic values."
            .to_string(),
    );
    Ok(Imported {
        profile,
        used,
        skipped,
    })
}

// ---- Eagle ---------------------------------------------------------------

/// Reads Eagle design rules (`.dru`): lines such as `mdSmdSmd = 6mil`.
///
/// Used: `mdSmdSmd` (pad to pad; `mdPadPad` when it is missing), `msDrill`
/// (smallest drill), `mdDrill` (hole to hole) and `rlMinPadTop` (annular
/// ring). The others are listed as skipped.
pub fn from_eagle_dru(name: &str, text: &str) -> Result<Imported, String> {
    let mut profile = FabProfile::generic();
    profile.name = name.to_string();
    let mut used = Vec::new();
    let mut skipped = Vec::new();
    let mut values: std::collections::BTreeMap<String, f64> = Default::default();
    let mut lines = 0;
    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        // Eagle keys are a short prefix and a name: md, ms, ml, rl, rv ...
        if key.len() < 4 || !key.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        lines += 1;
        if let Some(mm) = parse_length(value)
            && value.chars().any(|c| c.is_ascii_alphabetic())
        {
            values.insert(key.to_string(), mm);
        }
    }
    if lines == 0
        || !values
            .keys()
            .any(|k| k.starts_with("md") || k.starts_with("ms"))
    {
        return Err("no Eagle design rules (`mdWireWire = …`) found in this file".to_string());
    }
    let pick = |keys: &[&'static str]| -> Option<(&'static str, f64)> {
        keys.iter().find_map(|k| values.get(*k).map(|v| (*k, *v)))
    };
    let mut set = |label: &str, hit: Option<(&'static str, f64)>, put: &mut dyn FnMut(Mm)| {
        if let Some((key, v)) = hit {
            put(mm_of(v));
            used.push(format!(
                "{label}: {} mm, from `{key}`",
                crate::units::format_number((v * 1e6).round() / 1e6)
            ));
        }
    };
    set(
        "Copper to copper",
        pick(&["mdSmdSmd", "mdPadPad"]),
        &mut |v| profile.min_copper_gap = v,
    );
    set("Smallest drill", pick(&["msDrill"]), &mut |v| {
        profile.min_drill = v
    });
    set("Hole to hole", pick(&["mdDrill"]), &mut |v| {
        profile.min_hole_to_hole = v
    });
    set("Annular ring", pick(&["rlMinPadTop"]), &mut |v| {
        profile.min_annular_ring = v
    });
    for key in values.keys() {
        if !["mdSmdSmd", "mdPadPad", "msDrill", "mdDrill", "rlMinPadTop"].contains(&key.as_str()) {
            skipped.push(format!("`{key}` is not a limit of a footprint"));
        }
    }
    skipped.push(
        "Eagle rules have no silkscreen limits, smallest SMD pad or solder mask web; those keep the generic values."
            .to_string(),
    );
    Ok(Imported {
        profile,
        used,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRU: &str = r#"
(version 1)
# Outer layers.
(rule "Clearance pad to pad (outer)"
  (layer outer)
  (condition "A.Type == 'Pad' && B.Type == 'Pad'")
  (constraint clearance (min 0.127mm)))
(rule "Clearance inner"
  (layer inner)
  (constraint clearance (min 0.09mm)))
(rule "Track to pad"
  (condition "A.Type == 'Track' && B.Type == 'Pad'")
  (constraint clearance (min 0.2mm)))
(rule "Holes"
  (constraint hole_size (min 0.2mm) (max 6.3mm)))
(rule "NPTH"
  (condition "A.Pad_Type == 'NPTH, mechanical'")
  (constraint hole_size (min 0.5mm)))
(rule "Ring"
  (constraint annular_width (min 3mil)))
(rule "Silk"
  (constraint silk_clearance (min 0.15mm)))
(rule "Text"
  (constraint text_height (min 1mm))
  (constraint text_thickness (min 0.15mm)))
(rule "Edge"
  (constraint edge_clearance (min 0.3mm)))
"#;

    #[test]
    fn a_kicad_rules_file_gives_the_limits_that_apply_to_pads() {
        let r = from_kicad_dru("maker", DRU).unwrap();
        let p = &r.profile;
        assert_eq!(p.name, "maker");
        // The outer pad rule, not the inner one or the track one.
        assert_eq!(p.min_copper_gap.format(), "0.127");
        // The ordinary holes, not the non-plated ones.
        assert_eq!(p.min_drill.format(), "0.2");
        assert_eq!(p.min_annular_ring.format(), "0.0762");
        assert_eq!(p.min_silk_pad_gap.format(), "0.15");
        assert_eq!(p.min_silk_text_height.format(), "1");
        assert_eq!(p.min_silk_width.format(), "0.15");
        // Not in the file: the generic values stay.
        assert_eq!(p.min_pad_size, FabProfile::generic().min_pad_size);
        assert_eq!(p.min_mask_web, FabProfile::generic().min_mask_web);
        assert_eq!(r.used.len(), 6);
        assert!(r.skipped.iter().any(|s| s.contains("inner")));
        assert!(r.skipped.iter().any(|s| s.contains("npth")));
        assert!(r.skipped.iter().any(|s| s.contains("tracks")));
        assert!(r.skipped.iter().any(|s| s.contains("edge_clearance")));
        p.check().unwrap();
    }

    #[test]
    fn a_file_that_is_not_kicad_rules_is_refused() {
        assert!(from_kicad_dru("x", "hello").is_err());
        assert!(from_kicad_dru("x", "(rule \"a\" (constraint clearance (min 1mm))").is_err());
        assert!(from_kicad_dru("x", "(version 1)").is_err());
    }

    #[test]
    fn lengths_have_units() {
        assert_eq!(parse_length("0.1mm"), Some(0.1));
        assert_eq!(parse_length("0.1"), Some(0.1));
        assert!((parse_length("4mil").unwrap() - 0.1016).abs() < 1e-9);
        assert!((parse_length("0.004in").unwrap() - 0.1016).abs() < 1e-9);
        assert_eq!(parse_length("10mic"), Some(0.01));
        assert_eq!(parse_length("2furlong"), None);
        assert_eq!(parse_length("0mm"), None);
    }

    #[test]
    fn an_eagle_rules_file_gives_the_limits_it_has() {
        let text = "description[en] = Maker\nmdWireWire = 6mil\nmdSmdSmd = 8mil\nmsWidth = 6mil\nmsDrill = 0.3mm\nmdDrill = 0.4mm\nrlMinPadTop = 7mil\npsTop = -1\n";
        let r = from_eagle_dru("eagle", text).unwrap();
        assert_eq!(r.profile.min_copper_gap.format(), "0.2032");
        assert_eq!(r.profile.min_drill.format(), "0.3");
        assert_eq!(r.profile.min_hole_to_hole.format(), "0.4");
        assert_eq!(r.profile.min_annular_ring.format(), "0.1778");
        assert_eq!(r.used.len(), 4);
        assert!(r.skipped.iter().any(|s| s.contains("msWidth")));
        assert!(from_eagle_dru("x", "nothing here").is_err());
    }

    #[test]
    fn the_format_is_found_from_the_name_or_the_text() {
        assert_eq!(detect("a.kicad_dru", ""), Some("kicad"));
        assert_eq!(detect("a.DRU", ""), Some("eagle"));
        assert_eq!(detect("a.json", ""), Some("json"));
        assert_eq!(detect("a.txt", "(version 1)\n(rule \"x\")"), Some("kicad"));
        assert_eq!(detect("a.txt", "mdWireWire = 1mil"), Some("eagle"));
        assert_eq!(detect("a.txt", "hello"), None);
        let json = import("fine.json", r#"{"min_copper_gap": 0.1}"#).unwrap();
        assert_eq!(json.profile.min_copper_gap.format(), "0.1");
        assert!(import("a.txt", "hello").is_err());
    }
}
