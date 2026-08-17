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
