// SPDX-License-Identifier: GPL-3.0-or-later

//! The `kirpitch` command-line tool.

use std::process::ExitCode;

fn main() -> ExitCode {
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    kirpitch_cli::run(std::env::args_os(), &mut stdout.lock(), &mut stderr.lock())
}
