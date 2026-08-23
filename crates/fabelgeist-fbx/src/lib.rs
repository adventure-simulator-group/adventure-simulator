//! Reader for binary FBX files (Kaydara FBX Binary, versions 7100-7700).
//!
//! What is modelled: the node/property tree, the object table under `Objects`,
//! the connection list including the property names that object-property links
//! carry, and the animation curves reachable through them. Object resolution
//! follows OpenFBX (the loader momentum itself uses), because joint ordering in
//! a momentum character is defined by the order connections appear in the file.
//!
//! This is a container reader. What the objects *mean* — which nodes are joints,
//! how FBX's transform chain composes, what units the file is in — is left to
//! callers, because rigs disagree about all three.
//!
//! Deliberately free of heavy dependencies (`anyhow` and a pure-Rust inflate),
//! so an asset pipeline can read FBX without pulling in a tensor runtime.

use std::collections::HashMap;
use std::io::Read;

use anyhow::{Context, Result, anyhow, bail};
use flate2::read::ZlibDecoder;

pub mod animation;

pub use animation::{Curve, NodeAnimation, Take, TransformChannel};

/// A typed FBX property value.
#[derive(Debug, Clone)]
pub enum Prop {
    I16(i16),
    Bool(bool),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    ArrF32(Vec<f32>),
    ArrF64(Vec<f64>),
    ArrI32(Vec<i32>),
    ArrI64(Vec<i64>),
    ArrBool(Vec<u8>),
    /// FBX strings are not UTF-8 in general; object names embed a `\0\x01` separator.
    Str(Vec<u8>),
    Raw(Vec<u8>),
}

impl Prop {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Prop::I16(v) => Some(*v as i64),
            Prop::I32(v) => Some(*v as i64),
            Prop::I64(v) => Some(*v),
            Prop::Bool(v) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Prop::F32(v) => Some(*v as f64),
            Prop::F64(v) => Some(*v),
            Prop::I16(v) => Some(*v as f64),
            Prop::I32(v) => Some(*v as f64),
            Prop::I64(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&[u8]> {
        match self {
            Prop::Str(v) | Prop::Raw(v) => Some(v),
            _ => None,
        }
    }

    /// Numeric array widened to `f64`, whatever the on-disk element type.
    pub fn as_f64_array(&self) -> Option<Vec<f64>> {
        match self {
            Prop::ArrF32(v) => Some(v.iter().map(|x| *x as f64).collect()),
            Prop::ArrF64(v) => Some(v.clone()),
            Prop::ArrI32(v) => Some(v.iter().map(|x| *x as f64).collect()),
            Prop::ArrI64(v) => Some(v.iter().map(|x| *x as f64).collect()),
            _ => None,
        }
    }

    /// Integer array widened to `i64`, whatever the on-disk element type.
    pub fn as_i64_array(&self) -> Option<Vec<i64>> {
        match self {
            Prop::ArrI32(v) => Some(v.iter().map(|x| *x as i64).collect()),
            Prop::ArrI64(v) => Some(v.clone()),
            Prop::ArrBool(v) => Some(v.iter().map(|x| *x as i64).collect()),
            _ => None,
        }
    }
}

/// One node record of the FBX tree.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub props: Vec<Prop>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn prop(&self, index: usize) -> Option<&Prop> {
        self.props.get(index)
    }

    /// First property as a numeric array, widened to `f64`.
    pub fn f64_array(&self) -> Option<Vec<f64>> {
        self.props.first()?.as_f64_array()
    }

    /// First property as an integer array, widened to `i64`.
    pub fn i64_array(&self) -> Option<Vec<i64>> {
        self.props.first()?.as_i64_array()
    }

    pub fn str_prop(&self, index: usize) -> Option<&[u8]> {
        self.props.get(index)?.as_str()
    }

    /// Look up an entry of this node's `Properties70` block by name.
    ///
    /// Mirrors OpenFBX `resolveProperty`: a `P` record whose first property is
    /// the requested name; values start at index 4 (index 3 for legacy P60).
    pub fn property70(&self, name: &str) -> Option<&Node> {
        let props = self.child("Properties70")?;
        props.children.iter().find(|p| {
            p.props
                .first()
                .and_then(|v| v.as_str())
                .is_some_and(|v| v == name.as_bytes())
        })
    }

    /// A three-component `Properties70` value such as `Lcl Translation`.
    pub fn property70_vec3(&self, name: &str, default: [f64; 3]) -> [f64; 3] {
        let Some(p) = self.property70(name) else {
            return default;
        };
        let mut out = default;
        for (i, slot) in out.iter_mut().enumerate() {
            match p.props.get(4 + i).and_then(Prop::as_f64) {
                Some(v) => *slot = v,
                None => return default,
            }
        }
        out
    }

    /// A scalar integer `Properties70` value such as `RotationOrder`.
    pub fn property70_i64(&self, name: &str, default: i64) -> i64 {
        self.property70(name)
            .and_then(|p| p.props.get(4))
            .and_then(Prop::as_i64)
            .unwrap_or(default)
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    version: u32,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| anyhow!("FBX read overflow"))?;
        if end > self.data.len() {
            bail!(
                "truncated FBX file: wanted {len} bytes at offset {}",
                self.pos
            );
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// Node headers went 32-bit -> 64-bit in FBX 7500.
    fn header_word(&mut self) -> Result<u64> {
        if self.version >= 7500 {
            self.u64()
        } else {
            Ok(self.u32()? as u64)
        }
    }

    fn header_size(&self) -> usize {
        // 3 header words + the 1-byte name length.
        if self.version >= 7500 { 25 } else { 13 }
    }
}

