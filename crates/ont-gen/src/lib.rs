//! The text of one Rust module from the bytes of one schema file: a type per
//! object type, value type, action type and function, both directions of
//! every link type, and call functions that build each request and send it
//! through the consumer's [`Engine`](#the-engine). The contract is
//! `specs/ont/gen.md`.
//!
//! # Use
//!
//! A consumer's build script calls [`generate`] on the schema file and writes
//! the text under `OUT_DIR`; the crate includes it at item position. The
//! examples below include the committed module of the synthetic fixture
//! `tests/fixtures/example.schema.json`, and each refusal of the compiler
//! stands beside a block that compiles and differs from it in one name.
//!
//! # The engine
//!
//! An engine that answers each request with the request itself:
//!
//! ```
//! include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/generated/example.rs"));
//! use example::{Command, Engine};
//! struct Echo;
//! impl Engine for Echo {
//!     type Error = ();
//!     fn call(&mut self, _: Command, request: serde_json::Value) -> Result<serde_json::Value, ()> {
//!         Ok(request)
//!     }
//! }
//! fn main() {
//!     let action = example::actions::AddShelf {
//!         shelf: example::Id::new("north"),
//!         label: "North".into(),
//!     };
//!     let sent = example::apply(&mut Echo, &action, &example::Call::default());
//!     assert_eq!(sent.unwrap()["action_type"], "add_shelf");
//! }
//! ```
//!
//! # An action the schema does not have
//!
//! spec: specs/ont/gen.md#R8
//!
//! ```
//! include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/generated/example.rs"));
//! fn main() {
//!     let action = example::actions::AddShelf {
//!         shelf: example::Id::new("north"),
//!         label: "North".into(),
//!     };
//!     let _ = action;
//! }
//! ```
//!
//! ```compile_fail,E0422
//! include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/generated/example.rs"));
//! fn main() {
//!     let action = example::actions::MoveShelf {
//!         shelf: example::Id::new("north"),
//!         label: "North".into(),
//!     };
//!     let _ = action;
//! }
//! ```
//!
//! # A parameter the action does not have
//!
//! spec: specs/ont/gen.md#R7
//!
//! ```
//! include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/generated/example.rs"));
//! fn main() {
//!     let action = example::actions::AddShelf {
//!         shelf: example::Id::new("north"),
//!         label: "North".into(),
//!     };
//!     let _ = action;
//! }
//! ```
//!
//! ```compile_fail,E0560
//! include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/generated/example.rs"));
//! fn main() {
//!     let action = example::actions::AddShelf {
//!         shelf: example::Id::new("north"),
//!         title: "North".into(),
//!     };
//!     let _ = action;
//! }
//! ```
//!
//! # A reference to another object type
//!
//! spec: specs/ont/gen.md#R5
//!
//! ```
//! include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/generated/example.rs"));
//! use example::objects;
//! fn main() {
//!     let shelf = example::Id::<objects::Shelf>::new("north");
//!     let action = example::actions::AddShelf { shelf, label: "North".into() };
//!     let _ = action;
//! }
//! ```
//!
//! ```compile_fail,E0308
//! include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/generated/example.rs"));
//! use example::objects;
//! fn main() {
//!     let shelf = example::Id::<objects::Item>::new("north");
//!     let action = example::actions::AddShelf { shelf, label: "North".into() };
//!     let _ = action;
//! }
//! ```
//!
//! # A path whose second hop starts at another type
//!
//! spec: specs/ont/gen.md#R10
//!
//! ```
//! include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/generated/example.rs"));
//! use example::{Command, Engine, links};
//! struct Echo;
//! impl Engine for Echo {
//!     type Error = ();
//!     fn call(&mut self, _: Command, request: serde_json::Value) -> Result<serde_json::Value, ()> {
//!         Ok(request)
//!     }
//! }
//! fn main() {
//!     let path = (links::ItemShelf, links::ItemShelfReverse);
//!     let _sent = example::around(&mut Echo, None, path, None);
//! }
//! ```
//!
//! ```compile_fail,E0271
//! include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/generated/example.rs"));
//! use example::{Command, Engine, links};
//! struct Echo;
//! impl Engine for Echo {
//!     type Error = ();
//!     fn call(&mut self, _: Command, request: serde_json::Value) -> Result<serde_json::Value, ()> {
//!         Ok(request)
//!     }
//! }
//! fn main() {
//!     let path = (links::ItemShelf, links::ItemShelf);
//!     let _sent = example::around(&mut Echo, None, path, None);
//! }
//! ```
use ont_core::schema::{
    ActionType, CheckedSchema, FunctionDef, LinkType, ObjectType, PropertyType, ScalarType, Schema,
    ValueTypeDef, Violation, valid_name,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// The code of the generator's own refusal (R1 of `specs/ont/gen.md`).
const NAME_COLLISION: &str = "name_collision";

/// The text of one Rust module for the bytes of one schema file, or the
/// faults that refuse it (R1 of `specs/ont/gen.md`).
pub fn generate(schema: &[u8]) -> Result<String, Vec<Violation>> {
    let checked = Schema::check_bytes(schema, &BTreeSet::new())?;
    let plan = Plan::new(checked.schema())?;
    Ok(render(&checked, &plan))
}

/// Edition 2024 keywords, strict and reserved, that a raw identifier can write.
const RAW_KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn",
    "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
    "virtual", "where", "while", "yield",
];

