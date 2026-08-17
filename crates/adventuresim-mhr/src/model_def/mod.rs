//! Parser for the "Momentum Model Definition V1.0" text format
//! (`compact_v6_1.model`), which maps model parameters onto joint parameters.

use std::collections::HashMap;

use anyhow::{Result, bail};

use crate::character::{PARAMETERS_PER_JOINT, Skeleton};

pub mod parameter_limit;
pub mod parameter_transform;

pub use parameter_limit::ParameterLimit;
pub use parameter_transform::ParameterTransform;

/// `tx, ty, tz, rx, ry, rz, sc` — the seven channels of a momentum joint.
pub const JOINT_PARAMETER_NAMES: [&str; PARAMETERS_PER_JOINT] =
    ["tx", "ty", "tz", "rx", "ry", "rz", "sc"];

fn tokenize<'a>(text: &'a str, delimiters: &str) -> Vec<&'a str> {
    text.split(|c| delimiters.contains(c))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect()
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(pos) => line[..pos].trim(),
        None => line.trim(),
    }
}

fn split_sections(text: &str) -> Result<HashMap<String, Vec<String>>> {
    let mut lines = text.lines();
    let mut saw_header = false;
    for line in lines.by_ref() {
        let line = strip_comment(line);
        if line.is_empty() {
            continue;
        }
        if line == "Momentum Model Definition V1.0" {
            saw_header = true;
            break;
        }
        bail!("invalid model definition file; got {line:?}");
    }
    if !saw_header {
        bail!("invalid model definition file; missing the version header");
    }

    let mut sections: HashMap<String, Vec<String>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in lines {
        let line = strip_comment(line);
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current = Some(line[1..line.len() - 1].to_string());
            sections
                .entry(line[1..line.len() - 1].to_string())
                .or_default();
            continue;
        }
        if let Some(section) = &current {
            sections
                .entry(section.clone())
                .or_default()
                .push(line.to_string());
        }
    }
    Ok(sections)
}

type Triplet = (usize, usize, f32);

fn parse_expression(
    triplets: &mut Vec<Triplet>,
    transform: &mut ParameterTransform,
    skeleton: &Skeleton,
    expression: &str,
    row: usize,
    line: &str,
) -> Result<()> {
    for term in tokenize(expression, "+") {
        let factors = tokenize(term, "*");
        if factors.len() == 1 {
            let Ok(weight) = factors[0].parse::<f32>() else {
                continue;
            };
            transform.offsets[row] = weight;
            continue;
        }
        if factors.len() != 2 {
            continue;
        }

        let Ok(weight) = factors[0].parse::<f32>() else {
            bail!("could not parse weight in: {line}");
        };
        let name = factors[1];

        let parameter = transform.parameter_index(name);
        let reference = match name.split_once('.') {
            Some((joint, channel)) => skeleton.joint_index(joint).and_then(|joint| {
                JOINT_PARAMETER_NAMES
                    .iter()
                    .position(|c| *c == channel)
                    .map(|channel| joint * PARAMETERS_PER_JOINT + channel)
            }),
            None => None,
        };

        match (parameter, reference) {
            (Some(parameter), _) => triplets.push((row, parameter, weight)),
            (None, Some(reference)) => {
                for index in 0..triplets.len() {
                    let (source_row, column, value) = triplets[index];
                    if source_row == reference {
                        triplets.push((row, column, value * weight));
                    }
                }
            }
            (None, None) => {
                let parameter = transform.names.len();
                transform.names.push(name.to_string());
                triplets.push((row, parameter, weight));
            }
        }
    }
    Ok(())
}

fn parse_parameter_transform(
    lines: &[String],
    skeleton: &Skeleton,
) -> Result<(ParameterTransform, Vec<Triplet>)> {
    let rows = skeleton.len() * PARAMETERS_PER_JOINT;
    let mut transform = ParameterTransform {
        offsets: vec![0.0; rows],
        active_joint_parameters: vec![false; rows],
        num_joint_parameters: rows,
        ..Default::default()
    };
    let mut triplets: Vec<Triplet> = Vec::new();

    for line in lines {
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        let Some((joint_name, channel_name)) = left.trim().split_once('.') else {
            bail!("unknown joint name in expression: {line}");
        };
        let Some(joint) = skeleton.joint_index(joint_name.trim()) else {
            bail!("unknown joint name in expression: {line}");
        };
        let Some(channel) = JOINT_PARAMETER_NAMES
            .iter()
            .position(|c| *c == channel_name.trim())
        else {
            bail!("unknown channel name in expression: {line}");
        };

        let row = joint * PARAMETERS_PER_JOINT + channel;
        transform.active_joint_parameters[row] = true;
        parse_expression(&mut triplets, &mut transform, skeleton, right, row, line)?;
    }

    Ok((transform, triplets))
}

fn parse_parameter_sets(lines: &[String], transform: &mut ParameterTransform) {
    for line in lines {
        let tokens = tokenize(line, " \t");
        if tokens.len() < 2 || tokens[0] != "parameterset" {
            continue;
        }
        let mut set = vec![false; transform.num_parameters()];
        for name in &tokens[2..] {
            if let Some(index) = transform.parameter_index(name) {
                set[index] = true;
            }
        }
        transform.parameter_sets.insert(tokens[1].to_string(), set);
    }
}

