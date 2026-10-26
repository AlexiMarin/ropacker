//! Convierte una carpeta en instancias, con las convenciones de Script Sync y Rojo:
//!
//! ```text
//! Nombre.luau          ModuleScript
//! Nombre.server.luau   Script
//! Nombre.client.luau   LocalScript
//! Nombre.local.luau    LocalScript
//! carpeta/             Folder (o el script de su init.luau / init.server.luau / init.client.luau, con lo demás adentro)
//! ```
//!
//! También .lua. Se saltan los archivos ocultos y todo lo que no sea Luau (se cuentan en `skipped`).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

const SUFFIXES: [(&str, &str); 8] = [
    // el más largo primero
    (".server.luau", "Script"),
    (".client.luau", "LocalScript"),
    (".local.luau", "LocalScript"),
    (".luau", "ModuleScript"),
    (".server.lua", "Script"),
    (".client.lua", "LocalScript"),
    (".local.lua", "LocalScript"),
    (".lua", "ModuleScript"),
];

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub cls: &'static str,
    pub source: Option<String>,
    pub children: Vec<Node>,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub nodes: Vec<Node>,
    pub scripts: usize,
    pub skipped: Vec<PathBuf>,
}

/// `Main.server.luau` -> `("Main", "Script")`; `None` si no es Luau.
pub fn script_kind(file_name: &str) -> Option<(&str, &'static str)> {
    SUFFIXES.iter().find_map(|(suffix, cls)| file_name.strip_suffix(suffix).map(|name| (name, *cls)))
}

/// Lo que hay dentro de `folder` (la carpeta misma no es una instancia).
pub fn scan(folder: &Path) -> Result<Scan> {
    let mut out = Scan::default();
    out.nodes = children(folder, &mut out)?;
    Ok(out)
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("{}", path.display()))
}

fn entries(folder: &Path) -> Result<Vec<PathBuf>> {
    fs::read_dir(folder)
        .and_then(|dir| dir.map(|e| e.map(|e| e.path())).collect())
        .with_context(|| format!("{}", folder.display()))
}

fn children(folder: &Path, out: &mut Scan) -> Result<Vec<Node>> {
    let mut paths = entries(folder)?;
    paths.sort_by_key(|p| file_name(p));
    let mut nodes = Vec::new();
    for path in paths {
        let name = file_name(&path);
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            nodes.push(folder_node(&path, out)?);
            continue;
        }
        let Some((name, cls)) = script_kind(&name) else {
            out.skipped.push(path);
            continue;
        };
        if name == "init" {
            continue; // lo toma folder_node de la carpeta que lo contiene
        }
        out.scripts += 1;
        nodes.push(Node { name: name.to_string(), cls, source: Some(read(&path)?), children: Vec::new() });
    }
    let mut seen = HashSet::new();
    for n in &nodes {
        if !seen.insert(&n.name) {
            bail!("{}: hay dos cosas llamadas {}", folder.display(), n.name);
        }
    }
    Ok(nodes)
}

fn folder_node(path: &Path, out: &mut Scan) -> Result<Node> {
    let mut init = None;
    for child in entries(path)? {
        if let Some(("init", cls)) = script_kind(&file_name(&child)) {
            if init.is_some() {
                bail!("{}: más de un init", path.display());
            }
            init = Some((child, cls));
        }
    }
    let children = children(path, out)?;
    let name = file_name(path);
    let Some((file, cls)) = init else {
        return Ok(Node { name, cls: "Folder", source: None, children });
    };
    out.scripts += 1;
    Ok(Node { name, cls, source: Some(read(&file)?), children })
}