fn decode_array<T: Copy>(
    raw: &[u8],
    count: usize,
    encoding: u32,
    parse: impl Fn(&[u8]) -> T,
    width: usize,
) -> Result<Vec<T>> {
    let bytes = if encoding == 0 {
        raw.to_vec()
    } else {
        let mut out = Vec::with_capacity(count * width);
        ZlibDecoder::new(raw)
            .read_to_end(&mut out)
            .context("inflating FBX array property")?;
        out
    };
    if bytes.len() < count * width {
        bail!(
            "FBX array property is short: {} bytes for {count} x {width}",
            bytes.len()
        );
    }
    Ok((0..count)
        .map(|i| parse(&bytes[i * width..(i + 1) * width]))
        .collect())
}

fn read_props(reader: &mut Reader<'_>, count: usize) -> Result<Vec<Prop>> {
    let mut props = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = reader.u8()?;
        let prop = match kind {
            b'Y' => Prop::I16(i16::from_le_bytes(reader.take(2)?.try_into().unwrap())),
            b'C' => Prop::Bool(reader.u8()? != 0),
            b'I' => Prop::I32(i32::from_le_bytes(reader.take(4)?.try_into().unwrap())),
            b'F' => Prop::F32(f32::from_le_bytes(reader.take(4)?.try_into().unwrap())),
            b'D' => Prop::F64(f64::from_le_bytes(reader.take(8)?.try_into().unwrap())),
            b'L' => Prop::I64(i64::from_le_bytes(reader.take(8)?.try_into().unwrap())),
            b'f' | b'd' | b'l' | b'i' | b'b' => {
                let count = reader.u32()? as usize;
                let encoding = reader.u32()?;
                let compressed_len = reader.u32()? as usize;
                let raw = reader.take(compressed_len)?;
                match kind {
                    b'f' => Prop::ArrF32(decode_array(
                        raw,
                        count,
                        encoding,
                        |b| f32::from_le_bytes(b.try_into().unwrap()),
                        4,
                    )?),
                    b'd' => Prop::ArrF64(decode_array(
                        raw,
                        count,
                        encoding,
                        |b| f64::from_le_bytes(b.try_into().unwrap()),
                        8,
                    )?),
                    b'i' => Prop::ArrI32(decode_array(
                        raw,
                        count,
                        encoding,
                        |b| i32::from_le_bytes(b.try_into().unwrap()),
                        4,
                    )?),
                    b'l' => Prop::ArrI64(decode_array(
                        raw,
                        count,
                        encoding,
                        |b| i64::from_le_bytes(b.try_into().unwrap()),
                        8,
                    )?),
                    _ => Prop::ArrBool(decode_array(raw, count, encoding, |b| b[0], 1)?),
                }
            }
            b'S' => {
                let len = reader.u32()? as usize;
                Prop::Str(reader.take(len)?.to_vec())
            }
            b'R' => {
                let len = reader.u32()? as usize;
                Prop::Raw(reader.take(len)?.to_vec())
            }
            other => bail!("unknown FBX property type {:?}", other as char),
        };
        props.push(prop);
    }
    Ok(props)
}

