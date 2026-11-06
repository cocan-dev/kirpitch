// SPDX-License-Identifier: GPL-3.0-or-later

//! The guided way to read a datasheet: the person points at where the part
//! number, the pin table and the package drawing are (a page, or a rectangle
//! drawn on a page), and the models read only that text. It costs fewer tokens
//! and does not depend on the program guessing the right pages.

use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;

use kirpitch_core::BoundingBox;
use kirpitch_models::{PageLines, TextLine};
use serde::{Deserialize, Serialize};

use crate::{pdf_reader, read_pages};

/// A place on a page: a rectangle as fractions from the top left corner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionInput {
    /// The page, from 1.
    pub page: u32,
    /// Left, top, right, bottom; `[0, 0, 1, 1]` is the whole page.
    pub bbox: [f64; 4],
}

/// The places the person pointed at, for each step. A step with no place is
/// searched for by the program as before.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GuideInput {
    /// Where the part number, maker and description are.
    #[serde(default)]
    pub metadata: Vec<RegionInput>,
    /// Where the pin table is.
    #[serde(default)]
    pub pins: Vec<RegionInput>,
    /// Where the package drawing and its dimensions are.
    #[serde(default)]
    pub package: Vec<RegionInput>,
    /// Where the recommended footprint (land pattern) is.
    #[serde(default)]
    pub footprint: Vec<RegionInput>,
}

impl GuideInput {
    /// True when no step has a place.
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
            && self.pins.is_empty()
            && self.package.is_empty()
            && self.footprint.is_empty()
    }
}

fn region_box(r: &RegionInput) -> Result<BoundingBox, String> {
    let b = BoundingBox(r.bbox);
    if b.is_valid() && r.bbox[2] - r.bbox[0] > 0.001 && r.bbox[3] - r.bbox[1] > 0.001 {
        Ok(b)
    } else {
        Err(format!(
            "the region on page {} is empty or outside the page",
            r.page
        ))
    }
}

/// The text inside the regions, as pages (several regions of one page become
/// one page, lines top to bottom). `None` when there are no regions.
///
/// # Errors
///
/// A region is not valid or has no text (a scanned page).
pub fn text_of_regions(
    bytes: &[u8],
    regions: &[RegionInput],
) -> Result<Option<Vec<PageLines>>, String> {
    if regions.is_empty() {
        return Ok(None);
    }
    let reader = pdf_reader()?;
    let mut pages: BTreeMap<u32, Vec<TextLine>> = BTreeMap::new();
    for r in regions {
        let bbox = region_box(r)?;
        let lines = reader
            .lines_in_region(bytes, r.page, bbox)
            .map_err(|e| e.to_string())?;
        let converted: Vec<TextLine> = lines
            .iter()
            .filter_map(|l| {
                let (first, last) = (l.words.first()?, l.words.last()?);
                Some(TextLine {
                    text: l.text(),
                    y: l.y,
                    left: first.left,
                    right: last.right,
                })
            })
            .collect();
        if converted.is_empty() {
            return Err(format!(
                "there is no text in the region on page {} (a scanned page needs OCR)",
                r.page
            ));
        }
        pages.entry(r.page).or_default().extend(converted);
    }
    Ok(Some(
        pages
            .into_iter()
            .map(|(page, mut lines)| {
                lines.sort_by(|a, b| a.y.total_cmp(&b.y));
                PageLines { page, lines }
            })
            .collect(),
    ))
}

// ---- Suggestions and a preview ---------------------------------------------------------------

/// A page the program would look at, with its score.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suggested {
    /// The page.
    pub page: u32,
    /// How well it matches the words of the step.
    pub score: i32,
}

/// Pages to suggest for each step.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suggestions {
    /// Pages in the datasheet.
    pub page_count: u32,
    /// For the part number.
    pub metadata: Vec<Suggested>,
    /// For the pin table.
    pub pins: Vec<Suggested>,
    /// For the package.
    pub package: Vec<Suggested>,
}

