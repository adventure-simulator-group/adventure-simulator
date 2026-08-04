//! Framework-independent rules for physical inventory containment.
//!
//! This graph is deliberately unrelated to equipment attachment topology.

use std::collections::{BTreeMap, BTreeSet};

pub const MAX_CONTAINER_DEPTH: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    pub id: u64,
    /// Exterior displacement when this object is an immediate child.
    pub exterior_volume_ml: u64,
    /// Interior capacity; zero means this object is not a container.
    pub capacity_ml: u64,
    /// Measured liquid/material volume currently represented by this object.
    pub measured_volume_ml: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContainmentGraph {
    objects: BTreeMap<u64, Object>,
    parent_by_child: BTreeMap<u64, u64>,
}

impl ContainmentGraph {
    pub fn new(objects: impl IntoIterator<Item = Object>) -> Result<Self, &'static str> {
        let mut graph = Self::default();
        for object in objects {
            if object.exterior_volume_ml == 0 {
                return Err("Every physical object must have a positive exterior volume");
            }
            if object.measured_volume_ml > object.exterior_volume_ml {
                return Err("Measured volume exceeds the object's authored exterior volume");
            }
            if graph.objects.insert(object.id, object).is_some() {
                return Err("Duplicate inventory object identity");
            }
        }
        Ok(graph)
    }

    pub fn parent(&self, child: u64) -> Option<u64> {
        self.parent_by_child.get(&child).copied()
    }

    pub fn is_nonempty(&self, container: u64) -> bool {
        self.parent_by_child
            .values()
            .any(|parent| *parent == container)
    }

    pub fn used_ml(&self, container: u64) -> Result<u64, &'static str> {
        let mut used = 0u64;
        for child in self
            .parent_by_child
            .iter()
            .filter_map(|(child, parent)| (*parent == container).then_some(*child))
        {
            let object = self.objects.get(&child).ok_or("Unknown contained object")?;
            // An immediate liquid lot consumes its current amount. A vessel or
            // solid consumes its exterior displacement; grandchildren are
            // already physically inside that displacement and are not added.
            let volume = if object.measured_volume_ml > 0 {
                object.measured_volume_ml
            } else {
                object.exterior_volume_ml
            };
            used = used
                .checked_add(volume)
                .ok_or("Container volume overflow")?;
        }
        Ok(used)
    }

    pub fn insert(&mut self, child: u64, parent: u64) -> Result<(), &'static str> {
        if child == parent {
            return Err("A container cannot contain itself");
        }
        let parent_object = self
            .objects
            .get(&parent)
            .ok_or("Unknown destination container")?;
        if parent_object.capacity_ml == 0 {
            return Err("Destination is not a container");
        }
        let child_object = self.objects.get(&child).ok_or("Unknown child object")?;
        let child_volume = if child_object.measured_volume_ml > 0 {
            child_object.measured_volume_ml
        } else {
            child_object.exterior_volume_ml
        };

        let mut ancestor = Some(parent);
        let mut visited = BTreeSet::new();
        let mut ancestor_depth = 0usize;
        for _ in 0..=MAX_CONTAINER_DEPTH {
            let Some(id) = ancestor else { break };
            if id == child {
                return Err("Container nesting would create a cycle");
            }
            if !visited.insert(id) {
                return Err("Existing containment cycle");
            }
            ancestor = self.parent(id);
            ancestor_depth += 1;
        }
        if ancestor.is_some() {
            return Err("Container nesting exceeds the maximum depth");
        }
        let mut frontier = vec![(child, 0usize)];
        let mut descendant_depth = 0usize;
        while let Some((node, depth)) = frontier.pop() {
            descendant_depth = descendant_depth.max(depth);
            if ancestor_depth
                .checked_add(descendant_depth)
                .is_none_or(|combined| combined > MAX_CONTAINER_DEPTH)
            {
                return Err("Container nesting exceeds the maximum depth");
            }
            frontier.extend(self.parent_by_child.iter().filter_map(
                |(candidate, candidate_parent)| {
                    (*candidate_parent == node).then_some((*candidate, depth + 1))
                },
            ));
        }

        let old_parent = self.parent_by_child.remove(&child);
        let used = self.used_ml(parent)?;
        let fits = used
            .checked_add(child_volume)
            .is_some_and(|next| next <= parent_object.capacity_ml);
        if !fits {
            if let Some(old_parent) = old_parent {
                self.parent_by_child.insert(child, old_parent);
            }
            return Err("Container capacity exceeded");
        }
        self.parent_by_child.insert(child, parent);
        Ok(())
    }

    pub fn remove(&mut self, child: u64) -> bool {
        self.parent_by_child.remove(&child).is_some()
    }

    pub fn subtree(&self, root: u64) -> Result<Vec<u64>, &'static str> {
        if !self.objects.contains_key(&root) {
            return Err("Unknown inventory object");
        }
        let mut result = vec![root];
        let mut cursor = 0;
        while cursor < result.len() {
            if cursor >= MAX_CONTAINER_DEPTH * self.objects.len().max(1) {
                return Err("Containment traversal bound exceeded");
            }
            let parent = result[cursor];
            for child in self
                .parent_by_child
                .iter()
                .filter_map(|(child, candidate)| (*candidate == parent).then_some(*child))
            {
                if result.contains(&child) {
                    return Err("Containment cycle");
                }
                result.push(child);
            }
            cursor += 1;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(id: u64, exterior: u64, capacity: u64) -> Object {
        Object {
            id,
            exterior_volume_ml: exterior,
            capacity_ml: capacity,
            measured_volume_ml: 0,
        }
    }

    #[test]
    fn exact_fit_succeeds_and_one_ml_over_fails() {
        let mut exact =
            ContainmentGraph::new([object(1, 100, 1_000), object(2, 1_000, 0)]).unwrap();
        assert_eq!(exact.insert(2, 1), Ok(()));
        let mut over = ContainmentGraph::new([object(1, 100, 999), object(2, 1_000, 0)]).unwrap();
        assert_eq!(over.insert(2, 1), Err("Container capacity exceeded"));
    }

    #[test]
    fn liquid_uses_current_volume_and_grandchildren_are_not_double_counted() {
        let mut graph = ContainmentGraph::new([
            object(1, 500, 1_000),
            object(2, 900, 800),
            Object {
                id: 3,
                exterior_volume_ml: 1_000,
                capacity_ml: 0,
                measured_volume_ml: 700,
            },
        ])
        .unwrap();
        graph.insert(3, 2).unwrap();
        graph.insert(2, 1).unwrap();
        assert_eq!(graph.used_ml(2), Ok(700));
        assert_eq!(graph.used_ml(1), Ok(900));
    }

    #[test]
    fn rejects_self_cycles_and_excessive_ancestry() {
        let mut graph = ContainmentGraph::new((1..=18).map(|id| object(id, 1, 2))).unwrap();
        assert!(graph.insert(1, 1).is_err());
        for id in 2..=17 {
            graph.insert(id - 1, id).unwrap();
        }
        assert!(graph.insert(17, 1).is_err());
        assert!(graph.insert(17, 18).is_err());
    }

    #[test]
    fn subtree_is_atomic_and_deterministic() {
        let mut graph = ContainmentGraph::new((1..=4).map(|id| object(id, 1, 10))).unwrap();
        graph.insert(2, 1).unwrap();
        graph.insert(3, 1).unwrap();
        graph.insert(4, 2).unwrap();
        assert_eq!(graph.subtree(1).unwrap(), vec![1, 2, 3, 4]);
        assert!(graph.is_nonempty(1));
    }
}
