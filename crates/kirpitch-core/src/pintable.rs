// SPDX-License-Identifier: GPL-3.0-or-later

//! Import of pin tables from pasted text or CSV (comma-separated values).
//!
//! Datasheet pin tables copied from a PDF or spreadsheet come in many shapes:
//! with or without a header, separated by tabs, commas, semicolons, bars or
//! just spaces, with the pin number first or the name first. This module reads
//! them into [`Pin`] values and reports what it had to guess. Nothing is
//! guessed silently: every guessed type is listed in the warnings so that the
//! user can check it at the pinout gate.

use thiserror::Error;

use crate::electrical::ElectricalType;
use crate::names;
use crate::partspec::Pin;
use crate::source::Source;

/// Settings of the import.
#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    /// Source (page and region of the pin table) to attach to every pin.
    pub source: Option<Source>,
}

/// The pins read from a table and what needs a second look.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportResult {
    /// The pins, in the order of the table.
    pub pins: Vec<Pin>,
    /// Things that were guessed or skipped, one line each.
    pub warnings: Vec<String>,
}

/// Reasons a table cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImportError {
    /// There is no data row.
    #[error("the pin table is empty")]
    Empty,
    /// A row has no pin number or no pin name.
    #[error("line {line}: {message}")]
    Row {
        /// Line of the input, counting from 1.
        line: usize,
        /// What is wrong.
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Number,
    Name,
    Type,
    Group,
    Other,
}

/// Reads a pin table. See the module documentation for the accepted shapes.
pub fn parse_pin_table(text: &str, options: &ImportOptions) -> Result<ImportResult, ImportError> {
    let lines: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim_end_matches('\r')))
        .filter(|(_, l)| !l.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return Err(ImportError::Empty);
    }
    let delimiter = detect_delimiter(&lines);
    let mut rows: Vec<(usize, Vec<String>)> = lines
        .iter()
        .map(|(n, l)| (*n, split_row(l, delimiter)))
        .collect();

    // A header row names the columns; without one the order is number, name,
    // type, group.
    let columns = match header_columns(&rows[0].1) {
        Some(cols) => {
            rows.remove(0);
            cols
        }
        None => vec![Column::Number, Column::Name, Column::Type, Column::Group],
    };
    if rows.is_empty() {
        return Err(ImportError::Empty);
    }
    let find = |c: Column| columns.iter().position(|x| *x == c);
    let (Some(num_col), Some(name_col)) = (find(Column::Number), find(Column::Name)) else {
        return Err(ImportError::Row {
            line: lines[0].0,
            message: "the header needs a pin number column and a pin name column".into(),
        });
    };
    let type_col = find(Column::Type);
    let group_col = find(Column::Group);

    let mut pins = Vec::new();
    let mut warnings = Vec::new();
    for (line, cells) in &rows {
        let get = |i: usize| cells.get(i).map(|s| s.trim()).unwrap_or("");
        let numbers = split_numbers(get(num_col));
        let name = normalize_name(get(name_col));
        if numbers.is_empty() {
            return Err(ImportError::Row {
                line: *line,
                message: "the pin number is missing".into(),
            });
        }
        if name.is_empty() {
            return Err(ImportError::Row {
                line: *line,
                message: "the pin name is missing".into(),
            });
        }
        let type_text = type_col.map(get).unwrap_or("");
        let (electrical_type, guessed) = match guess_type(type_text) {
            Some(t) => (t, false),
            None => match type_from_name(&name) {
                Some(t) => (t, true),
                None => (ElectricalType::Unspecified, true),
            },
        };
        if guessed {
            warnings.push(format!(
                "line {line}: type of pin `{name}` is `{}`, {}",
                electrical_type.as_str(),
                if type_text.is_empty() {
                    "guessed from its name".to_string()
                } else {
                    format!("`{type_text}` was not understood")
                }
            ));
        }
        let group = group_col.map(get).filter(|g| !g.is_empty());
        for number in numbers {
            pins.push(Pin {
                number,
                name: name.clone(),
                electrical_type,
                group: group.map(str::to_string),
                alt_functions: Vec::new(),
                source: options.source,
            });
        }
    }
    Ok(ImportResult { pins, warnings })
}

/// Picks the separator used by most lines: tab, semicolon, bar, comma, or, if
/// none of these occurs, white space.
fn detect_delimiter(lines: &[(usize, &str)]) -> Option<char> {
    for d in ['\t', ';', '|', ','] {
        if lines.iter().all(|(_, l)| l.contains(d)) {
            return Some(d);
        }
    }
    ['\t', ';', '|', ',']
        .into_iter()
        .find(|&d| lines.iter().filter(|(_, l)| l.contains(d)).count() * 2 > lines.len())
}

/// Splits a row at the delimiter, honouring double quotes; without a delimiter
/// it splits at white space.
fn split_row(line: &str, delimiter: Option<char>) -> Vec<String> {
    let Some(d) = delimiter else {
        return line.split_whitespace().map(str::to_string).collect();
    };
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == d && !quoted => {
                cells.push(std::mem::take(&mut cell));
            }
            c => cell.push(c),
        }
    }
    cells.push(cell);
    cells
}

