use crate::Prop;

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