/// A field, a module or a variable: the schema's name, raw when it is a
/// keyword, with a trailing underscore when it cannot be raw (R4).
fn field_name(name: &str) -> String {
    if matches!(name, "self" | "super" | "crate") {
        format!("{name}_")
    } else if RAW_KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

/// A type or a variant: each part between `_` with its first letter in
/// upper case, joined; `Self` is written `Self_` (R4).
fn type_name(name: &str) -> String {
    let mut out = String::new();
    for part in name.split('_') {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.extend(chars);
        }
    }
    if out == "Self" {
        out.push('_');
    }
    out
}

/// A string literal: the quote, the backslash and every character outside
/// U+0020 to U+007E written as escapes (R3).
fn literal(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            _ => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
        }
    }
    out.push('"');
    out
}

/// A description as the text of a one-line `//` comment: every control
/// character and every character that changes the direction of text is a
/// space (R3).
fn comment_text(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control()
                || ('\u{202a}'..='\u{202e}').contains(&c)
                || ('\u{2066}'..='\u{2069}').contains(&c)
            {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn escape_pointer(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}

/// The names the module defines in one place, with the pointer of the schema
/// name each came from; the second name of one Rust name is refused.
#[derive(Default)]
struct Scope(BTreeMap<String, String>);
impl Scope {
    fn add(&mut self, rust: String, pointer: String, faults: &mut Vec<Violation>) {
        if self.0.contains_key(&rust) {
            faults.push(Violation {
                code: NAME_COLLISION,
                pointer,
                message: Some(format!("gives the Rust name {rust} a name before it gives")),
            });
        } else {
            self.0.insert(rust, pointer);
        }
    }
}

/// A parameter of an action or a function.
struct Parameter<'a> {
    description: Option<&'a str>,
    ty: &'a PropertyType,
    required: bool,
}

/// An action or a function, as the module writes it.
struct Callable<'a> {
    name: &'a str,
    description: Option<&'a str>,
    parameters: Vec<(&'a str, Parameter<'a>)>,
    /// The JSON pointer of the entry, for a refusal.
    pointer: String,
}

impl<'a> Callable<'a> {
    fn action(name: &'a str, at: &'a ActionType) -> Self {
        Self {
            name,
            description: at.description.as_deref(),
            parameters: at
                .parameters
                .iter()
                .map(|(p, def)| {
                    (
                        p.as_str(),
                        Parameter {
                            description: def.description.as_deref(),
                            ty: &def.ty,
                            required: def.required,
                        },
                    )
                })
                .collect(),
            pointer: format!("/action_types/{}", escape_pointer(name)),
        }
    }
    fn function(name: &'a str, def: &'a FunctionDef) -> Self {
        Self {
            name,
            description: def.description.as_deref(),
            parameters: def
                .parameters
                .iter()
                .map(|(p, def)| {
                    (
                        p.as_str(),
                        Parameter {
                            description: def.description.as_deref(),
                            ty: &def.ty,
                            required: def.required,
                        },
                    )
                })
                .collect(),
            pointer: format!("/functions/{}", escape_pointer(name)),
        }
    }
    /// The struct a parameter's `{struct}` type gives, at its depth under
    /// arrays: its members and the pointer of its type.
    fn structs(&self) -> Vec<(&'a str, &'a BTreeMap<String, PropertyType>, String)> {
        let mut out = Vec::new();
        for (name, p) in &self.parameters {
            let mut ty = p.ty;
            let mut pointer = format!("{}/parameters/{}/type", self.pointer, escape_pointer(name));
            loop {
                match ty {
                    PropertyType::Array(a) => {
                        ty = a.array.as_ref();
                        pointer.push_str("/array");
                    }
                    PropertyType::Struct(s) => {
                        out.push((*name, &s.struct_, pointer));
                        break;
                    }
                    _ => break,
                }
            }
        }
        out
    }
    fn struct_name(&self, parameter: &str) -> String {
        type_name(&format!("{}_{parameter}", self.name))
    }
}

