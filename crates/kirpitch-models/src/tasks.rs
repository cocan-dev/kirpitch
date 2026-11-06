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

    /// Weighted count of the keywords. Each keyword counts at most five times,
    /// so one repeated word cannot outweigh a page with many different ones.
    fn score(&self, keywords: &[(&str, i32)]) -> i32 {
        let text = self.as_text().to_ascii_lowercase();
        keywords
            .iter()
            .map(|(k, weight)| weight * text.matches(k).count().min(5) as i32)
            .sum()
    }
}

/// Words of a hint that help to find pages: package names and drawing codes.
fn hint_words(hint: Option<&str>) -> Vec<String> {
    const SKIP: [&str; 10] = [
        "pins", "package", "with", "page", "the", "this", "that", "from", "type", "drawing",
    ];
    hint.unwrap_or("")
        .to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|w| {
            w.len() >= 3
                && !SKIP.contains(w)
                && (w.len() >= 4 || w.chars().any(|c| c.is_ascii_digit()))
        })
        .map(str::to_string)
        .collect()
}

/// Like [`select_pages`], with the words of a hint counting extra.
fn select_with_hint<'a>(
    pages: &'a [PageLines],
    keywords: &[(&str, i32)],
    hint: Option<&str>,
    hint_weight: i32,
    max_pages: usize,
    max_chars: usize,
) -> Vec<&'a PageLines> {
    // A word of the hint is worth more the fewer pages have it: the name of
    // the maker is on every page and tells nothing, a drawing code is on one.
    let n = pages.len().max(1);
    let texts: Vec<String> = pages
        .iter()
        .map(|p| p.as_text().to_ascii_lowercase())
        .collect();
    let words: Vec<(String, i32)> = hint_words(hint)
        .into_iter()
        .filter_map(|w| {
            let df = texts.iter().filter(|t| t.contains(&w)).count();
            if df == 0 || df * 4 > n {
                return None;
            }
            Some((w, (hint_weight * (n - df) as i32 / n as i32).max(1)))
        })
        .collect();
    let mut all: Vec<(&str, i32)> = keywords.to_vec();
    all.extend(words.iter().map(|(w, weight)| (w.as_str(), *weight)));
    let chosen = select_pages(pages, &all, max_pages, max_chars);
    if std::env::var_os(crate::client::ENV_DEBUG).is_some() {
        let numbers: Vec<u32> = chosen.iter().map(|p| p.page).collect();
        let scores: Vec<(u32, i32)> = pages.iter().map(|p| (p.page, p.score(&all))).collect();
        let mut best = scores.clone();
        best.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
        best.truncate(6);
        eprintln!(
            "[kirpitch debug] pages chosen {numbers:?}; best scores {best:?}; hint words {words:?}"
        );
    }
    chosen
}

