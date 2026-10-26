//! ropacker build | publish | restart | run  (la configuración está en ropacker.toml; ver README).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use ropacker::cli::{self, Cmd};

#[derive(Parser)]
#[command(name = "ropacker", version, about = "Carpetas de Luau -> lugar de Roblox -> Open Cloud")]
struct Cli {
    /// buscar ropacker.toml desde esta carpeta
    #[arg(short = 'C', global = true, value_name = "CARPETA")]
    cwd: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

fn main() -> ExitCode {
    let args = Cli::parse();
    match cli::run(args.cwd.as_deref(), args.cmd) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}
