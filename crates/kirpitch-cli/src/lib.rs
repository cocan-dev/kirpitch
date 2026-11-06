// SPDX-License-Identifier: GPL-3.0-or-later

//! The command-line interface of Kirpitch.
//!
//! Milestone 1 offers three commands that work on a hand-written PartSpec:
//!
//! - `kirpitch schema` prints the JSON schema of the PartSpec.
//! - `kirpitch check <spec.json>` lists the validation findings.
//! - `kirpitch gen <symbol|footprint|all> <spec.json> -o <dir>` writes KiCad
//!   files.
//!
//! Exit codes: 0 success; 1 a file or usage problem; 3 the PartSpec has
//! validation errors. (clap itself uses 2 for command-line mistakes.)

mod flow;

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use kirpitch_core::partspec::json_schema_string;
use kirpitch_core::{FabProfile, Finding, Level, PartSpec, has_errors, validate};
use kirpitch_kicad::{KicadCli, KicadTarget};

/// Exit code for files that cannot be read or written.
const EXIT_FAILURE: u8 = 1;
/// Exit code for a PartSpec with validation errors.
const EXIT_INVALID: u8 = 3;

#[derive(Parser)]
#[command(
    name = "kirpitch",
    version,
    about = "Turn datasheets into reviewed KiCad parts"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the JSON schema of the PartSpec
    Schema,
    /// Check a PartSpec and list the findings
    Check {
        /// The PartSpec JSON file
        spec: PathBuf,
    },
    /// Create or update a PartSpec from a package template
    Template(flow::TemplateArgs),
    /// Read the pins of a PartSpec from a pin table (CSV or pasted text)
    Pins(flow::PinsArgs),
    /// Show where each review stage stands
    Status(flow::StatusArgs),
    /// Look at a review stage and optionally edit it
    Review(flow::ReviewArgs),
    /// Approve a review stage
    Approve(flow::ApproveArgs),
    /// Generate KiCad files from a PartSpec
    Gen {
        /// Which file to generate
        #[arg(value_enum)]
        what: What,
        /// The PartSpec JSON file
        spec: PathBuf,
        /// Folder to write into (created if missing)
        #[arg(short, long)]
        out: PathBuf,
        /// KiCad major version to write for (default: the installed KiCad
        /// version, or the newest supported one)
        #[arg(long)]
        target: Option<u32>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum What {
    /// A `.kicad_sym` library with the symbol
    Symbol,
    /// A `.pretty` folder with the footprint
    Footprint,
    /// Both
    All,
}

/// Runs the tool with `args` (the first one is the program name), writing
/// normal output to `out` and messages to `err`.
pub fn run<I, T>(args: I, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            // Help and version go to stdout, mistakes to stderr.
            let text = e.render().to_string();
            let _ = if e.use_stderr() {
                write!(err, "{text}")
            } else {
                write!(out, "{text}")
            };
            return ExitCode::from(if e.use_stderr() { 2 } else { 0 });
        }
    };
    match execute(cli.command, out, err) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Message(m)) => {
            let _ = writeln!(err, "error: {m}");
            ExitCode::from(EXIT_FAILURE)
        }
        Err(Failure::Invalid) => ExitCode::from(EXIT_INVALID),
    }
}

pub(crate) enum Failure {
    Message(String),
    /// The findings were already printed.
    Invalid,
}

impl From<String> for Failure {
    fn from(m: String) -> Failure {
        Failure::Message(m)
    }
}