/// The value types written as an enum: their variants in the order of the
/// `enum` (R6).
struct Plan<'a> {
    enums: BTreeMap<&'a str, Vec<(String, String)>>,
    actions: Vec<Callable<'a>>,
    functions: Vec<Callable<'a>>,
}

impl<'a> Plan<'a> {
    fn new(schema: &'a Schema) -> Result<Self, Vec<Violation>> {
        let mut faults = Vec::new();
        let mut objects = Scope::default();
        for name in schema.object_types.keys() {
            objects.add(
                type_name(name),
                format!("/object_types/{}", escape_pointer(name)),
                &mut faults,
            );
        }
        for (name, ty) in &schema.object_types {
            let base = format!("/object_types/{}", escape_pointer(name));
            let mut variants = Scope::default();
            for property in ty.properties.keys() {
                variants.add(
                    type_name(property),
                    format!("{base}/properties/{}", escape_pointer(property)),
                    &mut faults,
                );
            }
        }
        let mut values = Scope::default();
        let mut enums = BTreeMap::new();
        for (name, def) in &schema.value_types {
            values.add(
                type_name(name),
                format!("/value_types/{}", escape_pointer(name)),
                &mut faults,
            );
            if let Some(variants) = enum_variants(def) {
                enums.insert(name.as_str(), variants);
            }
        }
        let actions: Vec<Callable<'a>> = schema
            .action_types
            .iter()
            .map(|(n, at)| Callable::action(n, at))
            .collect();
        let functions: Vec<Callable<'a>> = schema
            .functions
            .iter()
            .map(|(n, def)| Callable::function(n, def))
            .collect();
        for kind in [&actions, &functions] {
            let mut items = Scope::default();
            for callable in kind {
                items.add(
                    type_name(callable.name),
                    callable.pointer.clone(),
                    &mut faults,
                );
                let mut fields = Scope::default();
                for (p, _) in &callable.parameters {
                    fields.add(
                        field_name(p),
                        format!("{}/parameters/{}", callable.pointer, escape_pointer(p)),
                        &mut faults,
                    );
                }
                for (p, members, pointer) in callable.structs() {
                    items.add(callable.struct_name(p), pointer.clone(), &mut faults);
                    let mut fields = Scope::default();
                    for member in members.keys() {
                        fields.add(
                            field_name(member),
                            format!("{pointer}/struct/{}", escape_pointer(member)),
                            &mut faults,
                        );
                    }
                }
            }
        }
        let mut links = Scope::default();
        for name in schema.link_types.keys() {
            let pointer = format!("/link_types/{}", escape_pointer(name));
            links.add(type_name(name), pointer.clone(), &mut faults);
            links.add(format!("{}Reverse", type_name(name)), pointer, &mut faults);
        }
        if faults.is_empty() {
            Ok(Self {
                enums,
                actions,
                functions,
            })
        } else {
            Err(faults)
        }
    }
}

/// The variants of a value type written as an enum, or `None` for the
/// struct form (R6): (variant, schema value).
fn enum_variants(def: &ValueTypeDef) -> Option<Vec<(String, String)>> {
    if def.base != ScalarType::String {
        return None;
    }
    let values: &[serde_json::Value] = def.enum_values.as_deref()?;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for value in values {
        let text = value.as_str().filter(|s| valid_name(s))?;
        let variant = type_name(text);
        if !seen.insert(variant.clone()) {
            return None;
        }
        out.push((variant, text.to_string()));
    }
    Some(out)
}

