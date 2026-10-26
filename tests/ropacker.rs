use std::fs;
use std::path::Path;

use ropacker::rbxlx::{self, items, name_of};
use ropacker::tree;
use ropacker::xml::{Content, Document, Element};

const BASE: &str = r#"<roblox xmlns:xmime="http://www.w3.org/2005/05/xmlmime" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="http://www.roblox.com/roblox.xsd" version="4">
<Meta name="ExplicitAutoJoints">true</Meta>
<Item class="Workspace" referent="RBX1"><Properties><string name="Name">Workspace</string></Properties>
  <Item class="Part" referent="RBX2"><Properties><string name="Name">Baseplate</string></Properties></Item>
</Item>
<Item class="ReplicatedStorage" referent="RBX3"><Properties><string name="Name">ReplicatedStorage</string></Properties>
  <Item class="Folder" referent="RBX4"><Properties><string name="Name">Shared</string></Properties>
    <Item class="ModuleScript" referent="RBX5"><Properties><string name="Name">Old</string><ProtectedString name="Source"><![CDATA[return 1]]></ProtectedString></Properties></Item>
  </Item>
  <Item class="RemoteEvent" referent="RBX6"><Properties><string name="Name">MadeInStudio</string></Properties></Item>
</Item>
<SharedStrings></SharedStrings>
</roblox>"#;

fn write(root: &Path, files: &[(&str, &str)]) {
    for (rel, text) in files {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
}

fn find<'a>(mut item: &'a Element, names: &[&str]) -> &'a Element {
    for name in names {
        item = items(item).find(|c| name_of(c).as_deref() == Some(*name)).unwrap_or_else(|| panic!("no está {name}"));
    }
    item
}

#[test]
fn kinds() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        &[
            ("Shared/Board.luau", "return {}"),
            ("Server/Main.server.luau", "print(1)"),
            ("Client/Main.local.luau", "print(2)"),
            ("Client/Ui.client.luau", "print(3)"),
            ("Lib/init.luau", "return 'lib'"),
            ("Lib/Part.luau", "return 1"),
            ("Shared/notes.md", "x"),
            (".DS_Store", "x"),
        ],
    );
    let scan = tree::scan(d.path()).unwrap();
    let by = |name: &str| scan.nodes.iter().find(|n| n.name == name).unwrap();
    let mut names: Vec<_> = scan.nodes.iter().map(|n| n.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["Client", "Lib", "Server", "Shared"]);
    assert_eq!(by("Shared").cls, "Folder");
    assert_eq!(by("Shared").children[0].cls, "ModuleScript");
    assert_eq!(by("Server").children[0].cls, "Script");
    assert!(by("Client").children.iter().all(|c| c.cls == "LocalScript"));
    assert_eq!((by("Lib").cls, by("Lib").source.as_deref()), ("ModuleScript", Some("return 'lib'")));
    assert_eq!(by("Lib").children.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Part"]);
    assert_eq!(scan.scripts, 6);
    assert_eq!(scan.skipped.len(), 1);
}

#[test]
fn duplicate_names_fail() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), &[("A.luau", ""), ("A.server.luau", "")]);
    assert!(tree::scan(d.path()).unwrap_err().to_string().contains("dos cosas llamadas A"));
}

#[test]
fn merge_keeps_the_rest() {
    let d = tempfile::tempdir().unwrap();
    let d = d.path();
    fs::write(d.join("base.rbxlx"), BASE).unwrap();
    write(d, &[("shared/Shared/New.luau", "return ']]>' -- & <x>"), ("client/Client/Main.local.luau", "print(1)")]);
    let mut place = rbxlx::read(&d.join("base.rbxlx")).unwrap();
    assert_eq!(place.place("ReplicatedStorage", &tree::scan(&d.join("shared")).unwrap().nodes), 1);
    assert_eq!(place.place("StarterPlayer.StarterPlayerScripts", &tree::scan(&d.join("client")).unwrap().nodes), 0);
    let out = d.join("out/place.rbxlx");
    place.write(&out).unwrap();

    let text = fs::read_to_string(&out).unwrap();
    let doc = Document::parse(&text).unwrap();
    let again = doc.root().unwrap();
    find(again, &["Workspace", "Baseplate"]);
    find(again, &["ReplicatedStorage", "MadeInStudio"]);
    let shared = find(again, &["ReplicatedStorage", "Shared"]);
    assert_eq!(items(shared).map(|c| name_of(c).unwrap()).collect::<Vec<_>>(), ["New"]);
    let source = items(shared).next().unwrap().child("Properties").unwrap().child("ProtectedString").unwrap();
    assert_eq!(source.text(), "return ']]>' -- & <x>");
    let main = find(again, &["StarterPlayer", "StarterPlayerScripts", "Client", "Main"]);
    assert_eq!(main.attr("class").as_deref(), Some("LocalScript"));
    // los servicios nuevos van antes de SharedStrings y se conservan los prefijos de Studio
    assert_eq!(again.elements().last().unwrap().name, "SharedStrings");
    assert!(text.contains("xsi:noNamespaceSchemaLocation"));
    assert!(!text.contains("ns0:"));
    let mut refs = Vec::new();
    fn walk(e: &Element, refs: &mut Vec<String>) {
        if e.name == "Item" {
            refs.push(e.attr("referent").unwrap());
        }
        e.elements().for_each(|c| walk(c, refs));
    }
    walk(again, &mut refs);
    let unique: std::collections::HashSet<_> = refs.iter().collect();
    assert_eq!(refs.len(), unique.len());
}

#[test]
fn untouched_parts_are_byte_for_byte() {
    // lo que no está en [tree] (CDATA, comentarios, entidades) sale igual que entró
    let xml = "<roblox version=\"4\"><!-- hola --><Item class=\"Part\" referent=\"A\"><Properties>\
               <string name=\"Name\">a &amp; b</string><BinaryString name=\"Tags\"></BinaryString>\
               <ProtectedString name=\"Source\"><![CDATA[x < y]]></ProtectedString></Properties></Item></roblox>";
    let doc = Document::parse(xml).unwrap();
    assert_eq!(doc.to_xml(), xml);
    assert_eq!(name_of(doc.root().unwrap().elements().next().unwrap()).as_deref(), Some("a & b"));
    assert!(matches!(doc.nodes[0], Content::Element(_)));
}

#[test]
fn empty_place_is_valid() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), &[("src/Hi.server.luau", "print('hi')")]);
    let mut place = rbxlx::empty();
    place.place("ServerScriptService", &tree::scan(&d.path().join("src")).unwrap().nodes);
    let doc = Document::parse(&place.to_xml()).unwrap();
    let hi = find(doc.root().unwrap(), &["ServerScriptService", "Hi"]);
    assert_eq!(hi.attr("class").as_deref(), Some("Script"));
}