fn parse_limits(lines: &[String], transform: &mut ParameterTransform) {
    for line in lines {
        let tokens = tokenize(line, " \t");
        if tokens.len() < 4 || tokens[0] != "limit" || tokens[2] != "minmax" {
            continue;
        }
        let Some(parameter) = transform.parameter_index(tokens[1]) else {
            continue;
        };
        let (Some(open), Some(close)) = (line.find('['), line.find(']')) else {
            continue;
        };
        let bounds: Vec<f32> = tokenize(&line[open + 1..close], ",")
            .iter()
            .filter_map(|t| t.parse::<f32>().ok())
            .collect();
        if bounds.len() == 2 {
            transform.limits.push(ParameterLimit {
                parameter,
                min: bounds[0],
                max: bounds[1],
                weight: line[close + 1..].trim().parse().unwrap_or(1.0),
            });
        }
    }
}

/// Parses a `.model` file against a skeleton, producing the parameter transform.
pub fn parse_model_definition(text: &str, skeleton: &Skeleton) -> Result<ParameterTransform> {
    let sections = split_sections(text)?;
    let empty = Vec::new();
    let (mut transform, triplets) = parse_parameter_transform(
        sections.get("ParameterTransform").unwrap_or(&empty),
        skeleton,
    )?;

    let columns = transform.num_parameters();
    transform.transform = vec![0.0; transform.num_joint_parameters * columns];
    for (row, column, value) in triplets {
        if value != 0.0 {
            transform.transform[row * columns + column] += value;
        }
    }

    if let Some(lines) = sections.get("ParameterSets") {
        parse_parameter_sets(lines, &mut transform);
    }
    if let Some(lines) = sections.get("Limits") {
        parse_limits(lines, &mut transform);
    }

    Ok(transform)
}

/// Appends one column per blend-shape coefficient. The new columns drive no joint.
pub fn append_blend_shape_parameters(transform: &mut ParameterTransform, count: usize) {
    let old_columns = transform.num_parameters();
    let new_columns = old_columns + count;
    let mut dense = vec![0.0; transform.num_joint_parameters * new_columns];
    for row in 0..transform.num_joint_parameters {
        let source = &transform.transform[row * old_columns..(row + 1) * old_columns];
        dense[row * new_columns..row * new_columns + old_columns].copy_from_slice(source);
    }
    transform.transform = dense;
    for index in 0..count {
        transform.names.push(format!("blend_{index}"));
    }
    for set in transform.parameter_sets.values_mut() {
        set.resize(new_columns, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skeleton() -> Skeleton {
        Skeleton {
            names: vec!["root".into(), "child".into()],
            parents: vec![-1, 0],
            translation_offsets: vec![[0.0; 3]; 2],
            prerotations: vec![[0.0, 0.0, 0.0, 1.0]; 2],
        }
    }

    fn parse(body: &str) -> ParameterTransform {
        let text = format!("Momentum Model Definition V1.0\n[ParameterTransform]\n{body}");
        parse_model_definition(&text, &skeleton()).unwrap()
    }

    #[test]
    fn parses_weights_and_parameter_order() {
        let pt = parse("root.tx = 10.0 * root_tx\nchild.rz = 1.0 * bend + 0.5 * lean\n");
        assert_eq!(pt.names, ["root_tx", "bend", "lean"]);
        assert_eq!(pt.row(0), [10.0, 0.0, 0.0]);
        assert_eq!(pt.row(12), [0.0, 1.0, 0.5]);
        assert!(pt.active_joint_parameters[0]);
        assert!(!pt.active_joint_parameters[1]);
    }

    #[test]
    fn a_joint_reference_copies_scaled_terms() {
        let pt = parse("root.rx = 1.0 * twist + 0.25 * lean\nchild.rx = -0.5 * root.rx\n");
        assert_eq!(pt.names, ["twist", "lean"]);
        assert_eq!(pt.row(3), [1.0, 0.25]);
        assert_eq!(pt.row(10), [-0.5, -0.125]);
    }

    #[test]
    fn duplicate_assignments_accumulate() {
        let pt = parse("root.tx = 1.0 * a\nroot.tx = 2.0 * a\n");
        assert_eq!(pt.row(0), [3.0]);
    }

    #[test]
    fn a_bare_constant_becomes_an_offset() {
        let pt = parse("root.ty = 1.0 * a + 0.75\n");
        assert_eq!(pt.offsets[1], 0.75);
    }

    #[test]
    fn parses_sets_and_limits() {
        let text = "Momentum Model Definition V1.0\n\
             [ParameterTransform]\n\
             root.tx = 1.0 * a\n\
             root.ty = 1.0 * b\n\
             [ParameterSets]\n\
             parameterset rigid a\n\
             [Limits]\n\
             limit b minmax [-0.5, 1.5]\n\
             limit a minmax [-0.25, 0.25] 0.1\n";
        let pt = parse_model_definition(text, &skeleton()).unwrap();
        assert_eq!(pt.parameter_sets["rigid"], [true, false]);
        assert_eq!(pt.limits.len(), 2);
        assert_eq!(pt.limits[0].weight, 1.0);
        assert_eq!(pt.limits[1].weight, 0.1);

        let mut parameters = [0.0, 3.0];
        pt.apply_limits(&mut parameters);
        assert_eq!(parameters, [0.0, 1.5]);
    }

    #[test]
    fn blend_shape_columns_are_appended_without_joint_influence() {
        let mut pt = parse("root.tx = 10.0 * root_tx\n");
        append_blend_shape_parameters(&mut pt, 2);
        assert_eq!(pt.names, ["root_tx", "blend_0", "blend_1"]);
        assert_eq!(pt.row(0), [10.0, 0.0, 0.0]);
    }
}