fn scalar_type(base: ScalarType) -> &'static str {
    match base {
        ScalarType::String | ScalarType::Decimal | ScalarType::Date => "::std::string::String",
        ScalarType::Integer | ScalarType::Timestamp => "i64",
        ScalarType::Boolean => "bool",
    }
}

/// The Rust type of a field, inside a submodule of the module (R7).
fn rust_type(ty: &PropertyType, struct_name: &str) -> String {
    match ty {
        PropertyType::Scalar(s) => scalar_type(*s).to_string(),
        PropertyType::Ref(r) => format!("super::Id<super::objects::{}>", type_name(&r.target)),
        PropertyType::Array(a) => format!("::std::vec::Vec<{}>", rust_type(&a.array, struct_name)),
        PropertyType::ValueType(v) => format!("super::values::{}", type_name(&v.value_type)),
        PropertyType::Struct(_) => struct_name.to_string(),
    }
}

/// The text, indented by four spaces per level.
struct Out {
    text: String,
    depth: usize,
}
impl Out {
    fn line(&mut self, line: &str) {
        if line.starts_with('}') || line.starts_with(')') || line.starts_with(']') {
            self.depth = self.depth.saturating_sub(1);
        }
        if !line.is_empty() {
            for _ in 0..self.depth {
                self.text.push_str("    ");
            }
            self.text.push_str(line);
        }
        self.text.push('\n');
        if line.ends_with('{') || line.ends_with('(') || line.ends_with('[') {
            self.depth += 1;
        }
    }
    fn comment(&mut self, description: Option<&str>) {
        if let Some(d) = description {
            self.line(&format!("// {}", comment_text(d)).trim_end().to_string());
        }
    }
}

fn render(checked: &CheckedSchema, plan: &Plan<'_>) -> String {
    let schema = checked.schema();
    let pin = checked.pin();
    let mut o = Out {
        text: String::new(),
        depth: 0,
    };
    o.line(&format!(
        "// Generated by ont-gen from the schema of the domain {}, version {}, sha256 {}.",
        pin.domain, pin.version, pin.sha256
    ));
    o.line("#[allow(");
    for lint in [
        "dead_code",
        "non_snake_case",
        "non_camel_case_types",
        "clippy::upper_case_acronyms",
        "clippy::enum_variant_names",
        "clippy::module_inception",
    ] {
        o.line(&format!("{lint},"));
    }
    o.line(")]");
    o.line(&format!("pub mod {} {{", field_name(&schema.domain)));
    let lines = [
        format!("pub const DOMAIN: &str = {};", literal(&pin.domain)),
        format!("pub const VERSION: u64 = {};", pin.version),
        format!("pub const SHA256: &str = {};", literal(&pin.sha256)),
        format!(
            "const INSTANCE: &str = {};",
            literal(&schema.namespace.instance)
        ),
        format!(
            "const ACCOUNT: &str = {};",
            literal(&schema.namespace.account)
        ),
    ];
    for line in &lines {
        o.line(line);
    }
    for line in FIXED.lines() {
        o.line(line);
    }
    objects(&mut o, schema);
    properties(&mut o, schema);
    values(&mut o, schema, plan);
    callables(&mut o, "actions", "Action", &plan.actions);
    callables(&mut o, "functions", "Function", &plan.functions);
    links(&mut o, schema);
    o.line("}");
    o.text
}

fn objects(o: &mut Out, schema: &Schema) {
    o.line("");
    o.line("pub mod objects {");
    for (name, ty) in &schema.object_types {
        let t = type_name(name);
        o.comment(object_description(ty));
        o.line("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]");
        o.line(&format!("pub struct {t};"));
        o.line(&format!("impl super::ObjectType for {t} {{"));
        o.line(&format!("const NAME: &'static str = {};", literal(name)));
        o.line("}");
    }
    o.line("}");
}

fn object_description(ty: &ObjectType) -> Option<&str> {
    ty.description.as_deref()
}

