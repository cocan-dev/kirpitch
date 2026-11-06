// SPDX-License-Identifier: GPL-3.0-or-later

//! Development helper for Kirpitch: `cargo xtask <command>`.
//!
//! Commands:
//! - `fetch-datasheets`: download the datasheet of every golden part into
//!   `testdata/pdfs/` (ignored by git) and check its SHA-256 hash.
//! - `verify-datasheets`: download every datasheet again into a temporary file
//!   and report parts whose hash differs from the recorded one ("drift").
//!
//! Datasheets are copyrighted and are never committed (CONTRIBUTING.md). A
//! changed hash is drift, not a test failure (ADR 0013): exit code 3 tells
//! scripts about it. Downloads use the system `curl` tool, which Windows 10 and
//! later, macOS and Linux provide, so no HTTP library is needed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use sha2::{Digest, Sha256};

/// Exit code when everything failed to run (network, files).
const EXIT_ERROR: u8 = 1;
/// Exit code when a datasheet changed since its hash was recorded.
const EXIT_DRIFT: u8 = 3;

/// A datasheet listed in a golden part's `datasheet.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Datasheet {
    /// Name of the golden folder, also used as the PDF file name.
    name: String,
    url: String,
    sha256: String,
}

/// What was found for one datasheet.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    /// The file has the recorded hash.
    Match,
    /// The file has another hash: the manufacturer changed the PDF.
    Drift { actual: String },
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = repo_root();
    let result = match args.first().map(String::as_str) {
        Some("fetch-datasheets") => fetch(&root),
        Some("verify-datasheets") => verify(&root),
        _ => {
            eprintln!("usage: cargo xtask <fetch-datasheets | verify-datasheets>");
            return ExitCode::from(EXIT_ERROR);
        }
    };
    match result {
        Ok(drift) if drift > 0 => {
            eprintln!(
                "{drift} datasheet(s) changed. Re-read the pages named in datasheet.json, \
                 update partspec.json and the hash, then run the tests."
            );
            ExitCode::from(EXIT_DRIFT)
        }
        Ok(_) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(EXIT_ERROR)
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the repository")
        .to_path_buf()
}

/// Reads all golden datasheet manifests, sorted by folder name.
fn read_datasheets(root: &Path) -> Result<Vec<Datasheet>, String> {
    let golden = root.join("testdata/golden");
    let mut out = Vec::new();
    let entries = fs::read_dir(&golden).map_err(|e| format!("{}: {e}", golden.display()))?;
    for entry in entries.flatten() {
        let manifest = entry.path().join("datasheet.json");
        if !manifest.is_file() {
            continue;
        }
        let text =
            fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
        out.push(parse_manifest(&entry.file_name().to_string_lossy(), &text)?);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn parse_manifest(name: &str, text: &str) -> Result<Datasheet, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("{name}/datasheet.json: {e}"))?;
    let field = |key: &str| {
        json[key]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("{name}/datasheet.json: `{key}` is missing"))
    };
    Ok(Datasheet {
        name: name.to_string(),
        url: field("url")?,
        sha256: field("sha256")?,
    })
}

/// Lowercase hexadecimal SHA-256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn check_hash(path: &Path, expected: &str) -> Result<Status, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let actual = sha256_hex(&bytes);
    Ok(if actual == expected {
        Status::Match
    } else {
        Status::Drift { actual }
    })
}

/// Downloads `url` to `target` with curl; follows redirects, fails on HTTP
/// errors and checks that a PDF (or, for tests, any file) arrived.
fn download(url: &str, target: &Path) -> Result<(), String> {
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let status = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--max-time",
            "180",
        ])
        .args(["--user-agent", "Mozilla/5.0 (compatible; kirpitch-xtask)"])
        .arg("--output")
        .arg(target)
        .arg(url)
        .status()
        .map_err(|e| format!("cannot run curl ({e}); install curl to use this command"))?;
    if !status.success() {
        return Err(format!("download of {url} failed ({status})"));
    }
    Ok(())
}

