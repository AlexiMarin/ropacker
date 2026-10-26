//! ropacker build | publish | restart | run  (la configuración está en ropacker.toml; ver README).

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ropacker::config::Config;
use ropacker::{cloud, config, rbxlx, tree};

#[derive(Parser)]
#[command(name = "ropacker", version, about = "Carpetas de Luau -> lugar de Roblox -> Open Cloud")]
struct Cli {
    /// buscar ropacker.toml desde esta carpeta
    #[arg(short = 'C', global = true, value_name = "CARPETA")]
    cwd: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// arma el .rbxlx
    Build,
    /// arma y publica una versión del lugar
    Publish {
        /// solo guardar la versión, sin publicarla
        #[arg(long)]
        saved: bool,
        /// después, reiniciar los servidores
        #[arg(long)]
        restart: bool,
        /// publicar aunque no haya lugar base
        #[arg(long)]
        allow_no_base: bool,
        /// en la experiencia de [test]
        #[arg(long)]
        test: bool,
    },
    /// reinicia los servidores para que usen la última versión
    Restart {
        /// todos los lugares del universo, no solo `place`
        #[arg(long)]
        all_places: bool,
        /// en la experiencia de [test]
        #[arg(long)]
        test: bool,
    },
    /// corre un script de Luau en un servidor de Roblox con el código de disco
    Run {
        script: PathBuf,
        /// usar esta versión del lugar en vez de subir una
        #[arg(long, value_name = "N")]
        version: Option<u64>,
        /// usar la versión publicada en vez de subir una
        #[arg(long)]
        live: bool,
        /// segundos (máximo 300)
        #[arg(long, default_value_t = 300)]
        timeout: u32,
        #[arg(long)]
        allow_no_base: bool,
        /// correr en `place` aunque haya [test]
        #[arg(long)]
        prod: bool,
    },
}

fn build(cfg: &Config, quiet: bool) -> Result<PathBuf> {
    let base = cfg.base.as_ref().filter(|b| b.exists());
    let mut place = match base {
        Some(b) => rbxlx::read(b)?,
        None => rbxlx::empty(),
    };
    if let (Some(b), None) = (&cfg.base, base) {
        eprintln!("aviso: no está el lugar base {}; sale un lugar solo con [tree]", b.display());
    }
    for (at, folder) in &cfg.tree {
        let found = tree::scan(folder)?;
        let replaced = place.place(at, &found.nodes);
        if !quiet {
            let extra = match found.skipped.len() {
                0 => String::new(),
                n => format!(", {n} archivos que no son Luau sin incluir"),
            };
            println!("{at:<36} {:>4} scripts  ({replaced} reemplazados){extra}", found.scripts);
        }
    }
    place.write(&cfg.output)?;
    if !quiet {
        let kb = fs::metadata(&cfg.output)?.len() / 1024;
        let shown = cfg.output.strip_prefix(&cfg.root).unwrap_or(&cfg.output);
        println!("-> {} ({kb} KB)", shown.display());
    }
    Ok(cfg.output.clone())
}

fn need_base(cfg: &Config, allow: bool) -> Result<()> {
    if allow || cfg.base.as_ref().is_some_and(|b| b.exists()) {
        return Ok(());
    }
    bail!(
        "sin lugar base: publicar así borra todo lo que no esté en [tree] (Workspace, ajustes de servicios...).\n\
         Exporta el lugar desde Studio (File > Save to File As > .rbxlx), ponlo en `base` de ropacker.toml,\n\
         o usa --allow-no-base si de verdad no hay nada más en el lugar."
    )
}

fn run(cli: Cli) -> Result<()> {
    let cfg = config::load(cli.cwd.as_deref())?;
    match cli.cmd {
        Cmd::Build => {
            build(&cfg, false)?;
        }
        Cmd::Publish { saved, restart, allow_no_base, test } => {
            let (universe, place) = cfg.need_place(test)?;
            need_base(&cfg, allow_no_base)?;
            let file = build(&cfg, false)?;
            let key = cfg.api_key(test)?;
            let version = cloud::publish(&key, universe, place, &file, saved)?;
            println!("{}: versión {version} del lugar {place}", if saved { "guardada" } else { "publicada" });
            if restart && !saved {
                cloud::restart(&key, universe, Some(&[place]))?;
                println!("servidores reiniciándose con la versión nueva");
            }
        }
        Cmd::Restart { all_places, test } => {
            let (universe, place) = cfg.need_place(test)?;
            let places = [place];
            cloud::restart(&cfg.api_key(test)?, universe, if all_places { None } else { Some(&places) })?;
            println!("servidores reiniciándose (los jugadores pasan a servidores con la última versión)");
        }
        Cmd::Run { script, version, live, timeout, allow_no_base, prod } => {
            // con [test], los scripts corren ahí: la clave con permiso de Luau llega a los DataStores de esa experiencia
            let test = cfg.test.is_some() && !prod;
            let (universe, place) = cfg.need_place(test)?;
            println!("corriendo en el lugar {place}");
            let source = fs::read_to_string(&script).with_context(|| format!("{}", script.display()))?;
            let key = cfg.api_key(test)?;
            let mut version = version;
            if version.is_none() && !live {
                need_base(&cfg, allow_no_base)?;
                let v = cloud::publish(&key, universe, place, &build(&cfg, true)?, true)?;
                println!("versión {v} guardada (no publicada) para correr {}", script.display());
                version = Some(v);
            }
            let task = cloud::run(&key, universe, place, version, &source, timeout)?;
            for line in &task.logs {
                println!("{line}");
            }
            if let Some(results) = task.results.as_ref().filter(|r| r.as_array().is_none_or(|a| !a.is_empty())) {
                println!("{}", serde_json::to_string_pretty(results)?);
            }
            if task.state != "COMPLETE" {
                let err = task.error.unwrap_or_default();
                let msg = err["message"].as_str().map_or_else(|| err.to_string(), str::to_string);
                bail!("{}: {msg}", task.state);
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}
