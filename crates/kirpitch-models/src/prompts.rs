// SPDX-License-Identifier: GPL-3.0-or-later

//! The prompts of the extraction tasks, with their defaults.
//!
//! Each request to a model has a system prompt and, for each task, an
//! instruction that comes before the text of the pages. All of them can be
//! replaced in the `[prompts]` section of `kirpitch.toml` (or in the app, under
//! Settings, Extraction prompts). A prompt left out uses the default below.
//!
//! In the instructions of the tasks these placeholders are filled in:
//!
//! - `{types}`: the electrical types the model may use (pin table only);
//! - `{hint}`: the line `Hint: ...` when the person gave a hint, else nothing;
//! - `{pages}`: the text of the chosen pages. When an instruction has no
//!   `{pages}`, the pages are added at its end.
//!
//! The prompts take part in the cache key of the answers (the cache key is made
//! from the whole request), so changing a prompt never returns old answers.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The electrical types a pin may have, as shown to the model.
pub const TYPE_HELP: &str = "input, output, bidirectional, tri_state, passive, power_in (supply and ground inputs), power_out, open_collector, open_emitter, no_connect, free, unspecified";

/// Longest prompt accepted, in characters.
pub const MAX_PROMPT_CHARS: usize = 20_000;

/// The system prompt, sent with every request.
pub const DEFAULT_SYSTEM: &str = "You read electronic component datasheets and report what they say. \
Answer only with a JSON object shaped like the example you are given, without any explanation or reasoning. Use only what is written in the \
text; never guess and never use what you remember about the part. Leave a value out (null) \
when the text does not give it. The text shows page numbers as [page N].";

/// Part number, maker and description.
pub const DEFAULT_METADATA: &str =
    "Give the part number, the manufacturer and a one-line description of this device.\n\n{pages}";

/// The pin table.
pub const DEFAULT_PINS: &str = "List every pin of the package, one entry per pin number, with the pin number as printed, the pin name as printed, and the electrical type (one of: {types}). Ground and supply pins are power_in. If the datasheet shows several packages, use the package with the most common pin count unless the hint says otherwise.\n{hint}\n{pages}";

/// The second, differently worded reading of the pin table.
pub const DEFAULT_PINS_SECOND: &str = "From the pin description in the text below, make a table of the pin numbers, names and electrical types (one of: {types}). Every physical pin of the package must appear exactly once. Copy names exactly as printed.\n{hint}\n{pages}";

/// The package dimensions.
pub const DEFAULT_PACKAGE: &str = "Read the outline dimensions of the package in millimetres (convert inches; datasheets often print both). Use the JEDEC letters of the drawing and give each range as min and max (if only one value is printed, use it for both):\n\
- e: pitch; A: largest height\n\
- D: body length (along the pin rows) and E1: body width (across the rows), E: lead span tip to tip. These are for SOIC, SSOP, TSSOP, MSOP, SOT-23.\n\
- QFN: D and E are the two body sizes, D2 and E2 the exposed pad.\n\
- L: lead or terminal length; b: lead or terminal width.\n\
Leave out what the text does not give. Choose the family that fits the package name; when it is none of them (for example LQFP, TQFP, BGA, SOT-223) use `other` and write the package name in `package_name`.\n{hint}\n{pages}";

/// The prompts a person changed. `None` means the default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prompts {
    /// System prompt for every request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Instruction for the part number, maker and description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
    /// Instruction for the pin table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pins: Option<String>,
    /// Instruction for the second reading of the pin table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pins_second: Option<String>,
    /// Instruction for the package dimensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

/// A prompt that can be changed, for screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptInfo {
    /// Name in the configuration.
    pub key: &'static str,
    /// Short title.
    pub title: &'static str,
    /// What it does.
    pub about: &'static str,
    /// The placeholders it may use.
    pub placeholders: &'static str,
    /// The default text.
    pub default: &'static str,
}

/// All prompts that can be changed, in the order of the extraction.
pub const PROMPT_LIST: [PromptInfo; 5] = [
    PromptInfo {
        key: "system",
        title: "System prompt",
        about: "Sent with every request. The rules the model always follows.",
        placeholders: "",
        default: DEFAULT_SYSTEM,
    },
    PromptInfo {
        key: "metadata",
        title: "Part number, maker and description",
        about: "Asked of the first two pages.",
        placeholders: "{pages}",
        default: DEFAULT_METADATA,
    },
    PromptInfo {
        key: "pins",
        title: "Pin table",
        about: "Asked of the pages that describe the pins.",
        placeholders: "{types} {hint} {pages}",
        default: DEFAULT_PINS,
    },
    PromptInfo {
        key: "pins_second",
        title: "Pin table, second reading",
        about: "Only used when you ask for the pin table to be read twice.",
        placeholders: "{types} {hint} {pages}",
        default: DEFAULT_PINS_SECOND,
    },
    PromptInfo {
        key: "package",
        title: "Package dimensions",
        about: "Asked of the pages that describe the package.",
        placeholders: "{hint} {pages}",
        default: DEFAULT_PACKAGE,
    },
];

