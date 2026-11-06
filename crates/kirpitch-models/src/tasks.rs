// SPDX-License-Identifier: GPL-3.0-or-later

//! The questions Kirpitch asks a model, and the checks on its answers.
//!
//! Every answer is JSON that must follow a schema. An answer that does not is
//! sent back once with the problem named (automatic repair). Values are only
//! kept when the text of the datasheet backs them up, and a reading that the
//! text does not back up gets a low confidence, so the review stages show it.

use std::path::{Path, PathBuf};

use kirpitch_core::ElectricalType;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::client::{ChatRequest, Client, ModelError, Usage};

/// Changes when the prompts change, so old cached answers are not reused.
pub const PROMPT_VERSION: u32 = 1;

/// The text of one page and where its lines are.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLines {
    /// Page number counting from 1.
    pub page: u32,
    /// Lines top to bottom.
    pub lines: Vec<TextLine>,
}

/// One line of text with its place on the page (fractions of the page, from
/// the top left).
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    /// The text.
    pub text: String,
    /// Vertical centre.
    pub y: f64,
    /// Left edge.
    pub left: f64,
    /// Right edge.
    pub right: f64,
}

/// Half the height of a line of text, as a fraction of the page.
const HALF_LINE: f64 = 0.008;

impl PageLines {
    /// The page as text, with a `[page N]` heading.
    pub fn as_text(&self) -> String {
        let mut out = format!("[page {}]\n", self.page);
        for l in &self.lines {
            out.push_str(&l.text);
            out.push('\n');
        }
        out
    }

    fn score(&self, keywords: &[&str]) -> usize {
        let text = self.as_text().to_ascii_lowercase();
        keywords.iter().map(|k| text.matches(k).count()).sum()
    }
}

/// Picks the pages most likely to hold what a task needs, best first by score
/// but returned in page order, within a size limit.
pub fn select_pages<'a>(
    pages: &'a [PageLines],
    keywords: &[&str],
    max_pages: usize,
    max_chars: usize,
) -> Vec<&'a PageLines> {
    let mut scored: Vec<(usize, &PageLines)> =
        pages.iter().map(|p| (p.score(keywords), p)).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.page.cmp(&b.1.page)));
    let mut chosen: Vec<&PageLines> = Vec::new();
    let mut size = 0usize;
    for (score, page) in scored {
        if score == 0 || chosen.len() >= max_pages {
            break;
        }
        let len = page.as_text().len();
        if size + len > max_chars && !chosen.is_empty() {
            continue;
        }
        size += len;
        chosen.push(page);
    }
    chosen.sort_by_key(|p| p.page);
    chosen
}

