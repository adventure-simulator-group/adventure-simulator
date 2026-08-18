//! Walks reflected [`TypeInfo`] and decides, for every field reachable from
//! a BRP-queryable Component/Resource, both a Python type annotation and
//! the exact `to_brp()`/`from_brp()` wire encoding for it.
//!
//! The reflected *shape* of a type is not always its wire *encoding* - see
//! wiki/reference/tactical-testing.md's "to_brp()/from_brp()" section for
//! the full behavioral spec this implements:
//!
//! - A single-field tuple struct (`CharacterId(u64)`) is transparent on the
//!   wire: as a *field* of some other type it inlines directly (no wrapper
//!   class); as a top-level Component/Resource it still gets a class (with
//!   a single `value: T` field) purely so the BrpClient API has a uniform
//!   class to work with, but `to_brp()` returns the bare value.
//! - `Option<T>` is `None` or a bare `T`, not `{"Some": ...}`.
//! - A small set of `glam` types have a hand-written `Serialize` impl the
//!   reflected shape can't see - hardcoded to their known
//!   `list[float]`/`list[int]` wire shape.
//! - A unit-only enum becomes a `Literal[...]` of its variant names.
//! - Anything else the resolver doesn't confidently know how to encode (a
//!   data-carrying enum, a `Map`, a `Set`, a multi-field tuple struct, an
//!   opaque type with no reflected fields) resolves to `Any` with a
//!   `# unresolved: <type path>` comment rather than a guess.

use std::collections::{BTreeMap, HashMap};

use bevy::reflect::{TypeInfo, enums::VariantInfo, enums::VariantType};

/// `glam` float-vector types with a hand-coded `Serialize` impl the
/// reflected shape can't see - hardcoded to `list[float]` with a known
/// element count instead of resolved generically as a struct.
const GLAM_FLOAT_TYPES: &[(&str, usize)] = &[
    ("glam::Vec2", 2),
    ("glam::Vec3", 3),
    ("glam::Vec3A", 3),
    ("glam::Vec4", 4),
    ("glam::Quat", 4),
];

/// Same as [`GLAM_FLOAT_TYPES`] but for integer vector types, encoded as
/// `list[int]`.
const GLAM_INT_TYPES: &[(&str, usize)] = &[
    ("glam::IVec2", 2),
    ("glam::IVec3", 3),
    ("glam::IVec4", 4),
    ("glam::UVec2", 2),
    ("glam::UVec3", 3),
    ("glam::UVec4", 4),
];

const INT_PRIMITIVES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];
const FLOAT_PRIMITIVES: &[&str] = &["f32", "f64"];

/// `Entity` has a hand-written `Serialize` impl (a packed 64-bit id) the
/// reflected shape can't see, same rationale as the glam vector types -
/// hardcoded to `int` rather than resolved generically.
const ENTITY_TYPE: &str = "bevy_ecs::entity::Entity";