/// Reads one node record. Returns `None` for the null record that terminates a list.
fn read_node(reader: &mut Reader<'_>) -> Result<Option<Node>> {
    let end_offset = reader.header_word()? as usize;
    let num_props = reader.header_word()? as usize;
    let _prop_list_len = reader.header_word()?;
    let name_len = reader.u8()? as usize;
    let name = String::from_utf8_lossy(reader.take(name_len)?).into_owned();

    if end_offset == 0 {
        return Ok(None);
    }

    let props = read_props(reader, num_props)?;

    let mut children = Vec::new();
    let sentinel = reader.header_size();
    while reader.pos + sentinel <= end_offset {
        match read_node(reader)? {
            Some(child) => children.push(child),
            None => break,
        }
    }
    reader.pos = end_offset;

    Ok(Some(Node {
        name,
        props,
        children,
    }))
}

/// Parses the top-level node list of a binary FBX file.
pub fn parse(data: &[u8]) -> Result<Vec<Node>> {
    const MAGIC: &[u8] = b"Kaydara FBX Binary  \x00";
    if data.len() < 27 || &data[..MAGIC.len()] != MAGIC {
        // ASCII FBX is a different format sharing the extension. Saying so is
        // more use than "not a binary FBX file", because the fix is a re-export.
        if data.starts_with(b"; FBX") || data.starts_with(b"\xef\xbb\xbf; FBX") {
            bail!("this is an ASCII FBX file; re-export it as binary FBX");
        }
        bail!("not a binary FBX file");
    }
    let version = u32::from_le_bytes(data[23..27].try_into().unwrap());
    let mut reader = Reader {
        data,
        pos: 27,
        version,
    };

    let mut roots = Vec::new();
    while reader.pos + reader.header_size() <= data.len() {
        match read_node(&mut reader)? {
            Some(node) => roots.push(node),
            None => break,
        }
    }
    Ok(roots)
}

/// An entry of the `Objects` block.
#[derive(Debug)]
pub struct Object {
    pub id: i64,
    /// Object name with the `\0\x01Class` suffix and any `namespace:` prefix removed.
    pub name: String,
    /// Object name with its namespace intact, e.g. `mixamorig:Hips`.
    ///
    /// Rig profiles are usually written against the namespaced name, so an
    /// importer wants this one even though momentum matches on the stripped one.
    pub qualified: String,
    /// The sub-class token, e.g. `LimbNode`, `Mesh`, `Cluster`, `BlendShapeChannel`.
    pub class: String,
    /// The record name, e.g. `Model`, `Geometry`, `Deformer`.
    pub kind: String,
    pub node: Node,
}

impl Object {
    /// OpenFBX maps `Model::Root` onto a limb node, which is why `body_world`
    /// becomes joint 0 of the MHR skeleton rather than a plain null node.
    pub fn is_limb(&self) -> bool {
        self.kind == "Model" && (self.class == "LimbNode" || self.class == "Root")
    }

    pub fn is_null_node(&self) -> bool {
        self.kind == "Model" && self.class == "Null"
    }

    pub fn is_node(&self) -> bool {
        self.kind == "Model"
    }
}

/// One entry of the connection list.
#[derive(Debug, Clone)]
pub struct Link {
    /// The object being connected in.
    pub from: i64,
    /// The property it connects to, for object-property (`OP`) links.
    ///
    /// Animation is addressed entirely through these: a curve node connects to
    /// a model's `Lcl Rotation`, and a curve connects to that node's `d|X`.
    pub property: Option<String>,
}

/// The object table plus the connection graph of an FBX file.
pub struct Scene {
    pub objects: Vec<Object>,
    /// The file's top-level records, kept for `GlobalSettings` and friends.
    pub roots: Vec<Node>,
    by_id: HashMap<i64, usize>,
    /// Incoming links per object id, in file order (id 0 is the scene root).
    links: HashMap<i64, Vec<Link>>,
}