fn pages_text(pages: &[&PageLines]) -> String {
    pages
        .iter()
        .map(|p| p.as_text())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Totals over a run.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stats {
    /// Questions asked (cached answers are not counted).
    pub calls: u32,
    /// Answers taken from the cache.
    pub cached: u32,
    /// Tokens used.
    pub usage: Usage,
    /// Seconds spent waiting for models.
    pub seconds: f64,
}

impl Stats {
    /// Adds another run's totals.
    pub fn add(&mut self, other: &Stats) {
        self.calls += other.calls;
        self.cached += other.cached;
        self.usage.input_tokens += other.usage.input_tokens;
        self.usage.output_tokens += other.usage.output_tokens;
        self.seconds += other.seconds;
    }

    /// Estimated cost from prices per million tokens.
    pub fn cost(&self, price_in: f64, price_out: f64) -> f64 {
        (self.usage.input_tokens as f64 * price_in + self.usage.output_tokens as f64 * price_out)
            / 1_000_000.0
    }
}

/// An optional folder where answers are remembered, so the same question to
/// the same model is not asked and paid for twice.
#[derive(Debug, Clone, Default)]
pub struct Cache {
    dir: Option<PathBuf>,
}

impl Cache {
    /// A cache in `dir` (created when first used).
    pub fn in_dir(dir: &Path) -> Cache {
        Cache {
            dir: Some(dir.to_path_buf()),
        }
    }

    /// No cache.
    pub fn none() -> Cache {
        Cache::default()
    }

    fn key(client: &Client, request: &ChatRequest) -> String {
        let mut h = Sha256::new();
        h.update(format!("{PROMPT_VERSION}\n{:?}\n", client.model()).as_bytes());
        h.update(request.system.as_bytes());
        h.update([0]);
        h.update(request.user.as_bytes());
        h.update([0]);
        for image in &request.images {
            h.update(image);
        }
        h.update(
            request
                .schema
                .as_ref()
                .map_or(String::new(), Value::to_string),
        );
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    fn get(&self, key: &str) -> Option<String> {
        let dir = self.dir.as_ref()?;
        std::fs::read_to_string(dir.join(format!("{key}.json"))).ok()
    }

    fn put(&self, key: &str, text: &str) {
        if let Some(dir) = &self.dir
            && std::fs::create_dir_all(dir).is_ok()
        {
            let _ = std::fs::write(dir.join(format!("{key}.json")), text);
        }
    }
}

/// Why a task gave no usable answer.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TaskError {
    /// The model could not be reached or refused.
    #[error("{0}")]
    Model(#[from] ModelError),
    /// The model answered twice without a usable answer.
    #[error("the model gave no usable answer: {0}")]
    Unusable(String),
}

/// Takes the JSON out of an answer, which may be wrapped in a code fence or
/// in sentences.
pub fn json_from_text(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Ok(v);
    }
    let start = trimmed
        .find('{')
        .ok_or("there is no JSON object in the answer")?;
    let end = trimmed
        .rfind('}')
        .ok_or("there is no JSON object in the answer")?;
    if end < start {
        return Err("there is no JSON object in the answer".into());
    }
    serde_json::from_str(&trimmed[start..=end]).map_err(|e| format!("the JSON is not valid: {e}"))
}

/// Asks, checks the answer with `check`, and asks again once with the problem
/// named when the check fails.
fn ask_json<T>(
    client: &Client,
    cache: &Cache,
    stats: &mut Stats,
    mut request: ChatRequest,
    check: impl Fn(&Value) -> Result<T, String>,
) -> Result<T, TaskError> {
    let mut last = String::new();
    for attempt in 0..2 {
        let key = Cache::key(client, &request);
        let (text, from_cache) = match cache.get(&key) {
            Some(t) => (t, true),
            None => {
                let r = match client.chat(&request) {
                    // A server that does not know JSON schemas: put the schema
                    // in the prompt and rely on the repair step.
                    Err(ModelError::Http {
                        status: 400 | 422, ..
                    }) if request.schema.is_some() => {
                        let schema = request.schema.take().unwrap_or_default();
                        request.user.push_str(&format!(
                            "

Answer with only a JSON object that follows this JSON schema:
{schema}"
                        ));
                        client.chat(&request)?
                    }
                    other => other?,
                };
                stats.calls += 1;
                stats.usage.input_tokens += r.usage.input_tokens;
                stats.usage.output_tokens += r.usage.output_tokens;
                stats.seconds += r.seconds;
                (r.text, false)
            }
        };
        match json_from_text(&text).and_then(|v| check(&v)) {
            Ok(value) => {
                if from_cache {
                    stats.cached += 1;
                } else {
                    cache.put(&key, &text);
                }
                return Ok(value);
            }
            Err(problem) => {
                last = problem.clone();
                if attempt == 0 {
                    request.user.push_str(&format!(
                        "\n\nYour previous answer was not usable: {problem}. Answer again with only the JSON object, following the schema."
                    ));
                }
            }
        }
    }
    Err(TaskError::Unusable(last))
}

const SYSTEM: &str = "You read electronic component datasheets and report what they say. \
Answer only with a JSON object that follows the schema. Use only what is written in the \
text; never guess and never use what you remember about the part. Leave a value out (null) \
when the text does not give it. The text shows page numbers as [page N].";

// ------------------------------------------------------------------ metadata

/// Part number, maker and description.
#[derive(Debug, Clone, PartialEq)]
pub struct MetadataAnswer {
    /// Manufacturer part number.
    pub mpn: String,
    /// Manufacturer.
    pub manufacturer: String,
    /// One-line description.
    pub description: String,
}

fn text_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Reads the part number, manufacturer and description.
pub fn extract_metadata(
    client: &Client,
    cache: &Cache,
    stats: &mut Stats,
    pages: &[PageLines],
) -> Result<MetadataAnswer, TaskError> {
    let first: Vec<&PageLines> = pages.iter().take(2).collect();
    let schema = json!({
        "type": "object",
        "properties": {
            "mpn": {"type": "string", "description": "the orderable part number of the main device"},
            "manufacturer": {"type": "string"},
            "description": {"type": "string", "description": "one line, for example 'Dual operational amplifier'"}
        },
        "required": ["mpn", "manufacturer", "description"]
    });
    let request = ChatRequest {
        system: SYSTEM.into(),
        user: format!(
            "Give the part number, the manufacturer and a one-line description of this device.\n\n{}",
            pages_text(&first)
        ),
        images: vec![],
        schema: Some(schema),
        max_tokens: 400,
    };
    ask_json(client, cache, stats, request, |v| {
        let get = |k: &str| text_field(v, k).ok_or_else(|| format!("`{k}` is missing"));
        Ok(MetadataAnswer {
            mpn: get("mpn")?,
            manufacturer: get("manufacturer")?,
            description: get("description")?,
        })
    })
}

// ---------------------------------------------------------------------- pins

/// One pin as the model read it.
#[derive(Debug, Clone, PartialEq)]
pub struct PinAnswer {
    /// Pin number as printed.
    pub number: String,
    /// Pin name.
    pub name: String,
    /// Electrical type.
    pub electrical_type: ElectricalType,
    /// Page the model says the pin is on.
    pub page: Option<u32>,
    /// The model's own confidence, 0 to 1.
    pub confidence: f64,
}

const TYPE_HELP: &str = "input, output, bidirectional, tri_state, passive, power_in (supply and ground inputs), power_out, open_collector, open_emitter, no_connect, free, unspecified";

fn string_or_number(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Reads the pins from a parsed answer, with the problems named for a repair.
pub fn pins_from_json(v: &Value) -> Result<Vec<PinAnswer>, String> {
    let list = v
        .get("pins")
        .and_then(Value::as_array)
        .ok_or("`pins` must be a list")?;
    if list.is_empty() {
        return Err("`pins` is empty".into());
    }
    let mut pins: Vec<PinAnswer> = Vec::new();
    for (i, item) in list.iter().enumerate() {
        let number = item
            .get("number")
            .and_then(string_or_number)
            .ok_or_else(|| format!("pins[{i}].number is missing"))?;
        let name = item
            .get("name")
            .and_then(string_or_number)
            .ok_or_else(|| format!("pins[{i}].name is missing"))?;
        let type_text = item
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("pins[{i}].type is missing"))?;
        let electrical_type: ElectricalType =
            serde_json::from_value(Value::String(type_text.trim().to_ascii_lowercase()))
                .map_err(|_| format!("pins[{i}].type `{type_text}` is not one of: {TYPE_HELP}"))?;
        if pins.iter().any(|p| p.number == number) {
            return Err(format!("pin number `{number}` appears twice"));
        }
        let page = item
            .get("page")
            .and_then(Value::as_u64)
            .and_then(|p| u32::try_from(p).ok())
            .filter(|p| *p >= 1);
        let confidence = item
            .get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(0.7)
            .clamp(0.0, 1.0);
        pins.push(PinAnswer {
            number,
            name,
            electrical_type,
            page,
            confidence,
        });
    }
    Ok(pins)
}

/// A pin with the place in the datasheet that backs it up.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadPin {
    /// The pin.
    pub pin: PinAnswer,
    /// Page of the line that holds the pin name (or the page the model named).
    pub page: u32,
    /// Box of that line, or the whole page when no line was found.
    pub bbox: [f64; 4],
    /// Confidence after the checks.
    pub confidence: f64,
    /// True when a line of the text holds both the number and the name.
    pub grounded: bool,
}

