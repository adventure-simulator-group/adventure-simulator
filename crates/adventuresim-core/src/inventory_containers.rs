//! Framework-independent rules for physical inventory containment.
//!
//! This graph is deliberately unrelated to equipment attachment topology.

use std::collections::{BTreeMap, BTreeSet};

use crate::physical_object::PhysicalObjectId;

pub const MAX_CONTAINER_DEPTH: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    pub id: PhysicalObjectId,
    /// Exterior displacement when this object is an immediate child.
    pub exterior_volume_ml: u64,
    /// Interior capacity; zero means this object is not a container.
    pub capacity_ml: u64,
    /// Measured liquid/material volume currently represented by this object.
    pub measured_volume_ml: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContainmentGraph {
    objects: BTreeMap<PhysicalObjectId, Object>,
    parent_by_child: BTreeMap<PhysicalObjectId, PhysicalObjectId>,
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

    pub fn parent(&self, child: PhysicalObjectId) -> Option<PhysicalObjectId> {
        self.parent_by_child.get(&child).copied()
    }

    pub fn is_nonempty(&self, container: PhysicalObjectId) -> bool {
        self.parent_by_child
            .values()
            .any(|parent| *parent == container)
    }

    pub fn used_ml(&self, container: PhysicalObjectId) -> Result<u64, &'static str> {
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

    pub fn insert(
        &mut self,
        child: PhysicalObjectId,
        parent: PhysicalObjectId,
    ) -> Result<(), &'static str> {
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

    pub fn remove(&mut self, child: PhysicalObjectId) -> bool {
        self.parent_by_child.remove(&child).is_some()
    }

    pub fn subtree(&self, root: PhysicalObjectId) -> Result<Vec<PhysicalObjectId>, &'static str> {
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
            id: PhysicalObjectId::try_new(id).unwrap(),
            exterior_volume_ml: exterior,
            capacity_ml: capacity,
            measured_volume_ml: 0,
        }
    }

    #[test]
    fn exact_fit_succeeds_and_one_ml_over_fails() {
        let mut exact =
            ContainmentGraph::new([object(1, 100, 1_000), object(2, 1_000, 0)]).unwrap();
        assert_eq!(exact.insert(object_id(2), object_id(1)), Ok(()));
        let mut over = ContainmentGraph::new([object(1, 100, 999), object(2, 1_000, 0)]).unwrap();
        assert_eq!(
            over.insert(object_id(2), object_id(1)),
            Err("Container capacity exceeded")
        );
    }

    #[test]
    fn liquid_uses_current_volume_and_grandchildren_are_not_double_counted() {
        let mut graph = ContainmentGraph::new([
            object(1, 500, 1_000),
            object(2, 900, 800),
            Object {
                id: PhysicalObjectId::try_new(3).unwrap(),
                exterior_volume_ml: 1_000,
                capacity_ml: 0,
                measured_volume_ml: 700,
            },
        ])
        .unwrap();
        graph.insert(object_id(3), object_id(2)).unwrap();
        graph.insert(object_id(2), object_id(1)).unwrap();
        assert_eq!(graph.used_ml(object_id(2)), Ok(700));
        assert_eq!(graph.used_ml(object_id(1)), Ok(900));
    }

    #[test]
    fn rejects_self_cycles_and_excessive_ancestry() {
        let mut graph = ContainmentGraph::new((1..=18).map(|id| object(id, 1, 2))).unwrap();
        assert!(graph.insert(object_id(1), object_id(1)).is_err());
        for id in 2..=17 {
            graph.insert(object_id(id - 1), object_id(id)).unwrap();
        }
        assert!(graph.insert(object_id(17), object_id(1)).is_err());
        assert!(graph.insert(object_id(17), object_id(18)).is_err());
    }

    #[test]
    fn subtree_is_atomic_and_deterministic() {
        let mut graph = ContainmentGraph::new((1..=4).map(|id| object(id, 1, 10))).unwrap();
        graph.insert(object_id(2), object_id(1)).unwrap();
        graph.insert(object_id(3), object_id(1)).unwrap();
        graph.insert(object_id(4), object_id(2)).unwrap();
        assert_eq!(
            graph.subtree(object_id(1)).unwrap(),
            vec![object_id(1), object_id(2), object_id(3), object_id(4)]
        );
        assert!(graph.is_nonempty(object_id(1)));
    }

    fn object_id(id: u64) -> PhysicalObjectId {
        PhysicalObjectId::try_new(id).unwrap()
    }

    #[test]
    fn duplicate_physical_object_ids_are_rejected() {
        assert_eq!(
            ContainmentGraph::new([object(1, 100, 100), object(1, 100, 100)]),
            Err("Duplicate inventory object identity")
        );
    }
}
