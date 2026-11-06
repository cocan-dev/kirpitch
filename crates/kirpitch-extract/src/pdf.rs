// SPDX-License-Identifier: GPL-3.0-or-later

//! PDF pages: rendering and text with positions, using pdfium (ADR 0016).
//!
//! All positions leave this module as fractions of the page size, measured from
//! the top-left corner, the same convention as `kirpitch_core::BoundingBox`.
//! Page numbers count from 1 like everywhere else in Kirpitch.

use std::path::Path;

use kirpitch_core::BoundingBox;
use pdfium_render::prelude::*;
use thiserror::Error;

/// Reasons a PDF cannot be read.
#[derive(Debug, Error)]
pub enum PdfError {
    /// The pdfium library could not be found or loaded.
    #[error("cannot load the pdfium library: {0}")]
    Library(String),
    /// The file is not a PDF that pdfium can open.
    #[error("cannot open the PDF: {0}")]
    Open(String),
    /// The page number is outside the document.
    #[error("page {page} does not exist; the document has {count} page(s)")]
    NoSuchPage {
        /// The requested page, counting from 1.
        page: u32,
        /// Number of pages in the document.
        count: u32,
    },
    /// A page could not be rendered or read.
    #[error("cannot read page {page}: {message}")]
    Page {
        /// The page, counting from 1.
        page: u32,
        /// What pdfium reported.
        message: String,
    },
}

/// A character of a page with its position.
#[derive(Debug, Clone, PartialEq)]
pub struct TextChar {
    /// The character.
    pub ch: char,
    /// Its box as fractions of the page, from the top-left corner.
    pub bbox: BoundingBox,
}

/// A word: characters that follow each other without a gap.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    /// The text.
    pub text: String,
    /// Left edge as a fraction of the page width.
    pub left: f64,
    /// Right edge as a fraction of the page width.
    pub right: f64,
}

/// A line of words.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Vertical centre as a fraction of the page height.
    pub y: f64,
    /// The words from left to right.
    pub words: Vec<Word>,
}

impl Line {
    /// The words joined by single spaces.
    pub fn text(&self) -> String {
        self.words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A page rendered as an image.
#[derive(Debug, Clone)]
pub struct RenderedPage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixels as RGBA, row by row from the top.
    pub rgba: Vec<u8>,
}

/// Reads PDFs with pdfium.
pub struct PdfReader {
    pdfium: Pdfium,
}

fn page_err(page: u32, e: impl std::fmt::Display) -> PdfError {
    PdfError::Page {
        page,
        message: e.to_string(),
    }
}

impl PdfReader {
    /// Loads pdfium from `library_dir`, or from the system when it is `None`.
    pub fn new(library_dir: Option<&Path>) -> Result<PdfReader, PdfError> {
        let name = match library_dir {
            Some(dir) => Pdfium::pdfium_platform_library_name_at_path(dir).into_os_string(),
            None => Pdfium::pdfium_platform_library_name(),
        };
        let bindings =
            Pdfium::bind_to_library(name).map_err(|e| PdfError::Library(e.to_string()))?;
        Ok(PdfReader {
            pdfium: Pdfium::new(bindings),
        })
    }

    fn open<'a>(&'a self, bytes: &'a [u8]) -> Result<PdfDocument<'a>, PdfError> {
        self.pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|e| PdfError::Open(e.to_string()))
    }

    fn page<'a>(document: &PdfDocument<'a>, page: u32) -> Result<PdfPage<'a>, PdfError> {
        let count = document.pages().len() as u32;
        if page == 0 || page > count {
            return Err(PdfError::NoSuchPage { page, count });
        }
        document
            .pages()
            .get((page - 1) as i32)
            .map_err(|e| page_err(page, e))
    }

    /// Number of pages.
    pub fn page_count(&self, bytes: &[u8]) -> Result<u32, PdfError> {
        Ok(self.open(bytes)?.pages().len() as u32)
    }

    /// Size of a page in points (1/72 inch): `(width, height)`.
    pub fn page_size(&self, bytes: &[u8], page: u32) -> Result<(f32, f32), PdfError> {
        let doc = self.open(bytes)?;
        let p = Self::page(&doc, page)?;
        Ok((p.width().value, p.height().value))
    }

    /// Renders a page to an image `width_px` pixels wide.
    pub fn render_page(
        &self,
        bytes: &[u8],
        page: u32,
        width_px: u32,
    ) -> Result<RenderedPage, PdfError> {
        let doc = self.open(bytes)?;
        let p = Self::page(&doc, page)?;
        let config = PdfRenderConfig::new().set_target_width(width_px as i32);
        let bitmap = p
            .render_with_config(&config)
            .map_err(|e| page_err(page, e))?;
        Ok(RenderedPage {
            width: bitmap.width() as u32,
            height: bitmap.height() as u32,
            rgba: bitmap.as_rgba_bytes(),
        })
    }

    /// Every character of a page with the box of its glyph cell (the font height,
    /// not just the ink, so dots and commas sit on the line of their word), in
    /// reading order of the
    /// PDF content stream.
    pub fn text_chars(&self, bytes: &[u8], page: u32) -> Result<Vec<TextChar>, PdfError> {
        let doc = self.open(bytes)?;
        let p = Self::page(&doc, page)?;
        let (w, h) = (f64::from(p.width().value), f64::from(p.height().value));
        let text = p.text().map_err(|e| page_err(page, e))?;
        let mut out = Vec::new();
        for c in text.chars().iter() {
            let Some(ch) = c.unicode_char() else { continue };
            let Ok(r) = c.loose_bounds() else { continue };
            let (left, right) = (
                f64::from(r.left().value) / w,
                f64::from(r.right().value) / w,
            );
            // PDF coordinates start at the bottom-left corner.
            let (top, bottom) = (
                1.0 - f64::from(r.top().value) / h,
                1.0 - f64::from(r.bottom().value) / h,
            );
            out.push(TextChar {
                ch,
                bbox: BoundingBox([left, top, right, bottom]),
            });
        }
        Ok(out)
    }

    /// The lines of text inside `region`, top to bottom. A character belongs to
    /// the region when its centre is inside.
    pub fn lines_in_region(
        &self,
        bytes: &[u8],
        page: u32,
        region: BoundingBox,
    ) -> Result<Vec<Line>, PdfError> {
        Ok(lines_from_chars(&self.text_chars(bytes, page)?, region))
    }

    /// The text inside `region`, one line per line of the page.
    pub fn text_in_region(
        &self,
        bytes: &[u8],
        page: u32,
        region: BoundingBox,
    ) -> Result<String, PdfError> {
        let lines = self.lines_in_region(bytes, page, region)?;
        Ok(lines.iter().map(Line::text).collect::<Vec<_>>().join("\n"))
    }
}