/// Picks the pages most likely to hold what a task needs, best first by score
/// but returned in page order, within a size limit.
pub fn select_pages<'a>(
    pages: &'a [PageLines],
    keywords: &[(&str, i32)],
    max_pages: usize,
    max_chars: usize,
) -> Vec<&'a PageLines> {
    let mut scored: Vec<(i32, &PageLines)> = pages.iter().map(|p| (p.score(keywords), p)).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.page.cmp(&b.1.page)));
    let mut chosen: Vec<&PageLines> = Vec::new();
    let mut size = 0usize;
    for (score, page) in scored {
        if score <= 0 || chosen.len() >= max_pages {
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
    // Models sometimes write sentences before and after the JSON: take the
    // first object that parses, wherever it starts.
    let mut problem = "there is no JSON object in the answer".to_string();
    for (start, _) in trimmed.match_indices('{') {
        let mut values = serde_json::Deserializer::from_str(&trimmed[start..]).into_iter::<Value>();
        match values.next() {
            Some(Ok(v)) if v.is_object() => return Ok(v),
            Some(Err(e)) => problem = format!("the JSON is not valid: {e}"),
            _ => {}
        }
    }
    Err(problem)
}

/// Models sometimes put the answer inside a wrapper (`properties`, `answer`,
/// `result`, ...), most often after seeing a schema. The answer itself is
/// checked first, then what is inside such a wrapper.
fn check_wrapped<T>(v: &Value, check: &impl Fn(&Value) -> Result<T, String>) -> Result<T, String> {
    let first = match check(v) {
        Ok(x) => return Ok(x),
        Err(e) => e,
    };
    for key in [
        "properties",
        "answer",
        "result",
        "data",
        "output",
        "response",
    ] {
        if let Some(inner) = v.get(key).filter(|x| x.is_object())
            && let Ok(x) = check(inner)
        {
            return Ok(x);
        }
    }
    Err(first)
}

/// Asks, checks the answer with `check`, and asks again (up to twice) with the
/// problem named when the check fails.
fn ask_json<T>(
    client: &Client,
    cache: &Cache,
    stats: &mut Stats,
    mut request: ChatRequest,
    check: impl Fn(&Value) -> Result<T, String>,
) -> Result<T, TaskError> {
    // Not every server enforces the schema (Ollama models that run in the
    // cloud do not), so the prompt always carries it as well.
    if let Some(example) = &request.example {
        request.user.push_str(&format!(
            "

Answer with only a JSON object shaped like this example. The values in it only show the shape: take the real values from the datasheet text and never copy the example's values. Do not write the schema or any field descriptions into the answer.
{example}"
        ));
    } else if let Some(schema) = &request.schema {
        request.user.push_str(&format!(
            "

Answer with only a JSON object that follows this JSON schema, with these exact field names:
{schema}"
        ));
    }
    let mut truncated = false;
    let mut last = String::new();
    let mut began = String::new();
    for attempt in 0..3 {
        let key = Cache::key(client, &request);
        let (text, from_cache) = match cache.get(&key) {
            Some(t) => (t, true),
            None => {
                let r = match client.chat(&request) {
                    // A server that does not know JSON schemas: the schema is
                    // in the prompt already, so ask again without it and rely
                    // on the repair step.
                    Err(ModelError::Http {
                        status: 400 | 422, ..
                    }) if request.schema.is_some() => {
                        request.schema = None;
                        client.chat(&request)?
                    }
                    other => other?,
                };
                stats.calls += 1;
                stats.usage.input_tokens += r.usage.input_tokens;
                stats.usage.output_tokens += r.usage.output_tokens;
                stats.seconds += r.seconds;
                truncated = r.truncated;
                (r.text, false)
            }
        };
        match json_from_text(&text).and_then(|v| check_wrapped(&v, &check)) {
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
                began = text
                    .chars()
                    .take(160)
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
                if attempt < 2 {
                    if truncated {
                        // The answer was cut off at the limit: allow much more.
                        request.max_tokens = (request.max_tokens * 4).min(32_000);
                    }
                    if text.trim().is_empty() {
                        // Nothing came back: a model that thinks used its whole
                        // budget on it. Turn thinking off where that can be
                        // done, and allow more room.
                        request.no_think = true;
                        request.max_tokens = (request.max_tokens * 2).min(32_000);
                    }
                    request.user.push_str(&format!(
                        "\n\nYour previous answer was not usable: {problem}. Answer again with only the JSON object, shaped like the example, and do not write your reasoning."
                    ));
                }
            }
        }
    }
    Err(TaskError::Unusable(format!(
        "{last} (the answer began: {began})"
    )))
}

const SYSTEM: &str = "You read electronic component datasheets and report what they say. \
Answer only with a JSON object that follows the schema, without any explanation or reasoning. Use only what is written in the \
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
        example: Some(
            json!({"mpn": "LM358DR", "manufacturer": "Texas Instruments", "description": "Dual operational amplifier"}),
        ),
        max_tokens: 2000,
        ..Default::default()
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

/// Replaces the minus signs and dashes that PDFs use by the plain hyphen, so
/// that `IN1−` and `IN1-` are the same name.
pub fn plain_dashes(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '‐' | '‑' | '‒' | '–' | '—' | '−' => '-',
            c => c,
        })
        .collect()
}