/// The result of reading the pins.
#[derive(Debug, Clone, PartialEq)]
pub struct PinsResult {
    /// The pins.
    pub pins: Vec<ReadPin>,
    /// What looked wrong, one line each.
    pub notes: Vec<String>,
}

fn contains_token(line: &str, token: &str) -> bool {
    let line = line.to_ascii_lowercase();
    let token = token.to_ascii_lowercase();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(pos) = line[from..].find(&token) {
        let start = from + pos;
        let end = start + token.len();
        let before_ok = line[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word(c));
        let after_ok = line[end..].chars().next().is_none_or(|c| !is_word(c));
        if before_ok && after_ok {
            return true;
        }
        from = start + token.len().max(1);
    }
    false
}

/// Finds the line that holds both the number and the name of a pin.
fn ground(pages: &[PageLines], pin: &PinAnswer) -> Option<(u32, [f64; 4])> {
    // The name is compared without the KiCad overbar notation `~{...}`.
    let name = pin.name.replace("~{", "").replace('}', "");
    for page in pages {
        for l in &page.lines {
            if contains_token(&l.text, &pin.number) && contains_token(&l.text, &name) {
                return Some((
                    page.page,
                    [
                        l.left,
                        (l.y - HALF_LINE).max(0.0),
                        l.right,
                        (l.y + HALF_LINE).min(1.0),
                    ],
                ));
            }
        }
    }
    None
}

