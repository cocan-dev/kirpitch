// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands that use language models: `build`, `models test` and `bench`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use kirpitch_core::{BoundingBox, PartSpec, has_errors, validate};
use kirpitch_extract::PdfReader;
use kirpitch_models::{
    BuildOptions, BuildReport, Cache, Client, Config, ConfigError, PageLines, Stats, Task,
    TextLine, build, probe, score,
};
use sha2::{Digest, Sha256};

use crate::{Failure, fab, print_findings, transport};

/// Folder where model answers are remembered.
const CACHE_DIR: &str = ".kirpitch-cache";

/// Read a datasheet with a language model and write a PartSpec.
#[derive(Args)]
pub struct BuildArgs {
    /// The datasheet PDF (text-based)
    pdf: PathBuf,
    /// The PartSpec file to write
    #[arg(short, long)]
    out: PathBuf,
    /// Configuration file (default: kirpitch.toml in the current folder)
    #[arg(long)]
    config: Option<PathBuf>,
    /// Tell the models where to look or which package to use, for example
    /// "the pinout is on page 4" or "SOIC package, 8 pins"
    #[arg(long)]
    hint: Option<String>,
    /// Read the pin table twice and flag every difference
    #[arg(long)]
    second_read: bool,
    /// Part number (skips reading it)
    #[arg(long)]
    mpn: Option<String>,
    /// Manufacturer (skips reading it)
    #[arg(long)]
    manufacturer: Option<String>,
    /// Description (skips reading it)
    #[arg(long)]
    description: Option<String>,
    /// Where the datasheet came from
    #[arg(long)]
    datasheet_url: Option<String>,
    /// Do not remember model answers
    #[arg(long)]
    no_cache: bool,
}

/// Commands about the configured models.
#[derive(Subcommand)]
pub enum ModelsCommand {
    /// Check each configured model: reachable, structured JSON, vision, context
    Test(ConfigArgs),
}

/// A configuration file option.
#[derive(Args)]
pub struct ConfigArgs {
    /// Configuration file (default: kirpitch.toml in the current folder)
    #[arg(long)]
    config: Option<PathBuf>,
}

/// Run the extraction on the golden set and report the accuracy.
#[derive(Args)]
pub struct BenchArgs {
    /// Configuration file (default: kirpitch.toml in the current folder)
    #[arg(long)]
    config: Option<PathBuf>,
    /// Folder with the golden parts
    #[arg(long, default_value = "testdata/golden")]
    golden: PathBuf,
    /// Folder with the datasheet PDFs (`cargo xtask fetch-datasheets`)
    #[arg(long, default_value = "testdata/pdfs")]
    pdfs: PathBuf,
    /// Read the pin tables twice
    #[arg(long)]
    second_read: bool,
}

fn config_error(e: ConfigError) -> Failure {
    Failure::Message(e.to_string())
}

fn load_config(path: Option<&Path>) -> Result<Config, Failure> {
    let found = match path {
        Some(p) => Some(Config::from_file(p).map_err(config_error)?),
        None => Config::discover().map_err(config_error)?,
    };
    found.ok_or_else(|| {
        Failure::Message(
            "no model is configured: there is no kirpitch.toml in this folder. Without a model, use the manual flow: `kirpitch template ...` for the package and `kirpitch pins ... --pdf FILE --page N --region l,t,r,b` for the pins".into(),
        )
    })
}