/// Python keywords can't be used as identifiers anywhere, including a
/// dataclass field name - a struct field named e.g. `global` still needs a
/// distinct wire key (`"global"`) from its Python attribute/parameter name
/// (`global_`).
const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// The Python identifier used for a field named `name` in code (attribute
/// declarations, `self.<ident>`, `cls(<ident>=...)`) - unchanged unless
/// `name` collides with a Python keyword, in which case a trailing
/// underscore disambiguates it. The *wire* key (dict key / `data[...]`
/// subscript) always stays the original `name`.
pub fn python_ident(name: &str) -> String {
    if PYTHON_KEYWORDS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StructKind {
    Component,
    Resource,
    Nested,
}

/// How a single field (or, for a top-level bare-value wrapper, the whole
/// type) is represented in Python: its type annotation, its trailing
/// comment (if any), and how to convert to/from the BRP wire value.
#[derive(Clone, Debug)]
pub enum Encoding {
    Bool,
    Int,
    Float,
    Str,
    /// Not confidently resolvable - encoded as `Any`, wire value passed
    /// through unchanged, with a `# unresolved: <type path>` comment.
    Any(String),
    /// One of the hardcoded glam vector types - `list[float]`/`list[int]`
    /// with a known length, never a class.
    GlamList {
        len: usize,
        ints: bool,
    },
    /// A `Vec<T>` (`len: None`) or fixed-size `[T; N]` (`len: Some(N)`).
    ListLike {
        elem: Box<Encoding>,
        len: Option<usize>,
    },
    /// `Option<T>` - `None` or a bare `T` on the wire.
    OptionOf(Box<Encoding>),
    /// A reference to a memoized, separately-defined class (Struct-shaped,
    /// or a top-level tuple-struct/enum's own wrapper class when it is
    /// itself the referenced type - see [`Resolver::get_or_create`]).
    Class(String),
    /// A unit-only enum - the bare variant-name string on the wire.
    Literal(Vec<String>),
}

impl Encoding {
    /// Whether `to_brp`/`from_brp` for this encoding is just the identity
    /// function - used to decide whether an enclosing `Option`/`Vec` needs
    /// the walrus-operator None-check/comprehension wrapping, or can just
    /// pass the raw value through untouched.
    fn is_identity(&self) -> bool {
        match self {
            Encoding::Bool
            | Encoding::Int
            | Encoding::Float
            | Encoding::Str
            | Encoding::Any(_)
            | Encoding::Literal(_) => true,
            Encoding::OptionOf(inner) => inner.is_identity(),
            Encoding::GlamList { .. } | Encoding::ListLike { .. } | Encoding::Class(_) => false,
        }
    }

    pub fn annotation(&self) -> String {
        match self {
            Encoding::Bool => "bool".to_string(),
            Encoding::Int => "int".to_string(),
            Encoding::Float => "float".to_string(),
            Encoding::Str => "str".to_string(),
            Encoding::Any(_) => "Any".to_string(),
            Encoding::GlamList { ints, .. } => {
                if *ints {
                    "list[int]".to_string()
                } else {
                    "list[float]".to_string()
                }
            }
            Encoding::ListLike { elem, .. } => format!("list[{}]", elem.annotation()),
            Encoding::OptionOf(inner) => format!("{} | None", inner.annotation()),
            Encoding::Class(name) => name.clone(),
            Encoding::Literal(variants) => {
                let quoted: Vec<String> = variants.iter().map(|v| format!("\"{v}\"")).collect();
                format!("Literal[{}]", quoted.join(", "))
            }
        }
    }

    /// The trailing `# unresolved: ...`/`# len N` comment for a field of
    /// this encoding, if any. Only ever looks through a single `Option`
    /// wrapper and a single `Vec` wrapper (matching the observed baseline -
    /// a `Vec<Vec3>` field gets no comment at all, since the length only
    /// describes the *inner* glam type, not the outer list).
    pub fn comment(&self) -> Option<String> {
        match self {
            Encoding::OptionOf(inner) => inner.comment(),
            Encoding::Any(path) => Some(format!("# unresolved: {path}")),
            Encoding::GlamList { len, .. } => Some(format!("# len {len}")),
            Encoding::ListLike { len: Some(n), .. } => Some(format!("# len {n}")),
            Encoding::ListLike { elem, len: None } => match elem.as_ref() {
                Encoding::Any(path) => Some(format!("# unresolved: {path}")),
                _ => None,
            },
            _ => None,
        }
    }

    /// A Python expression converting `expr` (already the raw
    /// attribute/dict-value) into its BRP wire form. `depth` numbers
    /// nested comprehension/walrus variables (`v0`, `v1`, ...) and is
    /// shared across one top-level call.
    pub fn to_brp_expr(&self, expr: &str, depth: &mut u32) -> String {
        match self {
            Encoding::Bool
            | Encoding::Int
            | Encoding::Float
            | Encoding::Str
            | Encoding::Any(_)
            | Encoding::Literal(_) => expr.to_string(),
            Encoding::GlamList { .. } => format!("list({expr})"),
            Encoding::ListLike { elem, .. } => {
                if elem.is_identity() {
                    format!("list({expr})")
                } else {
                    let v = format!("v{depth}");
                    *depth += 1;
                    let inner = elem.to_brp_expr(&v, depth);
                    format!("[{inner} for {v} in {expr}]")
                }
            }
            Encoding::OptionOf(inner) => {
                if inner.is_identity() {
                    expr.to_string()
                } else {
                    let v = format!("v{depth}");
                    *depth += 1;
                    let inner_expr = inner.to_brp_expr(&v, depth);
                    format!("(({inner_expr}) if ({v} := {expr}) is not None else None)")
                }
            }
            Encoding::Class(_) => format!("{expr}.to_brp()"),
        }
    }

    /// The reverse of [`Self::to_brp_expr`].
    pub fn from_brp_expr(&self, expr: &str, depth: &mut u32) -> String {
        match self {
            Encoding::Bool
            | Encoding::Int
            | Encoding::Float
            | Encoding::Str
            | Encoding::Any(_)
            | Encoding::Literal(_) => expr.to_string(),
            Encoding::GlamList { .. } => format!("list({expr})"),
            Encoding::ListLike { elem, .. } => {
                if elem.is_identity() {
                    format!("list({expr})")
                } else {
                    let v = format!("v{depth}");
                    *depth += 1;
                    let inner = elem.from_brp_expr(&v, depth);
                    format!("[{inner} for {v} in {expr}]")
                }
            }
            Encoding::OptionOf(inner) => {
                if inner.is_identity() {
                    expr.to_string()
                } else {
                    let v = format!("v{depth}");
                    *depth += 1;
                    let inner_expr = inner.from_brp_expr(&v, depth);
                    format!("(({inner_expr}) if ({v} := {expr}) is not None else None)")
                }
            }
            Encoding::Class(name) => format!("{name}.from_brp({expr})"),
        }
    }
}

pub struct FieldDef {
    pub name: String,
    pub encoding: Encoding,
}

pub enum Body {
    /// A named-field struct - encoded as a `{"field": ...}` dict.
    Fields(Vec<FieldDef>),
    /// Any other top-level shape (tuple-struct newtype, unit-only enum,
    /// glam type, unresolved, ...) - encoded as the bare wrapped value,
    /// with a single synthetic `value` field for the generated class.
    Transparent(Encoding),
}

pub struct ClassDef {
    pub type_path: String,
    pub class_name: String,
    pub kind: StructKind,
    pub body: Body,
}

pub struct Resolver {
    /// Keyed by full Rust type path - `BTreeMap` so iterating it back out
    /// is already sorted the way every section of the generated file is
    /// (Components/Resources/Nested each print their members in type-path
    /// order).
    pub classes: BTreeMap<String, ClassDef>,
    names_used: HashMap<String, u32>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            classes: BTreeMap::new(),
            names_used: HashMap::new(),
        }
    }

    /// Entry point for a type discovered directly in the `AppTypeRegistry`
    /// via `ReflectComponent`/`ReflectResource` type data.
    pub fn resolve_top_level(
        &mut self,
        type_path: &str,
        type_info: &'static TypeInfo,
        kind: StructKind,
    ) {
        self.get_or_create(type_path, Some(type_info), Some(kind));
    }

    /// Resolves (memoized by type path) the class for a Struct-shaped
    /// type, or the transparent wrapper for anything else. Used both for
    /// top-level Components/Resources and for nested helper types
    /// discovered while walking fields.
    fn get_or_create(
        &mut self,
        type_path: &str,
        type_info: Option<&'static TypeInfo>,
        preferred_kind: Option<StructKind>,
    ) -> String {
        if let Some(existing) = self.classes.get_mut(type_path) {
            if let Some(kind) = preferred_kind {
                if existing.kind == StructKind::Nested {
                    existing.kind = kind;
                }
            }
            return existing.class_name.clone();
        }

        let short_path = type_info
            .map(|ti| ti.type_path_table().short_path())
            .unwrap_or(type_path);
        let class_name = self.reserve_name(short_path);

        // Insert a placeholder before recursing so a self-referential type
        // (or a reference cycle between two types) resolves to a `Class`
        // reference rather than looping forever - the class' name is
        // reserved up front, its body is filled in below regardless of how
        // deep the recursion went in between.
        self.classes.insert(
            type_path.to_string(),
            ClassDef {
                type_path: type_path.to_string(),
                class_name: class_name.clone(),
                kind: preferred_kind.unwrap_or(StructKind::Nested),
                body: Body::Fields(Vec::new()),
            },
        );

        let body = match type_info {
            Some(TypeInfo::Struct(info)) => {
                let fields = info
                    .iter()
                    .map(|field| FieldDef {
                        name: field.name().to_string(),
                        encoding: self.resolve_type(field.type_info(), field.type_path()),
                    })
                    .collect();
                Body::Fields(fields)
            }
            _ => Body::Transparent(self.resolve_type(type_info, type_path)),
        };

        let entry = self.classes.get_mut(type_path).expect("just inserted");
        entry.body = body;
        class_name
    }

    fn reserve_name(&mut self, short_path: &str) -> String {
        let clean: String = short_path
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        let count = self.names_used.entry(clean.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            clean
        } else {
            format!("{clean}{}", *count)
        }
    }

    /// Resolves a single field's (or a top-level type's own) reflected
    /// type into its wire [`Encoding`]. See the module docs for the rules
    /// this applies, in priority order.
    pub fn resolve_type(
        &mut self,
        type_info: Option<&'static TypeInfo>,
        type_path: &str,
    ) -> Encoding {
        if let Some((_, len)) = GLAM_FLOAT_TYPES.iter().find(|(path, _)| *path == type_path) {
            return Encoding::GlamList {
                len: *len,
                ints: false,
            };
        }
        if let Some((_, len)) = GLAM_INT_TYPES.iter().find(|(path, _)| *path == type_path) {
            return Encoding::GlamList {
                len: *len,
                ints: true,
            };
        }
        if type_path == "bool" {
            return Encoding::Bool;
        }
        if FLOAT_PRIMITIVES.contains(&type_path) {
            return Encoding::Float;
        }
        if INT_PRIMITIVES.contains(&type_path) {
            return Encoding::Int;
        }
        if type_path == "alloc::string::String" {
            return Encoding::Str;
        }
        if type_path == ENTITY_TYPE {
            return Encoding::Int;
        }

        if type_path.starts_with("core::option::Option<") {
            if let Some(TypeInfo::Enum(enum_info)) = type_info {
                if let Some(VariantInfo::Tuple(some_variant)) = enum_info.variant("Some") {
                    if let Some(field) = some_variant.field_at(0) {
                        let inner = self.resolve_type(field.type_info(), field.type_path());
                        return Encoding::OptionOf(Box::new(inner));
                    }
                }
            }
            return Encoding::Any(type_path.to_string());
        }

        match type_info {
            None => Encoding::Any(type_path.to_string()),
            Some(TypeInfo::Struct(_)) => {
                Encoding::Class(self.get_or_create(type_path, type_info, None))
            }
            Some(TypeInfo::TupleStruct(info)) => {
                if info.field_len() == 1 {
                    let field = info.field_at(0).expect("field_len() == 1");
                    self.resolve_type(field.type_info(), field.type_path())
                } else {
                    // Multi-field (or zero-field) tuple structs don't have
                    // a single known wire shape - left unresolved rather
                    // than guessed.
                    Encoding::Any(type_path.to_string())
                }
            }
            Some(TypeInfo::Enum(enum_info)) => {
                if enum_info
                    .iter()
                    .all(|variant| variant.variant_type() == VariantType::Unit)
                {
                    Encoding::Literal(
                        enum_info
                            .iter()
                            .map(|variant| variant.name().to_string())
                            .collect(),
                    )
                } else {
                    Encoding::Any(type_path.to_string())
                }
            }
            Some(TypeInfo::List(info)) => {
                let elem = self.resolve_type(info.item_info(), info.item_ty().path());
                Encoding::ListLike {
                    elem: Box::new(elem),
                    len: None,
                }
            }
            Some(TypeInfo::Array(info)) => {
                let elem = self.resolve_type(info.item_info(), info.item_ty().path());
                Encoding::ListLike {
                    elem: Box::new(elem),
                    len: Some(info.capacity()),
                }
            }
            // `Tuple` (bare unnamed tuples), `Map`, `Set`, and `Opaque`
            // (`Duration`, `Handle<T>`, `Color`, remaining primitives we
            // don't special-case, ...) have no generically-known wire
            // shape.
            Some(TypeInfo::Tuple(_))
            | Some(TypeInfo::Map(_))
            | Some(TypeInfo::Set(_))
            | Some(TypeInfo::Opaque(_)) => Encoding::Any(type_path.to_string()),
        }
    }
}