fn properties(o: &mut Out, schema: &Schema) {
    o.line("");
    o.line("pub mod properties {");
    for (name, ty) in &schema.object_types {
        let t = type_name(name);
        o.line("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        o.line(&format!("pub enum {t} {{"));
        for (property, def) in &ty.properties {
            o.comment(def.description.as_deref());
            o.line(&format!("{},", type_name(property)));
        }
        o.line("}");
        o.line(&format!("impl {t} {{"));
        o.line("pub fn name(&self) -> &'static str {");
        if ty.properties.is_empty() {
            o.line("match *self {}");
        } else {
            o.line("match self {");
            for property in ty.properties.keys() {
                o.line(&format!(
                    "Self::{} => {},",
                    type_name(property),
                    literal(property)
                ));
            }
            o.line("}");
        }
        o.line("}");
        o.line("}");
    }
    o.line("}");
}

fn values(o: &mut Out, schema: &Schema, plan: &Plan<'_>) {
    o.line("");
    o.line("pub mod values {");
    for (name, def) in &schema.value_types {
        let t = type_name(name);
        if let Some(variants) = plan.enums.get(name.as_str()) {
            o.line("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
            o.line(&format!("pub enum {t} {{"));
            for (variant, _) in variants {
                o.line(&format!("{variant},"));
            }
            o.line("}");
            o.line(&format!("impl {t} {{"));
            o.line("pub fn as_str(&self) -> &'static str {");
            o.line("match self {");
            for (variant, value) in variants {
                o.line(&format!("Self::{variant} => {},", literal(value)));
            }
            o.line("}");
            o.line("}");
            o.line("pub fn parse(text: &str) -> ::std::option::Option<Self> {");
            o.line("match text {");
            for (variant, value) in variants {
                o.line(&format!(
                    "{} => ::std::option::Option::Some(Self::{variant}),",
                    literal(value)
                ));
            }
            o.line("_ => ::std::option::Option::None,");
            o.line("}");
            o.line("}");
            o.line("}");
            o.line(&format!("impl super::Json for {t} {{"));
            o.line("fn json(&self) -> ::serde_json::Value {");
            o.line("::serde_json::Value::String(::std::string::String::from(self.as_str()))");
            o.line("}");
            o.line("}");
        } else {
            o.line("#[derive(Debug, Clone, PartialEq, Eq, Hash)]");
            o.line(&format!("pub struct {t}(pub {});", scalar_type(def.base)));
            o.line(&format!("impl super::Json for {t} {{"));
            o.line("fn json(&self) -> ::serde_json::Value {");
            o.line("super::Json::json(&self.0)");
            o.line("}");
            o.line("}");
        }
    }
    o.line("}");
}

/// The body of `parameters` or `json`: an object of the fields, an optional
/// one only when it is given.
fn object_body(o: &mut Out, fields: &[(&str, bool)]) {
    if fields.is_empty() {
        o.line("::serde_json::Value::Object(::serde_json::Map::new())");
        return;
    }
    o.line("let mut object = ::serde_json::Map::new();");
    for (name, required) in fields {
        let field = field_name(name);
        if *required {
            o.line(&format!(
                "object.insert(::std::string::String::from({}), super::Json::json(&self.{field}));",
                literal(name)
            ));
        } else {
            o.line(&format!(
                "if let ::std::option::Option::Some(value) = &self.{field} {{"
            ));
            o.line(&format!(
                "object.insert(::std::string::String::from({}), super::Json::json(value));",
                literal(name)
            ));
            o.line("}");
        }
    }
    o.line("::serde_json::Value::Object(object)");
}

fn callables(o: &mut Out, module: &str, trait_name: &str, list: &[Callable<'_>]) {
    o.line("");
    o.line(&format!("pub mod {module} {{"));
    for callable in list {
        let t = type_name(callable.name);
        o.comment(callable.description);
        o.line("#[derive(Debug, Clone, PartialEq, Eq, Hash)]");
        o.line(&format!("pub struct {t} {{"));
        for (name, p) in &callable.parameters {
            let ty = rust_type(p.ty, &callable.struct_name(name));
            let ty = if p.required {
                ty
            } else {
                format!("::std::option::Option<{ty}>")
            };
            o.comment(p.description);
            o.line(&format!("pub {}: {ty},", field_name(name)));
        }
        o.line("}");
        o.line(&format!("impl super::{trait_name} for {t} {{"));
        o.line(&format!(
            "const NAME: &'static str = {};",
            literal(callable.name)
        ));
        o.line("fn parameters(&self) -> ::serde_json::Value {");
        let fields: Vec<(&str, bool)> = callable
            .parameters
            .iter()
            .map(|(n, p)| (*n, p.required))
            .collect();
        object_body(o, &fields);
        o.line("}");
        o.line("}");
        for (parameter, members, _) in callable.structs() {
            let s = callable.struct_name(parameter);
            o.line("#[derive(Debug, Clone, PartialEq, Eq, Hash)]");
            o.line(&format!("pub struct {s} {{"));
            for (member, ty) in members {
                // A struct holds no struct (`struct_in_struct`).
                o.line(&format!(
                    "pub {}: {},",
                    field_name(member),
                    rust_type(ty, &s)
                ));
            }
            o.line("}");
            o.line(&format!("impl super::Json for {s} {{"));
            o.line("fn json(&self) -> ::serde_json::Value {");
            let fields: Vec<(&str, bool)> = members.keys().map(|m| (m.as_str(), true)).collect();
            object_body(o, &fields);
            o.line("}");
            o.line("}");
        }
    }
    o.line("}");
}

fn links(o: &mut Out, schema: &Schema) {
    o.line("");
    o.line("pub mod links {");
    for (name, link) in &schema.link_types {
        let LinkType {
            from, to, names, ..
        } = link;
        let t = type_name(name);
        for (hop, start, end, hop_name) in [
            (t.clone(), from, to, &names.forward),
            (format!("{t}Reverse"), to, from, &names.reverse),
        ] {
            o.line("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]");
            o.line(&format!("pub struct {hop};"));
            o.line(&format!("impl super::Hop for {hop} {{"));
            o.line(&format!(
                "type From = super::objects::{};",
                type_name(start)
            ));
            o.line(&format!("type To = super::objects::{};", type_name(end)));
            o.line(&format!(
                "const NAME: &'static str = {};",
                literal(hop_name)
            ));
            o.line("}");
        }
    }
    o.line("}");
}

/// The items every module holds, whatever its schema (R5, R8 to R11).
const FIXED: &str = r#"
pub trait ObjectType {
const NAME: &'static str;
}

pub struct Id<T: ObjectType> {
instance: ::std::string::String,
account: ::std::string::String,
native_key: ::std::string::String,
object_type: ::std::marker::PhantomData<fn() -> T>,
}
impl<T: ObjectType> Id<T> {
pub fn new(native_key: impl ::std::convert::Into<::std::string::String>) -> Self {
Self::in_namespace(INSTANCE, ACCOUNT, native_key)
}
pub fn in_namespace(
instance: impl ::std::convert::Into<::std::string::String>,
account: impl ::std::convert::Into<::std::string::String>,
native_key: impl ::std::convert::Into<::std::string::String>,
) -> Self {
Self {
instance: instance.into(),
account: account.into(),
native_key: native_key.into(),
object_type: ::std::marker::PhantomData,
}
}
pub fn native_key(&self) -> &str {
&self.native_key
}
pub fn to_value(&self) -> ::serde_json::Value {
let mut namespace = ::serde_json::Map::new();
namespace.insert(::std::string::String::from("instance"), ::serde_json::Value::String(self.instance.clone()));
namespace.insert(::std::string::String::from("account"), ::serde_json::Value::String(self.account.clone()));
let mut id = ::serde_json::Map::new();
id.insert(::std::string::String::from("object_type"), ::serde_json::Value::from(T::NAME));
id.insert(::std::string::String::from("namespace"), ::serde_json::Value::Object(namespace));
id.insert(::std::string::String::from("native_key"), ::serde_json::Value::String(self.native_key.clone()));
::serde_json::Value::Object(id)
}
pub fn from_value(value: &::serde_json::Value) -> ::std::option::Option<Self> {
fn text<'a>(object: &'a ::serde_json::Map<::std::string::String, ::serde_json::Value>, key: &str) -> ::std::option::Option<&'a str> {
object.get(key)?.as_str().filter(|s| !s.is_empty() && s.len() <= 512)
}
let id = value.as_object()?;
let namespace = id.get("namespace")?.as_object()?;
if id.len() != 3 || namespace.len() != 2 || text(id, "object_type")? != T::NAME {
return ::std::option::Option::None;
}
::std::option::Option::Some(Self::in_namespace(text(namespace, "instance")?, text(namespace, "account")?, text(id, "native_key")?))
}
}
impl<T: ObjectType> ::std::clone::Clone for Id<T> {
fn clone(&self) -> Self {
Self::in_namespace(self.instance.clone(), self.account.clone(), self.native_key.clone())
}
}
impl<T: ObjectType> ::std::cmp::PartialEq for Id<T> {
fn eq(&self, other: &Self) -> bool {
self.instance == other.instance && self.account == other.account && self.native_key == other.native_key
}
}
impl<T: ObjectType> ::std::cmp::Eq for Id<T> {}
impl<T: ObjectType> ::std::hash::Hash for Id<T> {
fn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {
::std::hash::Hash::hash(&self.instance, state);
::std::hash::Hash::hash(&self.account, state);
::std::hash::Hash::hash(&self.native_key, state);
}
}
impl<T: ObjectType> ::std::fmt::Debug for Id<T> {
fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
f.debug_struct("Id")
.field("object_type", &T::NAME)
.field("instance", &self.instance)
.field("account", &self.account)
.field("native_key", &self.native_key)
.finish()
}
}

trait Json {
fn json(&self) -> ::serde_json::Value;
}
impl Json for ::std::string::String {
fn json(&self) -> ::serde_json::Value {
::serde_json::Value::String(self.clone())
}
}
impl Json for i64 {
fn json(&self) -> ::serde_json::Value {
::serde_json::Value::from(*self)
}
}
impl Json for bool {
fn json(&self) -> ::serde_json::Value {
::serde_json::Value::Bool(*self)
}
}
impl<T: ObjectType> Json for Id<T> {
fn json(&self) -> ::serde_json::Value {
self.to_value()
}
}
impl<X: Json> Json for ::std::vec::Vec<X> {
fn json(&self) -> ::serde_json::Value {
::serde_json::Value::Array(self.iter().map(Json::json).collect())
}
}

pub trait Action {
const NAME: &'static str;
fn parameters(&self) -> ::serde_json::Value;
}

pub trait Function {
const NAME: &'static str;
fn parameters(&self) -> ::serde_json::Value;
}

pub trait Hop {
type From: ObjectType;
type To: ObjectType;
const NAME: &'static str;
}

pub trait Path {
type From: ObjectType;
type To: ObjectType;
fn names(&self) -> ::std::vec::Vec<&'static str>;
}
impl<A: Hop> Path for (A,) {
type From = <A as Hop>::From;
type To = <A as Hop>::To;
fn names(&self) -> ::std::vec::Vec<&'static str> {
::std::vec![A::NAME]
}
}
impl<A: Hop, B: Hop<From = <A as Hop>::To>> Path for (A, B) {
type From = <A as Hop>::From;
type To = <B as Hop>::To;
fn names(&self) -> ::std::vec::Vec<&'static str> {
::std::vec![A::NAME, B::NAME]
}
}
impl<A: Hop, B: Hop<From = <A as Hop>::To>, C: Hop<From = <B as Hop>::To>> Path for (A, B, C) {
type From = <A as Hop>::From;
type To = <C as Hop>::To;
fn names(&self) -> ::std::vec::Vec<&'static str> {
::std::vec![A::NAME, B::NAME, C::NAME]
}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
ActionApply,
ActionValidate,
Get,
Search,
Around,
Fn(&'static str),
}
impl Command {
pub fn argv(&self) -> ::std::vec::Vec<&'static str> {
match *self {
Self::ActionApply => ::std::vec!["action", "apply"],
Self::ActionValidate => ::std::vec!["action", "validate"],
Self::Get => ::std::vec!["get"],
Self::Search => ::std::vec!["search"],
Self::Around => ::std::vec!["around"],
Self::Fn(name) => ::std::vec!["fn", name],
}
}
}

