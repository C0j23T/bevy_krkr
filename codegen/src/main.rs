//! CLI entry point for the `krkr_codegen` binary.
//!
//! Usage:
//!   krkr_codegen --src <KIRIKIROID2_SRC> \
//!               --out-header <PATH/krkr_c_api.h> \
//!               --out-cpp    <PATH/krkr_c_api.cpp> \
//!               [--verbose]

use anyhow::Result;
use clap::Parser;
use krkr_codegen::{run_codegen, CodegenConfig};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "krkr_codegen",
    about = "Generate C ABI wrapper headers for Kirikiroid2 (used by krkr_sys/build.rs)",
    version
)]
struct Cli {
    /// Path to a Kirikiroid2 source checkout.
    #[arg(long, value_name = "PATH")]
    src: PathBuf,

    /// Output path for the generated C header (`krkr_c_api.h`).
    #[arg(long, value_name = "PATH", default_value = "krkr_c_api.h")]
    out_header: PathBuf,

    /// Output path for the generated C++ shim.
    #[arg(long, value_name = "PATH", default_value = "krkr_c_api.cpp")]
    out_cpp: PathBuf,

    /// Print progress messages.
    #[arg(long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = CodegenConfig {
        kirikiroid2_src: cli.src,
        output_header: cli.out_header,
        output_cpp: cli.out_cpp,
        verbose: cli.verbose,
    };
    run_codegen(&config)?;
    eprintln!(
        "krkr_codegen: wrote {} and {}",
        config.output_header.display(),
        config.output_cpp.display()
    );
    Ok(())
}
