//! Un árbol XML mínimo que se escribe igual que se leyó: guarda el texto, los CDATA, los comentarios y los
//! prefijos (`xsi:`, `xmime:`) tal como vienen del archivo de Studio, para no tocar lo que ropacker no cambia.

use anyhow::{bail, Context, Result};
use quick_xml::escape::{escape, partial_escape, unescape};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

#[derive(Debug, Clone)]
pub enum Content {
    Element(Element),
    /// texto tal como va en el archivo (con `&amp;`, `&lt;`...)
    Text(String),
    CData(String),
    /// comentarios, declaración, instrucciones: se copian igual
    Other(String),
}

#[derive(Debug, Clone)]
pub struct Element {
    pub name: String,
    /// valores tal como van en el archivo (escapados)
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Content>,
    self_closing: bool,
}

impl Element {
    pub fn new(name: &str, attrs: &[(&str, &str)]) -> Element {
        Element {
            name: name.to_string(),
            attrs: attrs.iter().map(|(k, v)| (k.to_string(), escape(*v).into_owned())).collect(),
            children: Vec::new(),
            self_closing: false,
        }
    }

    pub fn with_text(mut self, text: &str) -> Element {
        self.children.push(Content::Text(partial_escape(text).into_owned()));
        self
    }

    pub fn push(&mut self, child: Element) {
        self.children.push(Content::Element(child));
    }

    pub fn attr(&self, key: &str) -> Option<String> {
        let (_, raw) = self.attrs.iter().find(|(k, _)| k == key)?;
        Some(unescape(raw).map_or_else(|_| raw.clone(), |v| v.into_owned()))
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| match c {
            Content::Element(e) => Some(e),
            _ => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for c in &self.children {
            match c {
                Content::Text(raw) => out += &unescape(raw).map_or_else(|_| raw.clone(), |v| v.into_owned()),
                Content::CData(s) => out += s,
                _ => {}
            }
        }
        out
    }

    fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (k, v) in &self.attrs {
            out.push_str(&format!(" {k}=\"{}\"", v.replace('"', "&quot;")));
        }
        if self.self_closing && self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for c in &self.children {
            c.write(out);
        }
        out.push_str(&format!("</{}>", self.name));
    }
}

impl Content {
    fn write(&self, out: &mut String) {
        match self {
            Content::Element(e) => e.write(out),
            Content::Text(s) | Content::Other(s) => out.push_str(s),
            Content::CData(s) => out.push_str(&format!("<![CDATA[{s}]]>")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub nodes: Vec<Content>,
}

fn utf8(bytes: &[u8]) -> Result<String> {
    Ok(std::str::from_utf8(bytes)?.to_string())
}

fn open(start: &BytesStart, self_closing: bool) -> Result<Element> {
    let mut attrs = Vec::new();
    for a in start.attributes() {
        let a = a?;
        attrs.push((utf8(a.key.as_ref())?, utf8(&a.value)?));
    }
    Ok(Element { name: utf8(start.name().as_ref())?, attrs, children: Vec::new(), self_closing })
}

impl Document {
    pub fn parse(text: &str) -> Result<Document> {
        let mut reader = Reader::from_str(text);
        let mut open_elements: Vec<Element> = Vec::new();
        let mut top = Vec::new();
        loop {
            let at = reader.buffer_position() as usize;
            let event = reader.read_event().with_context(|| format!("XML inválido cerca del byte {at}"))?;
            let raw = &text[at..reader.buffer_position() as usize];
            let node = match event {
                Event::Start(e) => {
                    open_elements.push(open(&e, false)?);
                    continue;
                }
                Event::End(_) => Content::Element(open_elements.pop().context("cierre sin apertura")?),
                Event::Empty(e) => Content::Element(open(&e, true)?),
                Event::Text(_) => Content::Text(raw.to_string()),
                Event::CData(c) => Content::CData(utf8(&c)?),
                Event::Eof => break,
                _ => Content::Other(raw.to_string()),
            };
            match open_elements.last_mut() {
                Some(parent) => parent.children.push(node),
                None => top.push(node),
            }
        }
        if let Some(e) = open_elements.last() {
            bail!("<{}> sin cerrar", e.name);
        }
        Ok(Document { nodes: top })
    }

    pub fn root(&self) -> Option<&Element> {
        self.nodes.iter().find_map(|c| match c {
            Content::Element(e) => Some(e),
            _ => None,
        })
    }

    pub fn root_mut(&mut self) -> Option<&mut Element> {
        self.nodes.iter_mut().find_map(|c| match c {
            Content::Element(e) => Some(e),
            _ => None,
        })
    }

    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        for c in &self.nodes {
            c.write(&mut out);
        }
        out
    }
}