pub trait Engine {
type Error;
fn call(&mut self, command: Command, request: ::serde_json::Value) -> ::std::result::Result<::serde_json::Value, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Call {
pub request_id: ::std::option::Option<::std::string::String>,
pub agent: ::std::option::Option<::std::string::String>,
pub session: ::std::option::Option<::std::string::String>,
pub role: ::std::option::Option<::std::string::String>,
pub expected: ::std::vec::Vec<(::serde_json::Value, ::std::string::String)>,
}

fn action_request<A: Action>(action: &A, call: &Call) -> ::serde_json::Value {
let mut request = ::serde_json::Map::new();
request.insert(::std::string::String::from("action_type"), ::serde_json::Value::from(A::NAME));
request.insert(::std::string::String::from("parameters"), action.parameters());
for (key, value) in [
("request_id", &call.request_id),
("agent", &call.agent),
("session", &call.session),
("role", &call.role),
] {
if let ::std::option::Option::Some(value) = value {
request.insert(::std::string::String::from(key), ::serde_json::Value::String(value.clone()));
}
}
if !call.expected.is_empty() {
let mut expected = ::std::vec::Vec::new();
for (id, revision) in &call.expected {
let mut head = ::serde_json::Map::new();
head.insert(::std::string::String::from("id"), id.clone());
head.insert(::std::string::String::from("revision"), ::serde_json::Value::String(revision.clone()));
expected.push(::serde_json::Value::Object(head));
}
request.insert(::std::string::String::from("expected"), ::serde_json::Value::Array(expected));
}
::serde_json::Value::Object(request)
}

pub fn apply<A: Action, E: Engine>(engine: &mut E, action: &A, call: &Call) -> ::std::result::Result<::serde_json::Value, E::Error> {
engine.call(Command::ActionApply, action_request(action, call))
}

pub fn validate<A: Action, E: Engine>(engine: &mut E, action: &A, call: &Call) -> ::std::result::Result<::serde_json::Value, E::Error> {
engine.call(Command::ActionValidate, action_request(action, call))
}

pub fn get<T: ObjectType, E: Engine>(engine: &mut E, id: &Id<T>) -> ::std::result::Result<::serde_json::Value, E::Error> {
let mut request = ::serde_json::Map::new();
request.insert(::std::string::String::from("id"), id.to_value());
engine.call(Command::Get, ::serde_json::Value::Object(request))
}

pub fn search<T: ObjectType, E: Engine>(
engine: &mut E,
filter: ::std::option::Option<::serde_json::Value>,
limit: ::std::option::Option<u64>,
continuation: ::std::option::Option<&str>,
) -> ::std::result::Result<::serde_json::Value, E::Error> {
let mut request = ::serde_json::Map::new();
request.insert(::std::string::String::from("object_type"), ::serde_json::Value::from(T::NAME));
if let ::std::option::Option::Some(filter) = filter {
request.insert(::std::string::String::from("where"), filter);
}
if let ::std::option::Option::Some(limit) = limit {
request.insert(::std::string::String::from("limit"), ::serde_json::Value::from(limit));
}
if let ::std::option::Option::Some(continuation) = continuation {
request.insert(::std::string::String::from("continuation"), ::serde_json::Value::from(continuation));
}
engine.call(Command::Search, ::serde_json::Value::Object(request))
}

pub fn around<P: Path, E: Engine>(
engine: &mut E,
filter: ::std::option::Option<::serde_json::Value>,
path: P,
limit: ::std::option::Option<u64>,
) -> ::std::result::Result<::serde_json::Value, E::Error> {
let mut from = ::serde_json::Map::new();
from.insert(::std::string::String::from("object_type"), ::serde_json::Value::from(<P::From as ObjectType>::NAME));
if let ::std::option::Option::Some(filter) = filter {
from.insert(::std::string::String::from("where"), filter);
}
let mut request = ::serde_json::Map::new();
request.insert(::std::string::String::from("from"), ::serde_json::Value::Object(from));
request.insert(
::std::string::String::from("path"),
::serde_json::Value::Array(path.names().into_iter().map(::serde_json::Value::from).collect()),
);
if let ::std::option::Option::Some(limit) = limit {
request.insert(::std::string::String::from("limit"), ::serde_json::Value::from(limit));
}
engine.call(Command::Around, ::serde_json::Value::Object(request))
}

pub fn call<F: Function, E: Engine>(engine: &mut E, function: &F) -> ::std::result::Result<::serde_json::Value, E::Error> {
let mut request = ::serde_json::Map::new();
request.insert(::std::string::String::from("parameters"), function.parameters());
request.insert(::std::string::String::from("domain"), ::serde_json::Value::from(DOMAIN));
engine.call(Command::Fn(F::NAME), ::serde_json::Value::Object(request))
}
"#;
