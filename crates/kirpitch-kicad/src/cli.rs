// SPDX-License-Identifier: GPL-3.0-or-later

//! Access to KiCad's command-line tool `kicad-cli`.
//!
//! `kicad-cli` has no "validate" command, and it does not always report
//! failures through its exit code (for example it exits with 0 when it cannot
//! write its output). This module therefore checks the exit code, the messages
//! and the files that were produced. See ADR 0012.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use thiserror::Error;

/// Name of the environment variable that holds the path of `kicad-cli`.
pub const ENV_KICAD_CLI: &str = "KIRPITCH_KICAD_CLI";

/// A KiCad version as printed by `kicad-cli version`, for example `10.0.5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct KicadVersion {
    /// Major version, such as 10.
    pub major: u32,
    /// Minor version.
    pub minor: u32,
    /// Patch version.
    pub patch: u32,
}

impl KicadVersion {
    /// Parses text such as `10.0.5` or `9.0.1-rc1`; extra text after the
    /// numbers is ignored.
    pub fn parse(text: &str) -> Option<KicadVersion> {
        let mut parts = text.trim().splitn(3, '.');
        let number = |s: Option<&str>| -> Option<u32> {
            let s = s?;
            let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        };
        let major = number(parts.next())?;
        let minor = number(parts.next())?;
        let patch = number(parts.next()).unwrap_or(0);
        Some(KicadVersion {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for KicadVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What `kicad-cli ... upgrade` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeOutcome {
    /// The library was already in the current format.
    Unchanged,
    /// The library was rewritten in the current format.
    Updated,
}

/// A problem running `kicad-cli`.
#[derive(Debug, Error)]
pub enum CliError {
    /// The program could not be started.
    #[error("cannot run {path}: {source}")]
    Spawn {
        /// Program that was started.
        path: PathBuf,
        /// Operating system error.
        source: std::io::Error,
    },
    /// `kicad-cli` reported a failure or could not load the file.
    #[error("kicad-cli failed (exit code {code:?}): {output}")]
    Failed {
        /// Exit code, if the process ended normally.
        code: Option<i32>,
        /// Combined standard output and error.
        output: String,
    },
    /// A file or folder could not be prepared or read.
    #[error("file error on {path}: {source}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Operating system error.
        source: std::io::Error,
    },
    /// The program printed something we do not understand.
    #[error("unexpected kicad-cli output: {0}")]
    Unexpected(String),
}

/// A `kicad-cli` executable.
#[derive(Debug, Clone)]
pub struct KicadCli {
    path: PathBuf,
}

impl KicadCli {
    /// Uses the executable at `path`.
    pub fn new(path: impl Into<PathBuf>) -> KicadCli {
        KicadCli { path: path.into() }
    }

    /// Uses the path in the `KIRPITCH_KICAD_CLI` environment variable, if set
    /// and not empty. This is what tests use; nothing else is searched.
    pub fn from_env() -> Option<KicadCli> {
        let value = std::env::var_os(ENV_KICAD_CLI)?;
        if value.is_empty() {
            None
        } else {
            Some(KicadCli::new(PathBuf::from(value)))
        }
    }

    /// Looks for `kicad-cli`: first the `KIRPITCH_KICAD_CLI` variable, then the
    /// `PATH`, then the default install places of KiCad (newest version first).
    /// Nothing is changed on the system. Returns `None` when none works.
    pub fn discover() -> Option<KicadCli> {
        if let Some(cli) = KicadCli::from_env() {
            return Some(cli);
        }
        let mut candidates: Vec<PathBuf> = vec![PathBuf::from("kicad-cli")];
        for major in [11, 10, 9, 8] {
            candidates.push(PathBuf::from(format!(
                "C:\\Program Files\\KiCad\\{major}.0\\bin\\kicad-cli.exe"
            )));
        }
        candidates.push(PathBuf::from(
            "/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli",
        ));
        candidates
            .into_iter()
            .map(KicadCli::new)
            .find(|cli| cli.version().is_ok())
    }

    /// The executable path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn run(&self, args: &[&Path], flags: &[&str]) -> Result<Output, CliError> {
        let mut cmd = Command::new(&self.path);
        for f in flags {
            cmd.arg(f);
        }
        for a in args {
            cmd.arg(a);
        }
        cmd.output().map_err(|source| CliError::Spawn {
            path: self.path.clone(),
            source,
        })
    }

    /// Runs `kicad-cli version`.
    pub fn version(&self) -> Result<KicadVersion, CliError> {
        let out = self.run(&[], &["version"])?;
        let text = combined(&out);
        if !out.status.success() {
            return Err(CliError::Failed {
                code: out.status.code(),
                output: text,
            });
        }
        KicadVersion::parse(&text).ok_or(CliError::Unexpected(text))
    }

    /// Exports every footprint of a `.pretty` folder to SVG in `out_dir`
    /// (created if missing) and returns the SVG files.
    ///
    /// Succeeds only when `kicad-cli` exits with 0, prints no error message and
    /// wrote at least one non-empty file for every footprint file.
    pub fn export_footprint_svg(
        &self,
        pretty_dir: &Path,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>, CliError> {
        let expected = count_files(pretty_dir, "kicad_mod")?;
        self.export_svg(&["fp", "export", "svg"], pretty_dir, out_dir, expected)
    }

    /// Exports every unit of every symbol in a `.kicad_sym` file to SVG in
    /// `out_dir` (created if missing) and returns the SVG files.
    pub fn export_symbol_svg(
        &self,
        sym_file: &Path,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>, CliError> {
        self.export_svg(&["sym", "export", "svg"], sym_file, out_dir, 1)
    }

    fn export_svg(
        &self,
        sub: &[&str],
        input: &Path,
        out_dir: &Path,
        at_least: usize,
    ) -> Result<Vec<PathBuf>, CliError> {
        fs::create_dir_all(out_dir).map_err(|source| CliError::Io {
            path: out_dir.to_path_buf(),
            source,
        })?;
        let mut flags: Vec<&str> = sub.to_vec();
        flags.push("-o");
        let out = self.run_with_out(&flags, out_dir, input)?;
        let text = combined(&out);
        if !out.status.success() || has_error_message(&text) {
            return Err(CliError::Failed {
                code: out.status.code(),
                output: text,
            });
        }
        let mut files = Vec::new();
        for entry in read_dir(out_dir)? {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "svg") {
                let len = fs::metadata(&path)
                    .map_err(|source| CliError::Io {
                        path: path.clone(),
                        source,
                    })?
                    .len();
                if len == 0 {
                    return Err(CliError::Unexpected(format!("{} is empty", path.display())));
                }
                files.push(path);
            }
        }
        files.sort();
        if files.len() < at_least {
            return Err(CliError::Unexpected(format!(
                "expected at least {at_least} SVG file(s), found {}: {text}",
                files.len()
            )));
        }
        Ok(files)
    }

    /// Exports a board to a STEP file with `kicad-cli pcb export step` and
    /// returns what the tool printed. It fails when the tool fails, prints an
    /// error, or cannot add the 3D model of a footprint (a missing or broken
    /// model file is only a warning for KiCad itself).
    pub fn export_board_step(&self, board: &Path, out_file: &Path) -> Result<String, CliError> {
        let mut cmd = Command::new(&self.path);
        cmd.args(["pcb", "export", "step", "--subst-models", "-f", "-o"])
            .arg(out_file)
            .arg(board);
        let out = cmd.output().map_err(|source| CliError::Spawn {
            path: self.path.clone(),
            source,
        })?;
        let text = combined(&out);
        if !out.status.success()
            || has_error_message(&text)
            || text.contains("Could not add 3D model")
            || text.contains("File not found")
        {
            return Err(CliError::Failed {
                code: out.status.code(),
                output: text,
            });
        }
        Ok(text)
    }

    fn run_with_out(
        &self,
        flags: &[&str],
        out_dir: &Path,
        input: &Path,
    ) -> Result<Output, CliError> {
        let mut cmd = Command::new(&self.path);
        cmd.args(flags).arg(out_dir).arg(input);
        cmd.output().map_err(|source| CliError::Spawn {
            path: self.path.clone(),
            source,
        })
    }

    /// Runs `kicad-cli fp upgrade` on a `.pretty` folder. The folder is
    /// modified in place when it was in an older format, so pass a copy.
    pub fn upgrade_footprints(&self, pretty_dir: &Path) -> Result<UpgradeOutcome, CliError> {
        self.upgrade(&["fp", "upgrade"], pretty_dir)
    }

    /// Runs `kicad-cli sym upgrade` on a `.kicad_sym` file. The file is
    /// modified in place when it was in an older format, so pass a copy.
    pub fn upgrade_symbols(&self, sym_file: &Path) -> Result<UpgradeOutcome, CliError> {
        self.upgrade(&["sym", "upgrade"], sym_file)
    }

    fn upgrade(&self, flags: &[&str], target: &Path) -> Result<UpgradeOutcome, CliError> {
        let out = self.run(&[target], flags)?;
        let text = combined(&out);
        if !out.status.success() || has_error_message(&text) {
            return Err(CliError::Failed {
                code: out.status.code(),
                output: text,
            });
        }
        if text.contains("was not updated") {
            Ok(UpgradeOutcome::Unchanged)
        } else if text.contains("updated format") || text.contains("was updated") {
            Ok(UpgradeOutcome::Updated)
        } else {
            Err(CliError::Unexpected(text))
        }
    }
}

fn combined(out: &Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s.trim().to_string()
}

fn has_error_message(text: &str) -> bool {
    text.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.contains("error") || l.contains("failed") || l.contains("unable")
    })
}

fn read_dir(dir: &Path) -> Result<Vec<fs::DirEntry>, CliError> {
    let rd = fs::read_dir(dir).map_err(|source| CliError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    Ok(rd.flatten().collect())
}

fn count_files(dir: &Path, extension: &str) -> Result<usize, CliError> {
    Ok(read_dir(dir)?
        .iter()
        .filter(|e| e.path().extension().is_some_and(|x| x == extension))
        .count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        let v = KicadVersion::parse("10.0.5\n").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (10, 0, 5));
        assert_eq!(KicadVersion::parse("9.0.1-rc1").unwrap().patch, 1);
        assert_eq!(KicadVersion::parse("8.0").unwrap().patch, 0);
        assert!(KicadVersion::parse("hello").is_none());
        assert!(KicadVersion::parse("10").is_none());
        assert_eq!(v.to_string(), "10.0.5");
        assert!(v > KicadVersion::parse("9.0.9").unwrap());
    }

    #[test]
    fn detects_error_messages() {
        assert!(has_error_message("Unable to load library"));
        assert!(has_error_message("Failed to create file 'x'."));
        assert!(has_error_message("Error creating svg file"));
        assert!(!has_error_message("Plotted to 'a.svg'.\nDone."));
    }
}
