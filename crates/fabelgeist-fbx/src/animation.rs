//! Animation takes, as FBX stores them.
//!
//! FBX addresses animation through connections rather than containment:
//!
//! ```text
//! AnimationStack ("Take 001")
//!   └── AnimationLayer
//!         └── AnimationCurveNode ──OP:"Lcl Rotation"──> Model
//!               ├── AnimationCurve ──OP:"d|X"
//!               ├── AnimationCurve ──OP:"d|Y"
//!               └── AnimationCurve ──OP:"d|Z"
//! ```
//!
//! Times come back in seconds. Values come back exactly as the file stores
//! them — rotations in degrees, in the target node's own rotation order, before
//! any pre/post-rotation — because composing those is a rig question, not a
//! container question.

use crate::{Prop, Scene};

/// FBX stores times as integer ticks of this many per second.
const TICKS_PER_SECOND: f64 = 46_186_158_000.0;

/// One animated scalar channel.
#[derive(Debug, Clone, Default)]
pub struct Curve {
    /// Key times in seconds, ascending.
    pub times: Vec<f64>,
    pub values: Vec<f64>,
}

impl Curve {
    pub fn is_empty(&self) -> bool {
        self.times.is_empty()
    }

    /// Samples the curve, holding the end values outside the keyed range.
    ///
    /// FBX keys carry cubic tangents that this ignores: sampling linearly at
    /// the file's own key times reproduces every key exactly, which is what an
    /// importer that re-keys on import needs.
    pub fn sample(&self, time: f64) -> Option<f64> {
        if self.times.is_empty() {
            return None;
        }
        let last = self.times.len() - 1;
        if time <= self.times[0] {
            return Some(self.values[0]);
        }
        if time >= self.times[last] {
            return Some(self.values[last]);
        }
        let upper = self.times.partition_point(|t| *t < time).min(last);
        let lower = upper.saturating_sub(1);
        let span = self.times[upper] - self.times[lower];
        if span <= f64::EPSILON {
            return Some(self.values[lower]);
        }
        let factor = (time - self.times[lower]) / span;
        Some(self.values[lower] + (self.values[upper] - self.values[lower]) * factor)
    }
}

/// One of a node's transform properties, as three channels plus the static
/// values to use where a channel has no curve.
#[derive(Debug, Clone, Default)]
pub struct TransformChannel {
    pub x: Curve,
    pub y: Curve,
    pub z: Curve,
    /// The curve node's own `d|X`, `d|Y`, `d|Z` values.
    pub default: [f64; 3],
}

impl TransformChannel {
    pub fn is_empty(&self) -> bool {
        self.x.is_empty() && self.y.is_empty() && self.z.is_empty()
    }

    /// The value at a time, falling back per axis to the static default.
    pub fn sample(&self, time: f64) -> [f64; 3] {
        [
            self.x.sample(time).unwrap_or(self.default[0]),
            self.y.sample(time).unwrap_or(self.default[1]),
            self.z.sample(time).unwrap_or(self.default[2]),
        ]
    }

    /// Every key time in the channel, sorted and deduplicated.
    pub fn key_times(&self, into: &mut Vec<f64>) {
        into.extend_from_slice(&self.x.times);
        into.extend_from_slice(&self.y.times);
        into.extend_from_slice(&self.z.times);
    }
}

/// Everything one take animates on one node.
#[derive(Debug, Clone)]
pub struct NodeAnimation {
    /// Object id of the animated `Model`.
    pub node: i64,
    /// The model's name, namespace intact.
    pub name: String,
    pub translation: Option<TransformChannel>,
    /// Euler angles in degrees, in the node's own rotation order.
    pub rotation: Option<TransformChannel>,
    pub scale: Option<TransformChannel>,
}

impl NodeAnimation {
    pub fn key_times(&self, into: &mut Vec<f64>) {
        for channel in [&self.translation, &self.rotation, &self.scale]
            .into_iter()
            .flatten()
        {
            channel.key_times(into);
        }
    }
}

/// One `AnimationStack` — what other tools call a take or a clip.
#[derive(Debug, Clone)]
pub struct Take {
    pub name: String,
    /// Seconds, from the stack's declared local time span when it has one and
    /// from the keys otherwise.
    pub duration: f64,
    pub nodes: Vec<NodeAnimation>,
}

impl Take {
    /// Every distinct key time in the take, sorted.
    pub fn key_times(&self) -> Vec<f64> {
        let mut times = Vec::new();
        for node in &self.nodes {
            node.key_times(&mut times);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        times.dedup_by(|a, b| (*a - *b).abs() <= 1.0e-9);
        if times.is_empty() {
            times.push(0.0);
        }
        times
    }
}

fn ticks_to_seconds(ticks: i64) -> f64 {
    ticks as f64 / TICKS_PER_SECOND
}

impl Scene {
    /// Every animation take in the file.
    ///
    /// Takes with no animated node are dropped: FBX exporters routinely emit an
    /// empty stack alongside the real one.
    pub fn takes(&self) -> Vec<Take> {
        // Curve nodes point at the model they drive, but the link is recorded
        // on the model's side, so the reverse map is built once rather than
        // rediscovered per curve node.
        let mut targets: std::collections::HashMap<i64, (i64, String)> =
            std::collections::HashMap::new();
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

                // Layers are blended by FBX; importers that do not blend take
                // the base layer, which is the first one connected.
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
    fn stack_duration(&self, stack: &crate::Object) -> Option<f64> {
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
        targets: &std::collections::HashMap<i64, (i64, String)>,
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