impl Prompts {
    /// The text of one prompt by its key: the changed text, else the default.
    pub fn get(&self, key: &str) -> &str {
        let (changed, default) = match key {
            "system" => (&self.system, DEFAULT_SYSTEM),
            "metadata" => (&self.metadata, DEFAULT_METADATA),
            "pins" => (&self.pins, DEFAULT_PINS),
            "pins_second" => (&self.pins_second, DEFAULT_PINS_SECOND),
            "package" => (&self.package, DEFAULT_PACKAGE),
            _ => return "",
        };
        changed.as_deref().unwrap_or(default)
    }

    /// Sets one prompt. Text equal to the default, or empty, means the default.
    ///
    /// # Errors
    ///
    /// The key is unknown or the text is too long.
    pub fn set(&mut self, key: &str, text: &str) -> Result<(), String> {
        let info = PROMPT_LIST
            .iter()
            .find(|p| p.key == key)
            .ok_or_else(|| format!("unknown prompt `{key}`"))?;
        if text.chars().count() > MAX_PROMPT_CHARS {
            return Err(format!(
                "the prompt `{}` is longer than {MAX_PROMPT_CHARS} characters",
                info.title
            ));
        }
        let value = (!text.trim().is_empty() && text.trim() != info.default.trim())
            .then(|| text.trim().to_string());
        match key {
            "system" => self.system = value,
            "metadata" => self.metadata = value,
            "pins" => self.pins = value,
            "pins_second" => self.pins_second = value,
            _ => self.package = value,
        }
        Ok(())
    }

    /// True when every prompt is the default.
    pub fn is_default(&self) -> bool {
        *self == Prompts::default()
    }

    /// A short code that names this set of prompts: the same texts always give
    /// the same code, and any change gives another. Shown next to scores so a
    /// score can be tied to the prompts that earned it.
    pub fn fingerprint(&self) -> String {
        let mut h = Sha256::new();
        for p in &PROMPT_LIST {
            h.update(p.key.as_bytes());
            h.update([0]);
            h.update(self.get(p.key).as_bytes());
            h.update([0]);
        }
        h.finalize()
            .iter()
            .take(4)
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// The keys of the prompts that differ from the default.
    pub fn changed(&self) -> Vec<&'static str> {
        PROMPT_LIST
            .iter()
            .filter(|p| self.get(p.key).trim() != p.default.trim())
            .map(|p| p.key)
            .collect()
    }
}

/// Fills in an instruction: `{types}`, `{hint}` (a line `Hint: ...` or
/// nothing) and `{pages}`. Pages are added at the end when the instruction
/// does not place them.
pub fn render(template: &str, hint: Option<&str>, pages: &str) -> String {
    let hint_line = hint.map(|h| format!("Hint: {h}\n")).unwrap_or_default();
    let mut text = template
        .replace("{types}", TYPE_HELP)
        .replace("{hint}", &hint_line);
    if text.contains("{pages}") {
        text = text.replace("{pages}", pages);
    } else {
        text.push_str("\n\n");
        text.push_str(pages);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_fingerprint_of_an_empty_set() {
        assert!(Prompts::default().is_default());
        assert_eq!(Prompts::default().fingerprint().len(), 8);
        assert!(Prompts::default().changed().is_empty());
    }

    #[test]
    fn a_change_gives_another_fingerprint_and_resetting_gives_it_back() {
        let mut p = Prompts::default();
        let before = p.fingerprint();
        p.set("pins", "List the pins.\n{pages}").unwrap();
        assert_ne!(p.fingerprint(), before);
        assert_eq!(p.changed(), ["pins"]);
        // The default text, or nothing, goes back to the default.
        p.set("pins", DEFAULT_PINS).unwrap();
        assert_eq!(p.fingerprint(), before);
        p.set("pins", "x").unwrap();
        p.set("pins", "   ").unwrap();
        assert!(p.is_default());
    }

    #[test]
    fn mistakes_are_named() {
        let mut p = Prompts::default();
        assert!(p.set("nope", "x").unwrap_err().contains("unknown prompt"));
        assert!(
            p.set("pins", &"a".repeat(MAX_PROMPT_CHARS + 1))
                .unwrap_err()
                .contains("longer")
        );
    }

    #[test]
    fn placeholders_are_filled_and_pages_are_added_when_missing() {
        let t = render("Types: {types}\n{hint}\n{pages}", Some("page 3"), "TEXT");
        assert!(t.contains("input, output"));
        assert!(t.contains("Hint: page 3"));
        assert!(t.ends_with("TEXT"));
        assert!(!t.contains('{'));
        let no_pages = render("Read it.", None, "TEXT");
        assert_eq!(no_pages, "Read it.\n\nTEXT");
        assert!(!render("{hint}x{pages}", None, "P").contains("Hint"));
    }
}
