//! Arma el .rbxlx: parte del lugar base (exportado de Studio como .rbxlx) y en cada ruta de [tree] cambia los
//! hijos con el mismo nombre por lo que hay en disco. Lo demás del lugar (Workspace, ajustes de servicios, lo que se
//! hizo a mano en Studio) queda igual. Sin base sale un lugar con solo esos servicios.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::tree::Node;
use crate::xml::{Content, Document, Element};

const XMIME: &str = "http://www.w3.org/2005/05/xmlmime";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Un lugar de Roblox: un documento XML cuya raíz es `<roblox>`.
#[derive(Debug, Clone)]
pub struct Place(Document);

pub fn empty() -> Place {
    let root = Element::new(
        "roblox",
        &[
            ("xmlns:xmime", XMIME),
            ("xmlns:xsi", XSI),
            ("xsi:noNamespaceSchemaLocation", "http://www.roblox.com/roblox.xsd"),
            ("version", "4"),
        ],
    );
    Place(Document { nodes: vec![Content::Element(root)] })
}

pub fn read(path: &Path) -> Result<Place> {
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("rbxlx")) {
        bail!("{}: el lugar base tiene que ser .rbxlx (en Studio: File > Save to File As > .rbxlx)", path.display());
    }
    let text = fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    let doc = Document::parse(&text).with_context(|| format!("{}", path.display()))?;
    if doc.root().map(|r| r.name.as_str()) != Some("roblox") {
        bail!("{}: no es un lugar de Roblox", path.display());
    }
    Ok(Place(doc))
}

pub fn name_of(item: &Element) -> Option<String> {
    let props = item.child("Properties")?;
    props.elements().find(|p| p.attr("name").as_deref() == Some("Name")).map(Element::text)
}

pub fn items(parent: &Element) -> impl Iterator<Item = &Element> {
    parent.elements().filter(|e| e.name == "Item")
}

fn make(cls: &str, name: &str, source: Option<&str>) -> Element {
    let referent = format!("RBX{}", uuid::Uuid::new_v4().simple()).to_uppercase();
    let mut props = Element::new("Properties", &[]);
    props.push(Element::new("string", &[("name", "Name")]).with_text(name));
    if let Some(source) = source {
        props.push(Element::new("ProtectedString", &[("name", "Source")]).with_text(source));
    }
    let mut item = Element::new("Item", &[("class", cls), ("referent", &referent)]);
    item.push(props);
    item
}

/// Agrega `child` y devuelve su posición: arriba de todo, los Item van antes de SharedStrings, como lo escribe Studio.
fn append(parent: &mut Element, child: Element) -> usize {
    let k = parent
        .children
        .iter()
        .position(|c| matches!(c, Content::Element(e) if e.name == "SharedStrings"))
        .unwrap_or(parent.children.len());
    parent.children.insert(k, Content::Element(child));
    k
}

/// 'StarterPlayer.StarterPlayerScripts' -> ese Item; lo que falte se crea (clase = nombre: son servicios).
fn resolve<'a>(root: &'a mut Element, path: &str) -> &'a mut Element {
    let mut node = root;
    for (depth, seg) in path.split('.').enumerate() {
        let found = node.children.iter().position(|c| match c {
            Content::Element(e) => {
                e.name == "Item"
                    && (name_of(e).as_deref() == Some(seg) || (depth == 0 && e.attr("class").as_deref() == Some(seg)))
            }
            _ => false,
        });
        let k = match found {
            Some(k) => k,
            None => append(node, make(seg, seg, None)),
        };
        node = match &mut node.children[k] {
            Content::Element(e) => e,
            _ => unreachable!(),
        };
    }
    node
}

fn build_item(node: &Node) -> Element {
    let mut item = make(node.cls, &node.name, node.source.as_deref());
    for child in &node.children {
        item.push(build_item(child));
    }
    item
}

impl Place {
    pub fn root(&self) -> &Element {
        self.0.root().expect("un Place siempre tiene raíz")
    }

    /// Pone `nodes` dentro de `path` cambiando los hijos con el mismo nombre. Devuelve cuántos se cambiaron.
    pub fn place(&mut self, path: &str, nodes: &[Node]) -> usize {
        let root = self.0.root_mut().expect("un Place siempre tiene raíz");
        let target = resolve(root, path);
        let names: HashSet<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        let before = target.children.len();
        target.children.retain(|c| match c {
            Content::Element(e) if e.name == "Item" => !name_of(e).is_some_and(|n| names.contains(n.as_str())),
            _ => true,
        });
        let replaced = before - target.children.len();
        for n in nodes {
            append(target, build_item(n));
        }
        replaced
    }

    pub fn to_xml(&self) -> String {
        self.0.to_xml()
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("{}", dir.display()))?;
        }
        fs::write(path, self.to_xml()).with_context(|| format!("{}", path.display()))
    }
}