/// Groups characters into lines and words. Pure geometry, so it can be tested
/// without pdfium.
pub fn lines_from_chars(chars: &[TextChar], region: BoundingBox) -> Vec<Line> {
    let [rl, rt, rr, rb] = region.0;
    let mut inside: Vec<&TextChar> = chars
        .iter()
        .filter(|c| {
            let [l, t, r, b] = c.bbox.0;
            let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
            !c.ch.is_control() && cx >= rl && cx <= rr && cy >= rt && cy <= rb
        })
        .collect();
    // Top to bottom, then group characters whose centres are within half a
    // character height of the line.
    inside.sort_by(|a, b| centre_y(a).total_cmp(&centre_y(b)));
    let mut rows: Vec<Vec<&TextChar>> = Vec::new();
    for c in inside {
        let height = c.bbox.0[3] - c.bbox.0[1];
        match rows.last_mut() {
            Some(row) if (centre_y(row[0]) - centre_y(c)).abs() <= height / 2.0 => row.push(c),
            _ => rows.push(vec![c]),
        }
    }
    rows.into_iter()
        .map(|mut row| {
            row.sort_by(|a, b| a.bbox.0[0].total_cmp(&b.bbox.0[0]));
            let y = row.iter().map(|c| centre_y(c)).sum::<f64>() / row.len() as f64;
            Line {
                y,
                words: words_of(&row),
            }
        })
        .filter(|line: &Line| !line.words.is_empty())
        .collect()
}

fn centre_y(c: &TextChar) -> f64 {
    (c.bbox.0[1] + c.bbox.0[3]) / 2.0
}

/// A gap wider than a third of a character starts a new word; spaces in the
/// PDF also end a word.
fn words_of(row: &[&TextChar]) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut prev_right: Option<f64> = None;
    let mut start_new = true;
    for c in row {
        if c.ch.is_whitespace() {
            start_new = true;
            continue;
        }
        let [l, _, r, _] = c.bbox.0;
        let width = r - l;
        if let Some(pr) = prev_right
            && l - pr > width / 3.0
        {
            start_new = true;
        }
        if start_new {
            words.push(Word {
                text: String::new(),
                left: l,
                right: r,
            });
            start_new = false;
        }
        let w = words.last_mut().expect("a word was started");
        w.text.push(c.ch);
        w.right = r;
        prev_right = Some(r);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(c: char, left: f64, top: f64) -> TextChar {
        TextChar {
            ch: c,
            bbox: BoundingBox([left, top, left + 0.01, top + 0.02]),
        }
    }

    fn word(s: &str, left: f64, top: f64) -> Vec<TextChar> {
        s.chars()
            .enumerate()
            .map(|(i, c)| ch(c, left + i as f64 * 0.011, top))
            .collect()
    }

    #[test]
    fn characters_become_words_and_lines() {
        let mut chars = word("1", 0.10, 0.10);
        chars.extend(word("GND", 0.30, 0.10));
        chars.extend(word("2", 0.10, 0.20));
        chars.extend(word("OUT", 0.30, 0.20));
        let lines = lines_from_chars(&chars, BoundingBox([0.0, 0.0, 1.0, 1.0]));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text(), "1 GND");
        assert_eq!(lines[1].text(), "2 OUT");
        assert!(lines[0].y < lines[1].y);
        assert_eq!(lines[0].words[1].text, "GND");
        assert!(lines[0].words[1].left > lines[0].words[0].right);
    }

    #[test]
    fn only_characters_inside_the_region_count() {
        let mut chars = word("IN", 0.10, 0.10);
        chars.extend(word("OUT", 0.60, 0.10));
        let lines = lines_from_chars(&chars, BoundingBox([0.5, 0.0, 1.0, 1.0]));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text(), "OUT");
        assert!(lines_from_chars(&chars, BoundingBox([0.0, 0.5, 1.0, 1.0])).is_empty());
    }

    #[test]
    fn a_line_slightly_off_still_joins() {
        let mut chars = word("A", 0.10, 0.100);
        chars.extend(word("B", 0.30, 0.105));
        let lines = lines_from_chars(&chars, BoundingBox([0.0, 0.0, 1.0, 1.0]));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text(), "A B");
    }

    #[test]
    fn spaces_and_control_characters_end_words() {
        let mut chars = word("AB", 0.10, 0.10);
        chars.push(ch(' ', 0.133, 0.10));
        chars.extend(word("CD", 0.134, 0.10));
        chars.push(ch('\r', 0.2, 0.10));
        let lines = lines_from_chars(&chars, BoundingBox([0.0, 0.0, 1.0, 1.0]));
        assert_eq!(lines[0].text(), "AB CD");
    }
}