/// Confidence of a reading the text does not back up.
pub const UNGROUNDED_CONFIDENCE: f64 = 0.3;
/// Confidence of a pin where two readings disagree.
pub const DISAGREE_CONFIDENCE: f64 = 0.4;

/// Reads the pin table. `hint` can say where to look, for example "8 pins,
/// SOIC package" or "the pinout is on page 4".
pub fn extract_pins(
    client: &Client,
    cache: &Cache,
    stats: &mut Stats,
    pages: &[PageLines],
    hint: Option<&str>,
    second_read: bool,
) -> Result<PinsResult, TaskError> {
    let keywords = [
        "pin",
        "pinout",
        "pin configuration",
        "pin description",
        "terminal",
    ];
    let chosen = select_pages(pages, &keywords, 4, 24_000);
    if chosen.is_empty() {
        return Err(TaskError::Unusable(
            "no page of the datasheet text mentions pins".into(),
        ));
    }
    let owned: Vec<PageLines> = chosen.iter().map(|p| (*p).clone()).collect();
    let schema = json!({
        "type": "object",
        "properties": {
            "pins": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "number": {"type": "string", "description": "pin number as printed, for example 1 or A1"},
                        "name": {"type": "string", "description": "pin name as printed"},
                        "type": {"type": "string", "enum": ["input", "output", "bidirectional", "tri_state", "passive", "power_in", "power_out", "open_collector", "open_emitter", "no_connect", "free", "unspecified"]},
                        "page": {"type": "integer"},
                        "confidence": {"type": "number", "description": "0 to 1"}
                    },
                    "required": ["number", "name", "type"]
                }
            }
        },
        "required": ["pins"]
    });
    let hint_text = hint.map(|h| format!("Hint: {h}\n")).unwrap_or_default();
    let ask = |variant: u8| ChatRequest {
        system: SYSTEM.into(),
        user: if variant == 0 {
            format!(
                "List every pin of the package, one entry per pin number, with the pin number as printed, the pin name as printed, and the electrical type (one of: {TYPE_HELP}). Ground and supply pins are power_in. If the datasheet shows several packages, use the package with the most common pin count unless the hint says otherwise.\n{hint_text}\n{}",
                pages_text(&chosen)
            )
        } else {
            format!(
                "From the pin description in the text below, make a table of the pin numbers, names and electrical types (one of: {TYPE_HELP}). Every physical pin of the package must appear exactly once. Copy names exactly as printed.\n{hint_text}\n{}",
                pages_text(&chosen)
            )
        },
        images: vec![],
        schema: Some(schema.clone()),
        max_tokens: 4000,
    };
    let first = ask_json(client, cache, stats, ask(0), pins_from_json)?;
    let mut notes = Vec::new();
    let second = if second_read {
        match ask_json(client, cache, stats, ask(1), pins_from_json) {
            Ok(p) => Some(p),
            Err(e) => {
                notes.push(format!("the second reading failed: {e}"));
                None
            }
        }
    } else {
        None
    };
    let default_page = owned.first().map_or(1, |p| p.page);
    let mut pins = Vec::new();
    for pin in first {
        let mut confidence = pin.confidence;
        let found = ground(&owned, &pin);
        let grounded = found.is_some();
        let (page, bbox) =
            found.unwrap_or((pin.page.unwrap_or(default_page), [0.0, 0.0, 1.0, 1.0]));
        if !grounded {
            confidence = confidence.min(UNGROUNDED_CONFIDENCE);
            notes.push(format!(
                "pin {} `{}` is not on one line of the text (check it on page {page})",
                pin.number, pin.name
            ));
        }
        if let Some(other) = &second {
            match other.iter().find(|o| o.number == pin.number) {
                Some(o) if o.name == pin.name && o.electrical_type == pin.electrical_type => {}
                Some(o) => {
                    confidence = confidence.min(DISAGREE_CONFIDENCE);
                    notes.push(format!(
                        "the two readings disagree on pin {}: `{}` ({}) and `{}` ({})",
                        pin.number,
                        pin.name,
                        pin.electrical_type.as_str(),
                        o.name,
                        o.electrical_type.as_str()
                    ));
                }
                None => {
                    confidence = confidence.min(DISAGREE_CONFIDENCE);
                    notes.push(format!("the second reading has no pin {}", pin.number));
                }
            }
        }
        pins.push(ReadPin {
            pin,
            page,
            bbox,
            confidence,
            grounded,
        });
    }
    if let Some(other) = &second {
        for o in other {
            if !pins.iter().any(|p| p.pin.number == o.number) {
                notes.push(format!(
                    "the second reading has pin {} `{}`, which the first does not",
                    o.number, o.name
                ));
            }
        }
    }
    Ok(PinsResult { pins, notes })
}