fn execute(command: Command, out: &mut dyn Write, err: &mut dyn Write) -> Result<(), Failure> {
    match command {
        Command::Schema => {
            write!(out, "{}", json_schema_string()).map_err(|e| e.to_string())?;
            Ok(())
        }
        Command::Template(args) => flow::template(args, out),
        Command::Pins(args) => flow::pins(args, out, err),
        Command::Status(args) => flow::status(args, out),
        Command::Review(args) => flow::review(args, out),
        Command::Approve(args) => flow::approve_cmd(args, out, err),
        Command::Check { spec } => {
            let spec = load(&spec)?;
            let findings = validate(&spec, &FabProfile::generic());
            print_findings(&findings, out).map_err(|e| e.to_string())?;
            if has_errors(&findings) {
                Err(Failure::Invalid)
            } else {
                Ok(())
            }
        }
        Command::Gen {
            what,
            spec,
            out: dir,
            target,
        } => {
            let spec = load(&spec)?;
            let findings = validate(&spec, &FabProfile::generic());
            if has_errors(&findings) {
                print_findings(&findings, err).map_err(|e| e.to_string())?;
                return Err(Failure::Invalid);
            }
            let target = choose_target(target, err)?;
            generate(what, &spec, &dir, target, out)
        }
    }
}

pub(crate) fn load(path: &Path) -> Result<PartSpec, Failure> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    PartSpec::from_json_str(&text).map_err(|e| Failure::Message(format!("{}: {e}", path.display())))
}

pub(crate) fn print_findings(findings: &[Finding], out: &mut dyn Write) -> std::io::Result<()> {
    for f in findings {
        let level = match f.level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        write!(out, "{level}[{}] {}: {}", f.check.id(), f.path, f.message)?;
        if let Some(note) = &f.overridden_by {
            write!(out, " (overridden: {note})")?;
        }
        writeln!(out)?;
    }
    let errors = findings.iter().filter(|f| f.level == Level::Error).count();
    let warnings = findings.len() - errors;
    writeln!(out, "{errors} error(s), {warnings} warning(s)")
}

/// Applies ADR 0002: an explicit `--target`, else the installed KiCad version,
/// else the newest supported one.
fn choose_target(explicit: Option<u32>, err: &mut dyn Write) -> Result<KicadTarget, Failure> {
    if let Some(major) = explicit {
        return KicadTarget::from_major(major).map_err(|e| Failure::Message(e.to_string()));
    }
    let Some(cli) = KicadCli::discover() else {
        return Ok(KicadTarget::LATEST);
    };
    match cli.version() {
        Ok(v) if v.major > KicadTarget::LATEST.major() => Ok(KicadTarget::LATEST),
        Ok(v) => match KicadTarget::from_major(v.major) {
            Ok(t) => Ok(t),
            Err(_) => {
                let _ = writeln!(
                    err,
                    "note: KiCad {v} is installed but not supported yet; writing KiCad {} files",
                    KicadTarget::LATEST.major()
                );
                Ok(KicadTarget::LATEST)
            }
        },
        Err(_) => Ok(KicadTarget::LATEST),
    }
}

fn generate(
    what: What,
    spec: &PartSpec,
    dir: &Path,
    target: KicadTarget,
    out: &mut dyn Write,
) -> Result<(), Failure> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut written: Vec<PathBuf> = Vec::new();
    if matches!(what, What::Symbol | What::All) {
        let options = kirpitch_gen_symbol::Options {
            target,
            ..Default::default()
        };
        let text = kirpitch_gen_symbol::generate(spec, &options).map_err(|e| e.to_string())?;
        let path = dir.join(kirpitch_gen_symbol::file_name(spec));
        write_file(&path, &text)?;
        written.push(path);
    }
    if matches!(what, What::Footprint | What::All) {
        let options = kirpitch_gen_footprint::Options {
            target,
            ..Default::default()
        };
        let text = kirpitch_gen_footprint::generate(spec, &options).map_err(|e| e.to_string())?;
        let folder = dir.join("kirpitch.pretty");
        fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        let path = folder.join(kirpitch_gen_footprint::file_name(spec));
        write_file(&path, &text)?;
        written.push(path);
    }
    for path in written {
        writeln!(out, "wrote {}", path.display()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Writes `text` exactly as given (always `\n` line endings).
fn write_file(path: &Path, text: &str) -> Result<(), Failure> {
    fs::write(path, text.as_bytes())
        .map_err(|e| Failure::Message(format!("{}: {e}", path.display())))
}
