//! Lee ropacker.toml (se busca desde la carpeta actual hacia arriba) y el .env con la clave de Open Cloud.
//!
//! ```toml
//! universe = 1234567890
//! place = 9876543210
//! base = "place/base.rbxlx"        # el lugar exportado de Studio (opcional, pero sin él se pierde lo demás)
//! output = "build/place.rbxlx"
//! env = "secrets/.env"             # opcional: archivo con ROBLOX_API_KEY
//!
//! [test]                           # opcional: otra experiencia para `run` y para --test (no toca la de verdad)
//! universe = 1234567891
//! place = 9876543211
//!
//! [tree]                           # ruta en el juego = carpeta cuyo contenido va ahí
//! ReplicatedStorage = "game/shared"
//! "StarterPlayer.StarterPlayerScripts" = "game/client"
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use toml::{Table, Value};

pub const NAME: &str = "ropacker.toml";

#[derive(Debug, Clone)]
pub struct Config {
    pub root: PathBuf,
    pub universe: Option<u64>,
    pub place: Option<u64>,
    pub base: Option<PathBuf>,
    pub output: PathBuf,
    /// en el orden del toml
    pub tree: Vec<(String, PathBuf)>,
    pub test: Option<(u64, u64)>,
    /// lo que vino de los .env (lo que ya esté en el entorno gana)
    pub env: HashMap<String, String>,
}

impl Config {
    pub fn var(&self, name: &str) -> Option<String> {
        std::env::var(name)
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| self.env.get(name).filter(|v| !v.is_empty()).cloned())
    }

    pub fn api_key(&self, test: bool) -> Result<String> {
        // la experiencia de [test] puede tener su propia clave (ROBLOX_TEST_API_KEY); si no, se usa la misma
        if test {
            if let Some(key) = self.var("ROBLOX_TEST_API_KEY") {
                return Ok(key);
            }
        }
        self.var("ROBLOX_API_KEY")
            .ok_or_else(|| anyhow!("falta ROBLOX_API_KEY (en el entorno o en el .env de ropacker.toml)"))
    }

    pub fn need_place(&self, test: bool) -> Result<(u64, u64)> {
        if test {
            return self.test.ok_or_else(|| {
                anyhow!("falta la tabla [test] (universe y place) en {}", self.root.join(NAME).display())
            });
        }
        match (self.universe, self.place) {
            (Some(u), Some(p)) => Ok((u, p)),
            _ => bail!("faltan universe y place en {}", self.root.join(NAME).display()),
        }
    }
}

pub fn find(start: &Path) -> Result<PathBuf> {
    start
        .ancestors()
        .map(|folder| folder.join(NAME))
        .find(|path| path.exists())
        .ok_or_else(|| anyhow!("no hay {NAME} aquí ni en las carpetas de arriba"))
}

/// CLAVE=valor por línea; lo que ya se leyó gana.
pub fn load_env(path: &Path, env: &mut HashMap<String, String>) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    for line in text.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            env.entry(k.trim().to_string()).or_insert_with(|| v.to_string());
        }
    }
    Ok(())
}

/// Un id de Roblox: entero positivo (0 o ausente = no está).
fn id(table: &Table, key: &str, path: &Path) -> Result<Option<u64>> {
    match table.get(key) {
        None => Ok(None),
        Some(Value::Integer(0)) => Ok(None),
        Some(Value::Integer(n)) if *n > 0 => Ok(Some(*n as u64)),
        Some(other) => bail!("{}: `{key}` tiene que ser un número positivo, no {other}", path.display()),
    }
}

fn string<'a>(table: &'a Table, key: &str, path: &Path) -> Result<Option<&'a str>> {
    match table.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(other) => bail!("{}: `{key}` tiene que ser texto, no {other}", path.display()),
    }
}

pub fn load(start: Option<&Path>) -> Result<Config> {
    let start = match start {
        Some(s) => s.to_path_buf(),
        None => std::env::current_dir()?,
    };
    let start = start.canonicalize().with_context(|| format!("{}", start.display()))?;
    let path = find(&start)?;
    let root = path.parent().unwrap().to_path_buf();
    let text = fs::read_to_string(&path).with_context(|| format!("{}", path.display()))?;
    let data: Table = text.parse().with_context(|| format!("{}", path.display()))?;

    let mut env = HashMap::new();
    if let Some(file) = string(&data, "env", &path)? {
        load_env(&root.join(file), &mut env)?;
    }
    load_env(&root.join(".env"), &mut env)?;

    let mut tree = Vec::new();
    if let Some(table) = data.get("tree").and_then(Value::as_table) {
        for (where_, folder) in table {
            let folder = folder
                .as_str()
                .ok_or_else(|| anyhow!("{}: [tree] {where_} tiene que ser una carpeta (texto)", path.display()))?;
            tree.push((where_.clone(), root.join(folder)));
        }
    }
    if tree.is_empty() {
        bail!("{}: falta la tabla [tree]", path.display());
    }
    for (where_, folder) in &tree {
        if !folder.is_dir() {
            bail!("{}: [tree] {where_} -> {} no es una carpeta", path.display(), folder.display());
        }
    }

    let test = match data.get("test") {
        None => None,
        Some(Value::Table(t)) => match (id(t, "universe", &path)?, id(t, "place", &path)?) {
            (Some(u), Some(p)) => Some((u, p)),
            _ => bail!("{}: [test] necesita universe y place", path.display()),
        },
        Some(_) => bail!("{}: `test` tiene que ser una tabla", path.display()),
    };

    Ok(Config {
        universe: id(&data, "universe", &path)?,
        place: id(&data, "place", &path)?,
        base: string(&data, "base", &path)?.map(|b| root.join(b)),
        output: root.join(string(&data, "output", &path)?.unwrap_or("build/place.rbxlx")),
        tree,
        test,
        env,
        root,
    })
}