/// Recognises a header row and maps its cells to columns.
fn header_columns(cells: &[String]) -> Option<Vec<Column>> {
    let cols: Vec<Column> = cells
        .iter()
        .map(|c| {
            let c = c.trim().to_ascii_lowercase();
            match c.as_str() {
                "pin" | "pin no" | "pin no." | "pin number" | "number" | "no" | "no." | "#"
                | "pin #" | "pad" | "ball" => Column::Number,
                "name" | "pin name" | "symbol" | "signal" | "mnemonic" | "label" => Column::Name,
                "type" | "pin type" | "i/o" | "io" | "direction" | "electrical type" => {
                    Column::Type
                }
                "group" | "function group" | "block" | "section" => Column::Group,
                _ => Column::Other,
            }
        })
        .collect();
    // It is a header only when it has words for both a number and a name.
    (cols.contains(&Column::Number) && cols.contains(&Column::Name)).then_some(cols)
}

/// A number cell may hold several pins: `1, 8`, `4 5` or `4;5` (stacked
/// supply pins). Returns the numbers; empty when there is none.
fn split_numbers(cell: &str) -> Vec<String> {
    cell.split([',', ';', ' ', '/'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Writes inverted signals in KiCad's overbar notation `~{NAME}`. Names that
/// start with `/`, `~` or `!`, or end with `#`, are inverted signals.
fn normalize_name(name: &str) -> String {
    let n = name.trim();
    if n.contains("~{") {
        return n.to_string();
    }
    for prefix in ['/', '~', '!'] {
        if let Some(rest) = n.strip_prefix(prefix)
            && !rest.is_empty()
        {
            return format!("~{{{rest}}}");
        }
    }
    if let Some(rest) = n.strip_suffix('#')
        && !rest.is_empty()
    {
        return format!("~{{{rest}}}");
    }
    n.to_string()
}

/// Understands the type words that datasheets use.
fn guess_type(text: &str) -> Option<ElectricalType> {
    let t = text.trim().to_ascii_lowercase();
    if t.is_empty() {
        return None;
    }
    let exact = match t.as_str() {
        "i" | "in" | "input" => Some(ElectricalType::Input),
        "o" | "out" | "output" => Some(ElectricalType::Output),
        "io" | "i/o" | "bidir" | "bidirectional" | "inout" | "in/out" => {
            Some(ElectricalType::Bidirectional)
        }
        "p" | "pwr" | "power" | "supply" | "vcc" | "vdd" | "g" | "gnd" | "ground" => {
            Some(ElectricalType::PowerIn)
        }
        "nc" | "n/c" | "dnc" | "no connect" | "not connected" | "no connection" => {
            Some(ElectricalType::NoConnect)
        }
        "passive" | "analog" | "a" => Some(ElectricalType::Passive),
        "z" | "tristate" | "tri-state" | "3-state" => Some(ElectricalType::TriState),
        _ => None,
    };
    if exact.is_some() {
        return exact;
    }
    if t.contains("open drain")
        || t.contains("open-drain")
        || t.contains("open collector")
        || t.contains("open-collector")
    {
        return Some(ElectricalType::OpenCollector);
    }
    if t.contains("open source") || t.contains("open emitter") {
        return Some(ElectricalType::OpenEmitter);
    }
    if t.contains("power out") || t.contains("supply out") {
        return Some(ElectricalType::PowerOut);
    }
    if t.contains("i/o") || t.contains("bidir") {
        return Some(ElectricalType::Bidirectional);
    }
    if t.contains("tri") && t.contains("state") {
        return Some(ElectricalType::TriState);
    }
    if t.contains("power") || t.contains("supply") || t.contains("ground") {
        return Some(ElectricalType::PowerIn);
    }
    if t.starts_with("output") {
        return Some(ElectricalType::Output);
    }
    if t.starts_with("input") {
        return Some(ElectricalType::Input);
    }
    None
}

fn type_from_name(name: &str) -> Option<ElectricalType> {
    let upper = name.to_ascii_uppercase();
    if matches!(upper.as_str(), "NC" | "N.C." | "DNC" | "N/C")
        || upper.starts_with("NC") && upper[2..].chars().all(|c| c.is_ascii_digit())
    {
        return Some(ElectricalType::NoConnect);
    }
    if names::looks_like_ground(name) || names::looks_like_supply(name) {
        return Some(ElectricalType::PowerIn);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> ImportResult {
        parse_pin_table(text, &ImportOptions::default()).unwrap()
    }

    fn names(r: &ImportResult) -> Vec<(&str, &str, ElectricalType)> {
        r.pins
            .iter()
            .map(|p| (p.number.as_str(), p.name.as_str(), p.electrical_type))
            .collect()
    }

    #[test]
    fn reads_a_csv_with_header_and_types() {
        let r = parse("Pin,Name,Type\n1,OUT1,Output\n2,IN1-,Input\n8,V+,Power\n");
        assert_eq!(
            names(&r),
            [
                ("1", "OUT1", ElectricalType::Output),
                ("2", "IN1-", ElectricalType::Input),
                ("8", "V+", ElectricalType::PowerIn),
            ]
        );
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn reads_tab_separated_text_from_a_spreadsheet() {
        let r = parse("No.\tSymbol\tI/O\tGroup\n1\tVDD\tP\tPower\n2\tSDA\tI/O\tI2C\n");
        assert_eq!(r.pins[1].electrical_type, ElectricalType::Bidirectional);
        assert_eq!(r.pins[1].group.as_deref(), Some("I2C"));
        assert_eq!(r.pins[0].electrical_type, ElectricalType::PowerIn);
    }

    #[test]
    fn name_first_columns_are_found_by_the_header() {
        let r = parse("Name;Pin\nCLK;3\nDATA;4\n");
        assert_eq!(r.pins[0].number, "3");
        assert_eq!(r.pins[0].name, "CLK");
    }

    #[test]
    fn reads_space_separated_lines_without_header() {
        let r = parse("1 GND\n2 TRIG\n3 OUT\n8 VCC\n");
        assert_eq!(r.pins.len(), 4);
        assert_eq!(r.pins[0].electrical_type, ElectricalType::PowerIn);
        assert_eq!(r.pins[3].electrical_type, ElectricalType::PowerIn);
        // TRIG and OUT are guessed as unspecified and reported.
        assert_eq!(r.pins[1].electrical_type, ElectricalType::Unspecified);
        assert_eq!(r.warnings.len(), 4);
        assert!(r.warnings.iter().any(|w| w.contains("`TRIG`")));
    }

    #[test]
    fn inverted_signals_get_the_kicad_overbar() {
        let r = parse("1 /CS\n2 RESET#\n3 ~WE\n4 !OE\n5 ~{OK}\n6 A/B\n");
        let n: Vec<&str> = r.pins.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(n, ["~{CS}", "~{RESET}", "~{WE}", "~{OE}", "~{OK}", "A/B"]);
    }

    #[test]
    fn one_row_can_list_several_pins_for_stacked_supplies() {
        let r = parse("Pin,Name,Type\n\"4, 5\",GND,Power\n1,VCC,Power\n");
        assert_eq!(r.pins.len(), 3);
        assert_eq!(
            (r.pins[0].number.as_str(), r.pins[1].number.as_str()),
            ("4", "5")
        );
        assert_eq!(r.pins[0].name, "GND");
    }

    #[test]
    fn quoted_cells_may_contain_the_delimiter() {
        let r = parse("Pin,Name,Type\n1,\"A,B\",Input\n");
        assert_eq!(r.pins[0].name, "A,B");
    }

    #[test]
    fn nc_pins_are_recognised() {
        let r = parse("1 NC\n2 N.C.\n3 NC2\n");
        assert!(
            r.pins
                .iter()
                .all(|p| p.electrical_type == ElectricalType::NoConnect)
        );
    }

    #[test]
    fn type_words_of_datasheets() {
        use ElectricalType::*;
        for (text, want) in [
            ("Open Drain", OpenCollector),
            ("open-collector output", OpenCollector),
            ("Power output", PowerOut),
            ("Ground", PowerIn),
            ("Tri-State", TriState),
            ("Bidirectional", Bidirectional),
            ("No Connect", NoConnect),
        ] {
            assert_eq!(guess_type(text), Some(want), "{text}");
        }
        assert_eq!(guess_type("who knows"), None);
    }

    #[test]
    fn unknown_type_text_is_reported_not_hidden() {
        let r = parse("Pin,Name,Type\n1,FOO,wobbly\n");
        assert_eq!(r.pins[0].electrical_type, ElectricalType::Unspecified);
        assert!(r.warnings[0].contains("`wobbly` was not understood"));
    }

    #[test]
    fn source_is_attached_to_every_pin() {
        let source = Source {
            page: 4,
            bbox: None,
            confidence: None,
        };
        let r = parse_pin_table(
            "1 A\n2 B\n",
            &ImportOptions {
                source: Some(source),
            },
        )
        .unwrap();
        assert!(r.pins.iter().all(|p| p.source == Some(source)));
    }

    #[test]
    fn errors_name_the_line() {
        assert_eq!(
            parse_pin_table("  \n\n", &ImportOptions::default()),
            Err(ImportError::Empty)
        );
        assert_eq!(
            parse_pin_table("Pin,Name\n", &ImportOptions::default()),
            Err(ImportError::Empty)
        );
        let e = parse_pin_table("Pin,Name\n1,A\n,B\n", &ImportOptions::default()).unwrap_err();
        assert_eq!(
            e,
            ImportError::Row {
                line: 3,
                message: "the pin number is missing".into()
            }
        );
        let e = parse_pin_table("Pin,Name\n1,\n", &ImportOptions::default()).unwrap_err();
        assert!(matches!(e, ImportError::Row { line: 2, .. }));
    }

    #[test]
    fn windows_line_endings_are_fine() {
        let r = parse("Pin,Name,Type\r\n1,A,Input\r\n2,B,Output\r\n");
        assert_eq!(r.pins.len(), 2);
        assert_eq!(r.pins[1].name, "B");
    }
}