type PageCache = Option<(
    String,
    u64,
    std::time::SystemTime,
    std::sync::Arc<Vec<PageLines>>,
)>;
static PAGES: Mutex<PageCache> = Mutex::new(None);

/// The text of every page, kept for the next question about the same file (a
/// large datasheet takes a moment to read).
fn all_pages(pdf: &str) -> Result<std::sync::Arc<Vec<PageLines>>, String> {
    let meta = fs::metadata(pdf).map_err(|e| format!("{pdf}: {e}"))?;
    let stamp = (meta.len(), meta.modified().unwrap_or(std::time::UNIX_EPOCH));
    if let Ok(cache) = PAGES.lock()
        && let Some((p, len, time, pages)) = cache.as_ref()
        && p == pdf
        && *len == stamp.0
        && *time == stamp.1
    {
        return Ok(pages.clone());
    }
    let bytes = fs::read(pdf).map_err(|e| format!("{pdf}: {e}"))?;
    let pages = std::sync::Arc::new(read_pages(&bytes)?);
    if let Ok(mut cache) = PAGES.lock() {
        *cache = Some((pdf.to_string(), stamp.0, stamp.1, pages.clone()));
    }
    Ok(pages)
}

/// Pages the automatic search would look at, step by step.
///
/// # Errors
///
/// The PDF cannot be read.
pub fn suggest_pages_for(pdf: &str, hint: &str) -> Result<Suggestions, String> {
    let pages = all_pages(pdf)?;
    let hint = Some(hint.trim()).filter(|h| !h.is_empty());
    let list = |step: &str| {
        kirpitch_models::suggest_pages(&pages, step, hint)
            .into_iter()
            .map(|(page, score)| Suggested { page, score })
            .collect::<Vec<_>>()
    };
    Ok(Suggestions {
        page_count: u32::try_from(pages.len()).unwrap_or(u32::MAX),
        metadata: list("metadata"),
        pins: list("pins"),
        package: list("package"),
    })
}

/// How much text a step would send.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct StepSize {
    /// Characters of the places chosen (0 when none were chosen).
    pub chosen: usize,
    /// Characters the automatic search would send, about.
    pub automatic: usize,
}

/// The sizes for the three steps.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Preview {
    /// Part number.
    pub metadata: StepSize,
    /// Pin table.
    pub pins: StepSize,
    /// Package.
    pub package: StepSize,
    /// Recommended footprint.
    pub footprint: StepSize,
}

fn chars(pages: &[PageLines]) -> usize {
    pages.iter().map(|p| p.as_text().len()).sum()
}

/// Compares what the chosen places hold with what the search would send.
///
/// # Errors
///
/// The PDF cannot be read or a region has no text.
pub fn preview_selection(pdf: &str, guide: &GuideInput, hint: &str) -> Result<Preview, String> {
    let bytes = fs::read(pdf).map_err(|e| format!("{pdf}: {e}"))?;
    let pages = all_pages(pdf)?;
    let hint_opt = Some(hint.trim()).filter(|h| !h.is_empty());
    let automatic = |step: &str, most: usize| -> usize {
        let best: Vec<u32> = kirpitch_models::suggest_pages(&pages, step, hint_opt)
            .into_iter()
            .take(most)
            .map(|(p, _)| p)
            .collect();
        pages
            .iter()
            .filter(|p| best.contains(&p.page))
            .map(|p| p.as_text().len())
            .sum()
    };
    let step = |regions: &[RegionInput], name: &str, most: usize| -> Result<StepSize, String> {
        Ok(StepSize {
            chosen: text_of_regions(&bytes, regions)?.map_or(0, |p| chars(&p)),
            automatic: automatic(name, most),
        })
    };
    Ok(Preview {
        metadata: step(&guide.metadata, "metadata", 2)?,
        pins: step(&guide.pins, "pins", 4)?,
        package: step(&guide.package, "package", 3)?,
        footprint: step(&guide.footprint, "footprint", 2)?,
    })
}
