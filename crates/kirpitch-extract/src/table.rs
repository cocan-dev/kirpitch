// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables from text with positions (the no-LLM table parsing of the
//! requirements: "text-based PDFs are parsed from text positions using column
//! detection").
//!
//! The columns are found from the words themselves: the horizontal space that
//! no word covers, in any line, separates two columns. This works for the plain
//! pin tables of datasheets, where every row has its cells under each other.

use crate::pdf::Line;

/// Smallest uncovered width, as a fraction of the page width, that separates
/// two columns. Spaces inside a cell are narrower.
pub const DEFAULT_COLUMN_GAP: f64 = 0.02;

/// A table: rows of cells. Cells that a row does not have are empty strings.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// The rows, top to bottom.
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// Number of columns.
    pub fn columns(&self) -> usize {
        self.rows.first().map_or(0, Vec::len)
    }

    /// The table as tab-separated text, which `kirpitch_core::pintable` reads.
    /// Tabs and line breaks inside cells cannot occur, since cells are built
    /// from words.
    pub fn to_tsv(&self) -> String {
        let mut out = String::new();
        for row in &self.rows {
            out.push_str(&row.join("\t"));
            out.push('\n');
        }
        out
    }
}

/// Splits lines into columns and cells.
pub fn table_from_lines(lines: &[Line], min_gap: f64) -> Table {
    // Horizontal intervals covered by words, merged when they touch or are
    // closer than `min_gap`.
    let mut spans: Vec<(f64, f64)> = lines
        .iter()
        .flat_map(|l| l.words.iter().map(|w| (w.left, w.right)))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut columns: Vec<(f64, f64)> = Vec::new();
    for (l, r) in spans {
        match columns.last_mut() {
            Some(last) if l - last.1 < min_gap => last.1 = last.1.max(r),
            _ => columns.push((l, r)),
        }
    }
    let rows = lines
        .iter()
        .map(|line| {
            let mut cells = vec![String::new(); columns.len()];
            for w in &line.words {
                let centre = (w.left + w.right) / 2.0;
                let col = columns
                    .iter()
                    .position(|(l, r)| centre >= *l && centre <= *r)
                    .unwrap_or(0);
                if !cells[col].is_empty() {
                    cells[col].push(' ');
                }
                cells[col].push_str(&w.text);
            }
            cells
        })
        .collect();
    Table { rows }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::Word;

    fn w(text: &str, left: f64, right: f64) -> Word {
        Word {
            text: text.into(),
            left,
            right,
        }
    }

    fn line(y: f64, words: Vec<Word>) -> Line {
        Line { y, words }
    }

    #[test]
    fn columns_are_found_from_the_gaps() {
        let lines = vec![
            line(0.1, vec![w("NAME", 0.10, 0.18), w("NO.", 0.40, 0.45)]),
            line(0.2, vec![w("GND", 0.10, 0.16), w("1", 0.41, 0.42)]),
            line(0.3, vec![w("TRIG", 0.10, 0.17), w("2", 0.41, 0.42)]),
        ];
        let t = table_from_lines(&lines, DEFAULT_COLUMN_GAP);
        assert_eq!(t.columns(), 2);
        assert_eq!(t.rows[1], ["GND", "1"]);
        assert_eq!(t.to_tsv(), "NAME\tNO.\nGND\t1\nTRIG\t2\n");
    }

    #[test]
    fn words_inside_a_cell_stay_together() {
        let lines = vec![
            line(
                0.1,
                vec![
                    w("Open", 0.10, 0.14),
                    w("drain", 0.145, 0.19),
                    w("O", 0.5, 0.52),
                ],
            ),
            line(0.2, vec![w("Input", 0.10, 0.16), w("I", 0.5, 0.51)]),
        ];
        let t = table_from_lines(&lines, DEFAULT_COLUMN_GAP);
        assert_eq!(t.rows[0], ["Open drain", "O"]);
        assert_eq!(t.rows[1], ["Input", "I"]);
    }

    #[test]
    fn missing_cells_are_empty() {
        let lines = vec![
            line(
                0.1,
                vec![w("1", 0.1, 0.11), w("A", 0.3, 0.31), w("x", 0.6, 0.61)],
            ),
            line(0.2, vec![w("2", 0.1, 0.11), w("x", 0.6, 0.61)]),
        ];
        let t = table_from_lines(&lines, DEFAULT_COLUMN_GAP);
        assert_eq!(t.rows[1], ["2", "", "x"]);
    }

    #[test]
    fn no_lines_give_an_empty_table() {
        let t = table_from_lines(&[], DEFAULT_COLUMN_GAP);
        assert_eq!(t.columns(), 0);
        assert_eq!(t.to_tsv(), "");
    }
}