fn object_name(raw: &[u8]) -> String {
    let name = match raw.windows(2).position(|w| w == [0, 1]) {
        Some(pos) => &raw[..pos],
        None => raw,
    };
    let name = String::from_utf8_lossy(name).into_owned();
    // momentum strips namespaces before matching joints against the .model file.
    match name.rfind(':') {
        Some(pos) => name[pos + 1..].to_string(),
        None => name,
    }
}

/// The object name with any namespace left on, e.g. `mixamorig:Hips`.
fn qualified_name(raw: &[u8]) -> String {
    let name = match raw.windows(2).position(|w| w == [0, 1]) {
        Some(pos) => &raw[..pos],
        None => raw,
    };
    String::from_utf8_lossy(name).into_owned()
}

impl Scene {
    pub fn from_roots(roots: Vec<Node>) -> Result<Self> {
        let mut objects = Vec::new();
        let mut by_id = HashMap::new();
        let mut links: HashMap<i64, Vec<Link>> = HashMap::new();

        for root in &roots {
            if root.name != "Objects" {
                continue;
            }
            for node in &root.children {
                let Some(id) = node.props.first().and_then(Prop::as_i64) else {
                    continue;
                };
                let name = node.str_prop(1).map(object_name).unwrap_or_default();
                let qualified = node.str_prop(1).map(qualified_name).unwrap_or_default();
                let class = node
                    .str_prop(2)
                    .map(|c| String::from_utf8_lossy(c).into_owned())
                    .unwrap_or_default();
                by_id.insert(id, objects.len());
                objects.push(Object {
                    id,
                    name,
                    qualified,
                    class,
                    kind: node.name.clone(),
                    node: node.clone(),
                });
            }
        }

        for root in &roots {
            if root.name != "Connections" {
                continue;
            }
            for c in &root.children {
                // C: [type, from, to, (property name)]
                let (Some(from), Some(to)) = (
                    c.props.get(1).and_then(Prop::as_i64),
                    c.props.get(2).and_then(Prop::as_i64),
                ) else {
                    continue;
                };
                if from == 0 {
                    continue;
                }
                let property = c
                    .props
                    .get(3)
                    .and_then(Prop::as_str)
                    .map(|name| String::from_utf8_lossy(name).into_owned());
                links.entry(to).or_default().push(Link { from, property });
            }
        }

        Ok(Self {
            objects,
            roots,
            by_id,
            links,
        })
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        Self::from_roots(parse(data)?)
    }

    pub fn get(&self, id: i64) -> Option<&Object> {
        self.by_id.get(&id).map(|i| &self.objects[*i])
    }

    /// A top-level record such as `GlobalSettings` or `Definitions`.
    pub fn root(&self, name: &str) -> Option<&Node> {
        self.roots.iter().find(|root| root.name == name)
    }

    /// Objects connected as children of `id`, in file order. This is OpenFBX's
    /// `resolveObjectLink` ordering, which fixes the joint order of the rig.
    pub fn children(&self, id: i64) -> impl Iterator<Item = &Object> {
        self.incoming(id).filter_map(|link| self.get(link.from))
    }

    /// As [`Scene::children`], keeping the property each link targets.
    pub fn children_with_property(&self, id: i64) -> impl Iterator<Item = (&Object, Option<&str>)> {
        self.incoming(id).filter_map(|link| {
            self.get(link.from)
                .map(|object| (object, link.property.as_deref()))
        })
    }

    fn incoming(&self, id: i64) -> impl Iterator<Item = &Link> {
        self.links.get(&id).map(Vec::as_slice).unwrap_or(&[]).iter()
    }

    /// The first child of `id` whose record name and class match.
    pub fn child_of_kind(&self, id: i64, kind: &str, class: &str) -> Option<&Object> {
        self.children(id)
            .find(|o| o.kind == kind && o.class == class)
    }

    pub fn objects_of_kind<'a>(
        &'a self,
        kind: &'a str,
        class: &'a str,
    ) -> impl Iterator<Item = &'a Object> {
        self.objects
            .iter()
            .filter(move |o| o.kind == kind && o.class == class)
    }
}