// ------------------------------------------------------------------- package

/// A range of values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    /// Smallest.
    pub min: f64,
    /// Largest.
    pub max: f64,
}

/// The package dimensions as the model read them, in millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageAnswer {
    /// `soic`, `ssop`, `tssop`, `msop`, `sot23`, `dip` or `qfn`.
    pub family: String,
    /// Number of terminals of a dual-row package.
    pub pin_count: Option<u32>,
    /// Terminals on the top and bottom sides of a QFN.
    pub pins_x: Option<u32>,
    /// Terminals on the left and right sides of a QFN.
    pub pins_y: Option<u32>,
    /// Pitch.
    pub pitch: f64,
    /// Body length (along the pin rows).
    pub body_length: Option<f64>,
    /// Body width (across the pin rows).
    pub body_width: Option<f64>,
    /// Largest height.
    pub height_max: f64,
    /// Lead span, tip to tip (gull-wing packages).
    pub lead_span: Option<Span>,
    /// Lead or terminal length.
    pub lead_length: Option<Span>,
    /// Lead or terminal width.
    pub lead_width: Option<Span>,
    /// Body size along x (QFN).
    pub body_x: Option<Span>,
    /// Body size along y (QFN).
    pub body_y: Option<Span>,
    /// Exposed pad size (QFN).
    pub exposed_pad: Option<(f64, f64)>,
    /// Distance between the rows of a DIP.
    pub row_spacing: Option<f64>,
    /// Drill diameter of a DIP.
    pub drill: Option<f64>,
    /// Pad diameter of a DIP.
    pub pad_diameter: Option<f64>,
    /// Page of the package drawing.
    pub page: Option<u32>,
    /// The model's confidence.
    pub confidence: f64,
}