/// Reads the text of every page of a PDF, with the place of each line.
fn read_pages(bytes: &[u8]) -> Result<Vec<PageLines>, Failure> {
    let dir = std::env::var_os("KIRPITCH_PDFIUM_LIB_DIR").map(PathBuf::from);
    let reader = PdfReader::new(dir.as_deref()).map_err(|e| Failure::Message(e.to_string()))?;
    let count = reader
        .page_count(bytes)
        .map_err(|e| Failure::Message(e.to_string()))?;
    let whole = BoundingBox([0.0, 0.0, 1.0, 1.0]);
    let mut pages = Vec::new();
    for page in 1..=count {
        let lines = reader
            .lines_in_region(bytes, page, whole)
            .map_err(|e| Failure::Message(e.to_string()))?;
        pages.push(PageLines {
            page,
            lines: lines
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
                .collect(),
        });
    }
    Ok(pages)
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn describe_stats(config: &Config, stats: &Stats, out: &mut dyn Write) -> std::io::Result<()> {
    write!(
        out,
        "{} model call(s), {} answer(s) from the cache, {} tokens in, {} tokens out, {:.1} s",
        stats.calls,
        stats.cached,
        stats.usage.input_tokens,
        stats.usage.output_tokens,
        stats.seconds
    )?;
    if let Ok((_, model)) = config.model_for(Task::PinTables)
        && let (Some(i), Some(o)) = (model.price_in, model.price_out)
    {
        write!(
            out,
            ", about {:.4} (estimated at the prices of the pin_tables model)",
            stats.cost(i, o)
        )?;
    }
    writeln!(out)
}

fn print_models(report: &BuildReport, out: &mut dyn Write) -> std::io::Result<()> {
    for (task, model, local) in &report.models {
        writeln!(
            out,
            "{:<10} {} ({})",
            task.name(),
            model,
            if *local {
                "data stays on this machine or the local network"
            } else {
                "data leaves this machine"
            }
        )?;
    }
    Ok(())
}

pub fn build_cmd(args: BuildArgs, out: &mut dyn Write, err: &mut dyn Write) -> Result<(), Failure> {
    let config = load_config(args.config.as_deref())?;
    let bytes = fs::read(&args.pdf).map_err(|e| format!("{}: {e}", args.pdf.display()))?;
    let pages = read_pages(&bytes)?;
    let with_text = pages.iter().filter(|p| !p.lines.is_empty()).count();
    if with_text == 0 {
        return Err(Failure::Message(
            "the PDF has no text on any page; scanned datasheets need OCR, which this version does not have. Use the manual flow with `kirpitch template` and `kirpitch pins --import`".into(),
        ));
    }
    writeln!(out, "read {} pages ({with_text} with text)", pages.len())
        .map_err(|e| e.to_string())?;
    let cache = if args.no_cache {
        Cache::none()
    } else {
        Cache::in_dir(Path::new(CACHE_DIR))
    };
    let options = BuildOptions {
        mpn: args.mpn,
        manufacturer: args.manufacturer,
        description: args.description,
        hint: args.hint,
        second_read: args.second_read,
        pdf_sha256: Some(hex(&bytes)),
        datasheet_url: args.datasheet_url,
    };
    let report = build(&config, transport(), &cache, &pages, &options);
    print_models(&report, out).map_err(|e| e.to_string())?;
    describe_stats(&config, &report.stats, out).map_err(|e| e.to_string())?;
    for note in &report.notes {
        writeln!(out, "check: {note}").map_err(|e| e.to_string())?;
    }
    for failure in &report.failures {
        writeln!(err, "failed: {failure}").map_err(|e| e.to_string())?;
    }
    match &report.spec {
        Some(spec) => {
            fs::write(&args.out, spec.to_json_string().as_bytes())
                .map_err(|e| format!("{}: {e}", args.out.display()))?;
            writeln!(out, "wrote {}", args.out.display()).map_err(|e| e.to_string())?;
            let findings = validate(spec, &fab());
            print_findings(&findings, out).map_err(|e| e.to_string())?;
            writeln!(
                out,
                "Next: `kirpitch review {0} --stage pinout`, then `kirpitch approve {0} --stage pinout`, and the same for footprint and 3d.",
                args.out.display()
            )
            .map_err(|e| e.to_string())?;
            if has_errors(&findings) {
                return Err(Failure::Invalid);
            }
            Ok(())
        }
        None => {
            if let Some(pins) = &report.pins {
                let mut csv = String::from("Pin,Name,Type\n");
                for p in &pins.pins {
                    csv.push_str(&format!(
                        "{},{},{}\n",
                        p.pin.number,
                        p.pin.name.replace(',', ";"),
                        p.pin.electrical_type.as_str()
                    ));
                }
                let path = args.out.with_extension("pins.csv");
                fs::write(&path, csv).map_err(|e| format!("{}: {e}", path.display()))?;
                writeln!(
                    out,
                    "The pins were read and saved to {}. Finish by hand: `kirpitch template {} ...` for the package, then `kirpitch pins {} --import {}`.",
                    path.display(),
                    args.out.display(),
                    args.out.display(),
                    path.display()
                )
                .map_err(|e| e.to_string())?;
            }
            Err(Failure::Message(
                "the PartSpec could not be built; the stages that failed are listed above, and the manual flow can finish them".into(),
            ))
        }
    }
}

pub fn models_cmd(command: ModelsCommand, out: &mut dyn Write) -> Result<(), Failure> {
    match command {
        ModelsCommand::Test(args) => {
            let config = load_config(args.config.as_deref())?;
            if config.models.is_empty() {
                return Err(Failure::Message("the configuration has no [models]".into()));
            }
            let mut any_bad = false;
            for (name, model) in &config.models {
                writeln!(out, "{name}: {:?} {}", model.provider, model.model)
                    .map_err(|e| e.to_string())?;
                if let Err(e) = config.allowed(name, model) {
                    writeln!(out, "  refused: {e}").map_err(|e| e.to_string())?;
                    any_bad = true;
                    continue;
                }
                let client = match Client::from_env(model, transport()) {
                    Ok(c) => c,
                    Err(e) => {
                        writeln!(out, "  not usable: {e}").map_err(|e| e.to_string())?;
                        any_bad = true;
                        continue;
                    }
                };
                let r = probe(name, &client);
                let yes = |b: bool| if b { "yes" } else { "NO" };
                let opt = |b: Option<bool>| b.map_or("not tested", |v| yes(v));
                writeln!(
                    out,
                    "  reachable: {}   structured JSON: {}   vision: {}   8k context: {}   ({:.1} s, {} tokens in, {} out)",
                    yes(r.reachable),
                    yes(r.structured_json),
                    opt(r.vision),
                    opt(r.context_8k),
                    r.seconds,
                    r.usage.input_tokens,
                    r.usage.output_tokens
                )
                .map_err(|e| e.to_string())?;
                for note in &r.notes {
                    writeln!(out, "  note: {note}").map_err(|e| e.to_string())?;
                }
                if !r.reachable || !r.structured_json {
                    any_bad = true;
                }
            }
            for task in [Task::PinTables, Task::Drawings, Task::Metadata] {
                match config.model_for(task) {
                    Ok((name, _)) => writeln!(
                        out,
                        "task {:<10} uses {name}; data {}",
                        task.name(),
                        if config.stays_local(task) {
                            "stays on this machine or the local network"
                        } else {
                            "leaves this machine"
                        }
                    ),
                    Err(e) => writeln!(out, "task {:<10} {e}", task.name()),
                }
                .map_err(|e| e.to_string())?;
            }
            if any_bad {
                Err(Failure::Message("at least one model is not usable".into()))
            } else {
                Ok(())
            }
        }
    }
}

pub fn bench_cmd(args: BenchArgs, out: &mut dyn Write) -> Result<(), Failure> {
    let config = load_config(args.config.as_deref())?;
    let mut dirs: Vec<PathBuf> = fs::read_dir(&args.golden)
        .map_err(|e| format!("{}: {e}", args.golden.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("partspec.json").is_file())
        .collect();
    dirs.sort();
    writeln!(
        out,
        "{:<18} {:>8} {:>10} {:>7} {:>7} {:>10} {:>8}",
        "part", "pinout", "dimensions", "pads", "extra", "tokens", "seconds"
    )
    .map_err(|e| e.to_string())?;
    let mut total = Stats::default();
    let (mut pins_ok, mut pins_all, mut fp_ok, mut fp_all) = (0usize, 0usize, 0usize, 0usize);
    let (mut pads_ok, mut pads_all) = (0usize, 0usize);
    for dir in dirs {
        let name = dir
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let text = fs::read_to_string(dir.join("partspec.json"))
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        let expected = PartSpec::from_json_str(&text).map_err(|e| format!("{name}: {e}"))?;
        let pdf = args.pdfs.join(format!("{name}.pdf"));
        let Ok(bytes) = fs::read(&pdf) else {
            writeln!(
                out,
                "{name:<18} skipped: no {} (run `cargo xtask fetch-datasheets`)",
                pdf.display()
            )
            .map_err(|e| e.to_string())?;
            continue;
        };
        let pages = read_pages(&bytes)?;
        let hint = fs::read_to_string(dir.join("datasheet.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| {
                v.get("package")
                    .and_then(|p| p.as_str())
                    .map(str::to_string)
            });
        let options = BuildOptions {
            hint,
            second_read: args.second_read,
            ..Default::default()
        };
        let report = build(&config, transport(), &Cache::none(), &pages, &options);
        total.add(&report.stats);
        let tokens = report.stats.usage.input_tokens + report.stats.usage.output_tokens;
        match &report.spec {
            Some(spec) => {
                let s = score(spec, &expected);
                pins_ok += s.pins_correct;
                pins_all += s.pins_expected;
                fp_ok += s.dimensions_correct;
                fp_all += s.dimensions_expected;
                pads_ok += s.pads_correct;
                pads_all += s.pads_expected;
                writeln!(
                    out,
                    "{name:<18} {:>7.0}% {:>9.0}% {:>6.0}% {:>7} {:>10} {:>8.1}",
                    s.pinout() * 100.0,
                    s.footprint() * 100.0,
                    s.pads() * 100.0,
                    s.pins_extra,
                    tokens,
                    report.stats.seconds
                )
                .and_then(|()| {
                    for m in &s.mismatches {
                        writeln!(out, "    {m}")?;
                    }
                    Ok(())
                })
            }
            None => {
                pins_all += expected.pins.len();
                fp_all += expected.package.dimensions.len();
                pads_all += expected.land_pattern.pads.len();
                writeln!(out, "{name:<18} failed: {}", report.failures.join("; "))
            }
        }
        .map_err(|e| e.to_string())?;
    }
    let pct = |a: usize, b: usize| {
        if b == 0 {
            0.0
        } else {
            100.0 * a as f64 / b as f64
        }
    };
    writeln!(
        out,
        "{:<18} {:>7.0}% {:>9.0}% {:>6.0}%   (all parts together)",
        "total",
        pct(pins_ok, pins_all),
        pct(fp_ok, fp_all),
        pct(pads_ok, pads_all)
    )
    .map_err(|e| e.to_string())?;
    describe_stats(&config, &total, out).map_err(|e| e.to_string())?;
    writeln!(out, "dimensions: package dimensions (minimum and maximum) that match the golden part. pads: pads that match the golden land pattern; they come from the IPC-7351 formulas, while a golden part may carry the maker's own land pattern, so a difference is not always an error.")
        .map_err(|e| e.to_string())?;
    Ok(())
}
