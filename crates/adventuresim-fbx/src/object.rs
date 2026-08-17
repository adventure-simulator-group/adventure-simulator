use crate::Node;

/// An entry of the `Objects` block.
#[derive(Debug, Clone)]
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

pub(crate) fn object_name(raw: &[u8]) -> String {
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
pub(crate) fn qualified_name(raw: &[u8]) -> String {
    let name = match raw.windows(2).position(|w| w == [0, 1]) {
        Some(pos) => &raw[..pos],
        None => raw,
    };
    String::from_utf8_lossy(name).into_owned()
}