fn number(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64).filter(|n| n.is_finite())
}

fn span(v: &Value, key: &str) -> Result<Option<Span>, String> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(s) => {
            let min = number(s, "min").ok_or_else(|| format!("`{key}.min` is missing"))?;
            let max = number(s, "max").ok_or_else(|| format!("`{key}.max` is missing"))?;
            if min > max || min <= 0.0 {
                return Err(format!("`{key}` must have 0 < min <= max"));
            }
            Ok(Some(Span { min, max }))
        }
    }
}

fn count(v: &Value, key: &str) -> Option<u32> {
    v.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
}

/// Reads the package answer, checking that the family has what it needs.
pub fn package_from_json(v: &Value) -> Result<PackageAnswer, String> {
    let family = text_field(v, "family")
        .ok_or("`family` is missing")?
        .to_ascii_lowercase()
        .replace(['-', ' '], "");
    if !["soic", "ssop", "tssop", "msop", "sot23", "dip", "qfn"].contains(&family.as_str()) {
        return Err(format!(
            "`family` `{family}` is not one of soic, ssop, tssop, msop, sot23, dip, qfn"
        ));
    }
    let pitch = number(v, "pitch")
        .filter(|p| *p > 0.0)
        .ok_or("`pitch` is missing")?;
    let height_max = number(v, "height_max")
        .filter(|p| *p > 0.0)
        .ok_or("`height_max` is missing")?;
    let answer = PackageAnswer {
        pin_count: count(v, "pin_count"),
        pins_x: count(v, "pins_x"),
        pins_y: count(v, "pins_y"),
        pitch,
        body_length: number(v, "body_length").filter(|n| *n > 0.0),
        body_width: number(v, "body_width").filter(|n| *n > 0.0),
        height_max,
        lead_span: span(v, "lead_span")?,
        lead_length: span(v, "lead_length")?,
        lead_width: span(v, "lead_width")?,
        body_x: span(v, "body_x")?,
        body_y: span(v, "body_y")?,
        exposed_pad: match v.get("exposed_pad") {
            Some(Value::Object(_)) => {
                let p = &v["exposed_pad"];
                let w = number(p, "width").filter(|n| *n > 0.0);
                let h = number(p, "height").filter(|n| *n > 0.0);
                match (w, h) {
                    (Some(w), Some(h)) => Some((w, h)),
                    _ => return Err("`exposed_pad` needs `width` and `height`".into()),
                }
            }
            _ => None,
        },
        row_spacing: number(v, "row_spacing").filter(|n| *n > 0.0),
        drill: number(v, "drill").filter(|n| *n > 0.0),
        pad_diameter: number(v, "pad_diameter").filter(|n| *n > 0.0),
        page: v
            .get("page")
            .and_then(Value::as_u64)
            .and_then(|p| u32::try_from(p).ok())
            .filter(|p| *p >= 1),
        confidence: number(v, "confidence").unwrap_or(0.7).clamp(0.0, 1.0),
        family,
    };
    let need = |ok: bool, what: &str| -> Result<(), String> {
        if ok {
            Ok(())
        } else {
            Err(format!(
                "`{what}` is missing for a {} package",
                answer.family
            ))
        }
    };
    match answer.family.as_str() {
        "qfn" => {
            need(answer.pins_x.is_some(), "pins_x")?;
            need(answer.pins_y.is_some(), "pins_y")?;
            need(answer.body_x.is_some(), "body_x")?;
            need(answer.body_y.is_some(), "body_y")?;
            need(answer.lead_length.is_some(), "lead_length")?;
            need(answer.lead_width.is_some(), "lead_width")?;
        }
        "dip" => {
            need(answer.pin_count.is_some(), "pin_count")?;
            need(answer.body_length.is_some(), "body_length")?;
            need(answer.body_width.is_some(), "body_width")?;
            need(answer.row_spacing.is_some(), "row_spacing")?;
        }
        _ => {
            need(answer.pin_count.is_some(), "pin_count")?;
            need(answer.body_length.is_some(), "body_length")?;
            need(answer.body_width.is_some(), "body_width")?;
            need(answer.lead_span.is_some(), "lead_span")?;
            need(answer.lead_length.is_some(), "lead_length")?;
            need(answer.lead_width.is_some(), "lead_width")?;
        }
    }
    Ok(answer)
}

