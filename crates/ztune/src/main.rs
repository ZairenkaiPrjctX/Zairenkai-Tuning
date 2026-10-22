// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! `ztune`: prints one `zairenkai.tuning` schema 1 document per run (`docs/CONTRACT.md`).

use std::process::ExitCode;

use ztune::cli;
use ztune_core::contract::{error_document, Error, ErrorCode};

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if matches!(
        argv.first().map(String::as_str),
        Some("--help" | "-h" | "help")
    ) {
        println!("{}", cli::USAGE);
        return ExitCode::from(cli::EXIT_OK);
    }
    if argv.first().map(String::as_str) == Some("--version") {
        println!("ztune {}", cli::VERSION);
        return ExitCode::from(cli::EXIT_OK);
    }
    let (code, doc) = match cli::parse(&argv) {
        Ok(a) => cli::run(&a),
        Err(e) => {
            eprintln!("ztune: {e}\n\n{}", cli::USAGE);
            (
                cli::EXIT_USAGE,
                error_document(cli::VERSION, &Error::new(ErrorCode::Usage, e)),
            )
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&doc).expect("documents serialize")
    );
    ExitCode::from(code)
}