fn string_or_number(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(plain_dashes(s.trim())).filter(|s| !s.is_empty()),
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
            let text = plain_dashes(&l.text);
            if contains_token(&text, &pin.number) && contains_token(&text, &name) {
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
        ("pin functions", 8),
        ("pin configuration", 8),
        ("pin description", 8),
        ("pinout", 4),
        ("pin", 1),
        ("terminal", 1),
        ("tape and reel", -6),
        ("orderable", -3),
    ];
    let chosen = select_with_hint(pages, &keywords, hint, 3, 4, 24_000);
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
        example: Some(json!({"pins": [
            {"number": "1", "name": "OUT1", "type": "output", "page": 3, "confidence": 0.9},
            {"number": "4", "name": "GND", "type": "power_in", "page": 3, "confidence": 0.9}
        ]})),
        max_tokens: 8000,
        ..Default::default()
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
    /// The range that `body_length` was taken from (`D`).
    pub body_length_range: Option<Span>,
    /// The range that `body_width` was taken from (`E1`).
    pub body_width_range: Option<Span>,
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

fn nominal(s: Span) -> f64 {
    (s.min + s.max) / 2.0
}

/// The body size for the package name and outline: the middle of the range,
/// rounded to 0.05 mm like the names in the KiCad libraries (a range of
/// 3.81 to 3.98 gives 3.9).
fn body_size(s: Span) -> f64 {
    (nominal(s) / 0.05).round() * 0.05
}

/// Reads the package answer, which uses the JEDEC letters of the drawing:
/// `e` pitch, `A` height, `D` and `E` body sizes (QFN) or `D` body length,
/// `E1` body width and `E` lead span (dual-row packages), `L` and `b` lead
/// length and width, `D2`/`E2` exposed pad. It checks that the family has what
/// it needs.
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
    let pitch = number(v, "e")
        .filter(|p| *p > 0.0)
        .ok_or("`e` (the pitch in mm) is missing")?;
    let height_max = number(v, "A")
        .filter(|p| *p > 0.0)
        .ok_or("`A` (the largest height in mm) is missing")?;
    let (d, e, e1) = (span(v, "D")?, span(v, "E")?, span(v, "E1")?);
    let (l, b) = (span(v, "L")?, span(v, "b")?);
    let pin_count = count(v, "pin_count");
    let mut answer = PackageAnswer {
        family: family.clone(),
        pin_count,
        pins_x: count(v, "pins_x"),
        pins_y: count(v, "pins_y"),
        pitch,
        body_length: None,
        body_width: None,
        body_length_range: None,
        body_width_range: None,
        height_max,
        lead_span: None,
        lead_length: l,
        lead_width: b,
        body_x: None,
        body_y: None,
        exposed_pad: None,
        row_spacing: number(v, "row_spacing").filter(|n| *n > 0.0),
        drill: number(v, "drill").filter(|n| *n > 0.0),
        pad_diameter: number(v, "pad_diameter").filter(|n| *n > 0.0),
        page: v
            .get("page")
            .and_then(Value::as_u64)
            .and_then(|p| u32::try_from(p).ok())
            .filter(|p| *p >= 1),
        confidence: number(v, "confidence").unwrap_or(0.7).clamp(0.0, 1.0),
    };
    let need = |ok: bool, what: &str| -> Result<(), String> {
        if ok {
            Ok(())
        } else {
            Err(format!("`{what}` is missing for a {family} package"))
        }
    };
    match family.as_str() {
        "qfn" => {
            answer.body_x = d;
            answer.body_y = e;
            if answer.pins_x.is_none() && answer.pins_y.is_none() {
                // A square package has the same number on every side.
                if let Some(n) = pin_count.filter(|n| n % 4 == 0) {
                    answer.pins_x = Some(n / 4);
                    answer.pins_y = Some(n / 4);
                }
            }
            if let (Some(w), Some(h)) = (number(v, "D2"), number(v, "E2"))
                && w > 0.0
                && h > 0.0
            {
                answer.exposed_pad = Some((w, h));
            }
            need(answer.pins_x.is_some(), "pin_count (or pins_x and pins_y)")?;
            need(answer.pins_y.is_some(), "pin_count (or pins_x and pins_y)")?;
            need(answer.body_x.is_some(), "D")?;
            need(answer.body_y.is_some(), "E")?;
            need(answer.lead_length.is_some(), "L")?;
            need(answer.lead_width.is_some(), "b")?;
        }
        "dip" => {
            answer.body_length = d.map(body_size);
            answer.body_width = e1.map(body_size);
            answer.body_length_range = d;
            answer.body_width_range = e1;
            need(pin_count.is_some(), "pin_count")?;
            need(answer.body_length.is_some(), "D")?;
            need(answer.body_width.is_some(), "E1")?;
            need(answer.row_spacing.is_some(), "row_spacing")?;
        }
        _ => {
            answer.body_length = d.map(body_size);
            answer.body_width = e1.map(body_size);
            answer.body_length_range = d;
            answer.body_width_range = e1;
            answer.lead_span = e;
            need(pin_count.is_some(), "pin_count")?;
            need(answer.body_length.is_some(), "D")?;
            need(answer.body_width.is_some(), "E1")?;
            need(answer.lead_span.is_some(), "E")?;
            need(answer.lead_length.is_some(), "L")?;
            need(answer.lead_width.is_some(), "b")?;
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
        ("package outline", 6),
        ("mechanical data", 4),
        ("seating plane", 6),
        ("dimensions are in millimeters", 8),
        ("dimensions are in", 3),
        ("typ", 1),
        ("bsc", 2),
        ("max", 1),
        ("min", 1),
        ("pitch", 2),
        ("tape", -4),
        ("reel", -4),
        ("dimension limits", 6),
        ("millimeters", 2),
        ("product identification", -10),
        ("revision history", -10),
        ("ordering information", -5),
        ("orderable", -5),
    ];
    let chosen = select_with_hint(pages, &keywords, hint, 8, 3, 16_000);
    if chosen.is_empty() {
        return Err(TaskError::Unusable(
            "no page of the datasheet text describes the package".into(),
        ));
    }
    let range = json!({
        "type": "object",
        "properties": {"min": {"type": "number"}, "max": {"type": "number"}},
        "required": ["min", "max"]
    });
    let schema = json!({
        "type": "object",
        "properties": {
            "family": {"type": "string", "enum": ["soic", "ssop", "tssop", "msop", "sot23", "dip", "qfn"]},
            "pin_count": {"type": "integer", "description": "number of terminals, not counting an exposed pad"},
            "e": {"type": "number", "description": "pitch in mm"},
            "A": {"type": "number", "description": "largest height in mm"},
            "D": range.clone(),
            "E": range.clone(),
            "E1": range.clone(),
            "L": range.clone(),
            "b": range,
            "D2": {"type": "number", "description": "QFN exposed pad width in mm, if any"},
            "E2": {"type": "number", "description": "QFN exposed pad height in mm, if any"},
            "row_spacing": {"type": "number", "description": "DIP: distance between the two rows of pins in mm"},
            "page": {"type": "integer"},
            "confidence": {"type": "number", "description": "0 to 1"}
        },
        "required": ["family", "pin_count", "e", "A"]
    });
    let hint_text = hint.map(|h| format!("Hint: {h}\n")).unwrap_or_default();
    let request = ChatRequest {
        system: SYSTEM.into(),
        user: format!(
            "Read the outline dimensions of the package in millimetres (convert inches; datasheets often print both). Use the JEDEC letters of the drawing and give each range as min and max (if only one value is printed, use it for both):\n\
- e: pitch; A: largest height\n\
- D: body length (along the pin rows) and E1: body width (across the rows), E: lead span tip to tip. These are for SOIC, SSOP, TSSOP, MSOP, SOT-23.\n\
- QFN: D and E are the two body sizes, D2 and E2 the exposed pad.\n\
- L: lead or terminal length; b: lead or terminal width.\n\
Leave out what the text does not give. Choose the family that fits the package name.\n{hint_text}\n{}",
            pages_text(&chosen)
        ),
        images: vec![],
        schema: Some(schema),
        example: Some(json!({
            "family": "soic", "pin_count": 8, "e": 1.27, "A": 1.75,
            "D": {"min": 4.8, "max": 5.0}, "E1": {"min": 3.8, "max": 4.0}, "E": {"min": 5.8, "max": 6.2},
            "L": {"min": 0.4, "max": 1.27}, "b": {"min": 0.31, "max": 0.51},
            "page": 12, "confidence": 0.8
        })),
        max_tokens: 4000,
        ..Default::default()
    };
    ask_json(client, cache, stats, request, package_from_json)
}
