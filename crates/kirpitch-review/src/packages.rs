// SPDX-License-Identifier: GPL-3.0-or-later

//! The packages a datasheet talks about, found in its text without asking a
//! model: names like `LQFP64`, `UFBGA169` or `SOIC-8`. A datasheet for a family
//! lists several of them; the person says which one the part is for, and the
//! steps that follow read only that one.

use std::collections::BTreeMap;

use serde::Serialize;

/// One package named in the datasheet.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PackageChoice {
    /// The name, for example `LQFP64`.
    pub label: String,
    /// The package family, for example `LQFP`.
    pub family: String,
    /// The number of pins, when the name says it.
    pub pins: Option<u32>,
    /// How many times the name appears (more often: more likely the one).
    pub mentions: usize,
    /// The first page it is named on.
    pub page: u32,
}

/// The family names, the longest first so that `UFQFPN` is not read as `QFP`.
const FAMILIES: [&str; 40] = [
    "UFQFPN", "WFQFPN", "VFQFPN", "WLCSP", "VSSOP", "TSSOP", "UFBGA", "TFBGA", "LFBGA", "VFBGA",
    "LQFP", "TQFP", "VQFP", "PQFP", "VQFN", "WQFN", "UQFN", "UDFN", "WDFN", "WSON", "FBGA", "LBGA",
    "TBGA", "SSOP", "MSOP", "TSOP", "SOIC", "PDIP", "SDIP", "QFP", "QFN", "DFN", "SON", "BGA",
    "CSP", "LGA", "SOP", "SOT", "DIP", "SIP",
];
/// Families where the number is not a pin count (`TO-220`, `SOD-123`), kept as written.
const AS_WRITTEN: [&str; 3] = ["TO", "SOD", "SC"];

fn trim_word(w: &str) -> &str {
    w.trim_matches(|c: char| !c.is_ascii_alphanumeric())
}

/// Reads one word (and the word after it, for `LQFP 64`) as a package name.
fn package_of(word: &str, next: Option<&str>) -> Option<(String, String, Option<u32>)> {
    let up = trim_word(word).to_ascii_uppercase();
    if up.is_empty() {
        return None;
    }
    let (family, rest) = FAMILIES
        .iter()
        .find_map(|f| up.strip_prefix(f).map(|r| (*f, r)))
        .or_else(|| {
            AS_WRITTEN
                .iter()
                .find_map(|f| up.strip_prefix(f).map(|r| (*f, r)))
        })?;
    let as_written = AS_WRITTEN.contains(&family);
    let rest = rest.trim_start_matches('-');
    let (digits, spaced) = if rest.is_empty() {
        // `LQFP 64`: the number is the next word (not for `TO`, an ordinary word).
        if as_written {
            return None;
        }
        let n = trim_word(next?);
        (n.to_string(), true)
    } else {
        (rest.to_string(), false)
    };
    // The groups of digits: `64`, `23-5`.
    let groups: Vec<&str> = digits.split('-').collect();
    if groups.is_empty()
        || groups
            .iter()
            .any(|g| g.is_empty() || g.len() > 4 || !g.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    if spaced && groups.len() != 1 {
        return None;
    }
    let number = |g: &str| g.parse::<u32>().ok();
    let (label, pins) = match family {
        _ if as_written => (format!("{family}-{}", groups.join("-")), None),
        // SOT-23 has no pin count in its name; SOT-23-5 has.
        "SOT" => {
            let label = format!("SOT-{}", groups.join("-"));
            (
                label,
                if groups.len() > 1 {
                    number(groups[groups.len() - 1])
                } else {
                    None
                },
            )
        }
        _ => {
            let n = number(groups[0])?;
            (format!("{family}{n}"), Some(n))
        }
    };
    if let Some(n) = pins
        && !(2..=1500).contains(&n)
    {
        return None;
    }
    Some((label, family.to_string(), pins))
}

/// The packages named in the text of the pages (one string per page), the one
/// named most often first.
#[must_use]
pub fn scan_packages(pages: &[String]) -> Vec<PackageChoice> {
    let mut found: BTreeMap<String, PackageChoice> = BTreeMap::new();
    for (i, text) in pages.iter().enumerate() {
        let words: Vec<&str> = text.split_whitespace().collect();
        for (w, word) in words.iter().enumerate() {
            let Some((label, family, pins)) = package_of(word, words.get(w + 1).copied()) else {
                continue;
            };
            let page = u32::try_from(i + 1).unwrap_or(u32::MAX);
            found
                .entry(label.clone())
                .and_modify(|c| c.mentions += 1)
                .or_insert(PackageChoice {
                    label,
                    family,
                    pins,
                    mentions: 1,
                    page,
                });
        }
    }
    let mut list: Vec<PackageChoice> = found.into_values().collect();
    list.sort_by(|a, b| {
        b.mentions
            .cmp(&a.mentions)
            .then_with(|| a.page.cmp(&b.page))
            .then_with(|| a.label.cmp(&b.label))
    });
    list.truncate(10);
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(text: &str) -> Vec<(String, Option<u32>)> {
        scan_packages(&[text.to_string()])
            .into_iter()
            .map(|c| (c.label, c.pins))
            .collect()
    }

    #[test]
    fn names_are_found_however_they_are_written() {
        let got = labels("Package: LQFP64 (10 x 10 mm), also LQFP-100 and UFBGA 169, WLCSP90.");
        let names: Vec<&str> = got.iter().map(|(l, _)| l.as_str()).collect();
        for want in ["LQFP64", "LQFP100", "UFBGA169", "WLCSP90"] {
            assert!(names.contains(&want), "{want} in {names:?}");
        }
        assert!(got.contains(&("LQFP100".to_string(), Some(100))));
    }

    #[test]
    fn a_longer_family_is_not_taken_for_a_shorter_one() {
        let got = labels("UFQFPN48 and VQFN-32 and TSSOP20");
        let names: Vec<&str> = got.iter().map(|(l, _)| l.as_str()).collect();
        assert!(names.contains(&"UFQFPN48") && !names.contains(&"QFP48"));
        assert!(names.contains(&"VQFN32") && names.contains(&"TSSOP20"));
    }

    #[test]
    fn small_outline_and_transistor_packages() {
        let got = labels("SOT-23-5 SOT-23 TO-220 SOD-123 SOIC-8 SO");
        let find = |n: &str| got.iter().find(|(l, _)| l == n).map(|(_, p)| *p);
        assert_eq!(find("SOT-23-5"), Some(Some(5)));
        assert_eq!(find("SOT-23"), Some(None));
        assert_eq!(find("TO-220"), Some(None));
        assert_eq!(find("SOD-123"), Some(None));
        assert_eq!(find("SOIC8"), Some(Some(8)));
    }

    #[test]
    fn ordinary_words_are_not_packages() {
        assert!(labels("Go TO the top. 5 to 3.3 V. Size of 64 pins. CSP is a word").is_empty());
    }

    #[test]
    fn the_one_named_most_often_comes_first() {
        let pages = vec![
            "LQFP100 table".to_string(),
            "LQFP64 LQFP64 LQFP64 package".to_string(),
        ];
        let list = scan_packages(&pages);
        assert_eq!(list[0].label, "LQFP64");
        assert_eq!(list[0].mentions, 3);
        assert_eq!(list[0].page, 2);
        assert_eq!(list[1].label, "LQFP100");
    }
}