fn pdf_path(root: &Path, sheet: &Datasheet) -> PathBuf {
    root.join("testdata/pdfs")
        .join(format!("{}.pdf", sheet.name))
}

/// Downloads missing datasheets and checks the hash of every one. Returns the
/// number of drifted datasheets.
fn fetch(root: &Path) -> Result<usize, String> {
    let mut drift = 0;
    for sheet in read_datasheets(root)? {
        let path = pdf_path(root, &sheet);
        if !path.is_file() {
            println!("{}: downloading {}", sheet.name, sheet.url);
            download(&sheet.url, &path)?;
        }
        drift += report(&sheet, check_hash(&path, &sheet.sha256)?);
    }
    Ok(drift)
}

/// Downloads every datasheet again to a temporary file and compares hashes.
fn verify(root: &Path) -> Result<usize, String> {
    let mut drift = 0;
    let tmp = std::env::temp_dir().join(format!("kirpitch-xtask-{}", std::process::id()));
    for sheet in read_datasheets(root)? {
        let path = tmp.join(format!("{}.pdf", sheet.name));
        println!("{}: downloading {}", sheet.name, sheet.url);
        let result = download(&sheet.url, &path).and_then(|()| check_hash(&path, &sheet.sha256));
        let _ = fs::remove_file(&path);
        match result {
            Ok(status) => drift += report(&sheet, status),
            Err(e) => {
                let _ = fs::remove_dir_all(&tmp);
                return Err(e);
            }
        }
    }
    let _ = fs::remove_dir_all(&tmp);
    Ok(drift)
}

fn report(sheet: &Datasheet, status: Status) -> usize {
    match status {
        Status::Match => {
            println!("{}: ok (hash {}...)", sheet.name, &sheet.sha256[..12]);
            0
        }
        Status::Drift { actual } => {
            println!(
                "{}: DRIFT. Recorded {}, found {}",
                sheet.name, sheet.sha256, actual
            );
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kirpitch-xtask-test-{label}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn manifest_needs_url_and_hash() {
        let ok = parse_manifest("p", r#"{"url":"https://x/y.pdf","sha256":"ab"}"#).unwrap();
        assert_eq!(ok.url, "https://x/y.pdf");
        assert!(
            parse_manifest("p", r#"{"url":"u"}"#)
                .unwrap_err()
                .contains("sha256")
        );
        assert!(parse_manifest("p", "not json").is_err());
    }

    #[test]
    fn hash_check_reports_match_and_drift() {
        let dir = temp("hash");
        let file = dir.join("a.pdf");
        fs::write(&file, b"abc").unwrap();
        let good = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(check_hash(&file, good).unwrap(), Status::Match);
        fs::write(&file, b"abd").unwrap();
        assert!(matches!(
            check_hash(&file, good).unwrap(),
            Status::Drift { .. }
        ));
        assert!(check_hash(&dir.join("missing.pdf"), good).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn download_works_with_a_file_url() {
        let dir = temp("download");
        let source = dir.join("source.bin");
        fs::write(&source, b"abc").unwrap();
        let url = format!(
            "file:///{}",
            source
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches('/')
        );
        let target = dir.join("out/copy.bin");
        match download(&url, &target) {
            Ok(()) => assert_eq!(fs::read(&target).unwrap(), b"abc"),
            Err(e) if e.contains("cannot run curl") => eprintln!("SKIPPED: {e}"),
            Err(e) => panic!("{e}"),
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failing_download_is_an_error() {
        let dir = temp("fail");
        let err = download("file:///definitely/not/here.bin", &dir.join("x.bin"));
        assert!(err.is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_repository_manifests_can_be_read() {
        let sheets = read_datasheets(&repo_root()).unwrap();
        assert!(sheets.len() >= 3);
        for s in sheets {
            assert!(s.url.starts_with("https://"), "{}", s.name);
            assert_eq!(s.sha256.len(), 64, "{}", s.name);
        }
    }
}