/// Reads the package family and its dimensions.
pub fn extract_package(
    client: &Client,
    cache: &Cache,
    stats: &mut Stats,
    pages: &[PageLines],
    hint: Option<&str>,
) -> Result<PackageAnswer, TaskError> {
    let keywords = [
        "package",
        "dimension",
        "outline",
        "millimeter",
        "mm",
        "pitch",
        "soic",
        "tssop",
        "qfn",
        "sot",
        "dip",
    ];
    let chosen = select_pages(pages, &keywords, 4, 24_000);
    if chosen.is_empty() {
        return Err(TaskError::Unusable(
            "no page of the datasheet text describes the package".into(),
        ));
    }
    let range = json!({"type": "object", "properties": {"min": {"type": "number"}, "max": {"type": "number"}}, "required": ["min", "max"]});
    let schema = json!({
        "type": "object",
        "properties": {
            "family": {"type": "string", "enum": ["soic", "ssop", "tssop", "msop", "sot23", "dip", "qfn"]},
            "pin_count": {"type": "integer", "description": "terminals, not counting an exposed pad"},
            "pins_x": {"type": "integer", "description": "QFN: terminals on each of top and bottom"},
            "pins_y": {"type": "integer", "description": "QFN: terminals on each of left and right"},
            "pitch": {"type": "number", "description": "mm, e"},
            "body_length": {"type": "number", "description": "mm, D, along the pin rows (dual-row packages)"},
            "body_width": {"type": "number", "description": "mm, E1, across the pin rows (dual-row packages)"},
            "height_max": {"type": "number", "description": "mm, A, largest"},
            "lead_span": range,
            "lead_length": range,
            "lead_width": range,
            "body_x": range,
            "body_y": range,
            "exposed_pad": {"type": ["object", "null"], "properties": {"width": {"type": "number"}, "height": {"type": "number"}}},
            "row_spacing": {"type": "number", "description": "mm, distance between the rows of a DIP"},
            "drill": {"type": "number"},
            "pad_diameter": {"type": "number"},
            "page": {"type": "integer"},
            "confidence": {"type": "number"}
        },
        "required": ["family", "pitch", "height_max"]
    });
    let hint_text = hint.map(|h| format!("Hint: {h}\n")).unwrap_or_default();
    let request = ChatRequest {
        system: SYSTEM.into(),
        user: format!(
            "Read the package outline dimensions in millimetres (convert inches). Use the JEDEC letters: A height, b lead width, D and E body sizes, E1 body width of dual-row packages, e pitch, L lead length, and the lead span (E) tip to tip for gull-wing packages. Give ranges as min and max; if only a nominal is printed, use it for both. Choose the family that fits the package name.\n{hint_text}\n{}",
            pages_text(&chosen)
        ),
        images: vec![],
        schema: Some(schema),
        max_tokens: 1500,
    };
    ask_json(client, cache, stats, request, package_from_json)
}
