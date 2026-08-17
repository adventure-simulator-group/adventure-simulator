use std::collections::HashMap;

use anyhow::Result;

use crate::object::{object_name, qualified_name};
use crate::reader::parse;
use crate::{Curve, Link, Node, NodeAnimation, Object, Prop, Take, TransformChannel};

/// FBX stores times as integer ticks of this many per second.
const TICKS_PER_SECOND: f64 = 46_186_158_000.0;

fn ticks_to_seconds(ticks: i64) -> f64 {
    ticks as f64 / TICKS_PER_SECOND
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

    /// Every animation take in the file.
    ///
    /// Takes with no animated node are dropped: FBX exporters routinely emit an
    /// empty stack alongside the real one.
    pub fn takes(&self) -> Vec<Take> {
        let mut targets: HashMap<i64, (i64, String)> = HashMap::new();
        for model in self.objects.iter().filter(|object| object.is_node()) {
            for (child, property) in self.children_with_property(model.id) {
                if let Some(property @ ("Lcl Translation" | "Lcl Rotation" | "Lcl Scaling")) =
                    property
                {
                    targets.insert(child.id, (model.id, property.to_string()));
                }
            }
        }

        self.objects
            .iter()
            .filter(|object| object.kind == "AnimationStack")
            .filter_map(|stack| {
                let mut nodes: Vec<NodeAnimation> = Vec::new();

                if let Some(layer) = self
                    .children(stack.id)
                    .find(|object| object.kind == "AnimationLayer")
                {
                    self.collect_layer(layer.id, &targets, &mut nodes);
                }
                if nodes.is_empty() {
                    return None;
                }

                let duration = self.stack_duration(stack).unwrap_or_else(|| {
                    let mut times = Vec::new();
                    for node in &nodes {
                        node.key_times(&mut times);
                    }
                    times.into_iter().fold(0.0, f64::max)
                });

                Some(Take {
                    name: stack.name.clone(),
                    duration,
                    nodes,
                })
            })
            .collect()
    }

    /// The stack's declared time span, when the exporter wrote one.
    fn stack_duration(&self, stack: &Object) -> Option<f64> {
        let start = stack.node.property70("LocalStart")?;
        let stop = stack.node.property70("LocalStop")?;
        let start = start.props.get(4).and_then(Prop::as_i64)?;
        let stop = stop.props.get(4).and_then(Prop::as_i64)?;
        let span = ticks_to_seconds(stop) - ticks_to_seconds(start);
        (span > 0.0).then_some(span)
    }

    /// Walks a layer's curve nodes and attributes each to the model and
    /// property it drives.
    fn collect_layer(
        &self,
        layer: i64,
        targets: &HashMap<i64, (i64, String)>,
        nodes: &mut Vec<NodeAnimation>,
    ) {
        for curve_node in self
            .children(layer)
            .filter(|object| object.kind == "AnimationCurveNode")
        {
            let Some((model, property)) = targets.get(&curve_node.id) else {
                continue;
            };
            let (model, property) = (*model, property.as_str());
            let channel = self.transform_channel(curve_node.id);
            if channel.is_empty() {
                continue;
            }

            let entry = match nodes.iter_mut().find(|entry| entry.node == model) {
                Some(entry) => entry,
                None => {
                    let name = self
                        .get(model)
                        .map(|object| object.qualified.clone())
                        .unwrap_or_default();
                    nodes.push(NodeAnimation {
                        node: model,
                        name,
                        translation: None,
                        rotation: None,
                        scale: None,
                    });
                    nodes.last_mut().expect("just pushed")
                }
            };

            match property {
                "Lcl Translation" => entry.translation = Some(channel),
                "Lcl Rotation" => entry.rotation = Some(channel),
                "Lcl Scaling" => entry.scale = Some(channel),
                _ => {}
            }
        }
    }

    /// A curve node's three axis curves plus its static defaults.
    fn transform_channel(&self, curve_node: i64) -> TransformChannel {
        let mut channel = TransformChannel::default();

        if let Some(object) = self.get(curve_node) {
            for (index, name) in ["d|X", "d|Y", "d|Z"].into_iter().enumerate() {
                if let Some(value) = object
                    .node
                    .property70(name)
                    .and_then(|property| property.props.get(4))
                    .and_then(Prop::as_f64)
                {
                    channel.default[index] = value;
                }
            }
        }

        for (curve, property) in self.children_with_property(curve_node) {
            if curve.kind != "AnimationCurve" {
                continue;
            }
            let Some(times) = curve
                .node
                .child("KeyTime")
                .and_then(|node| node.i64_array())
            else {
                continue;
            };
            let Some(values) = curve
                .node
                .child("KeyValueFloat")
                .and_then(|node| node.props.first().and_then(Prop::as_f64_array))
            else {
                continue;
            };
            let count = times.len().min(values.len());
            let curve = Curve {
                times: times[..count]
                    .iter()
                    .map(|t| ticks_to_seconds(*t))
                    .collect(),
                values: values[..count].to_vec(),
            };
            match property {
                Some("d|X") => channel.x = curve,
                Some("d|Y") => channel.y = curve,
                Some("d|Z") => channel.z = curve,
                _ => {}
            }
        }

        channel
    }
}
