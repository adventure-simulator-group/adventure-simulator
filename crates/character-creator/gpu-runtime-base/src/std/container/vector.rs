use crate::Value;

#[derive(Debug, Clone, Copy, Default)]
pub struct Vector;

impl Vector {
    pub fn new() -> Vec<Value> {
        Vec::new()
    }

    pub fn push(mut vector: Vec<Value>, value: Value) -> Vec<Value> {
        vector.push(value);
        vector
    }

    pub fn pop(mut vector: Vec<Value>) -> Vec<Value> {
        vector.pop();
        vector
    }

    pub fn remove(mut vector: Vec<Value>, index: usize) -> Vec<Value> {
        if index < vector.len() {
            vector.remove(index);
        }
        vector
    }

    pub fn insert(mut vector: Vec<Value>, index: usize, value: Value) -> Vec<Value> {
        if index <= vector.len() {
            vector.insert(index, value);
        }
        vector
    }

    pub fn replace(mut vector: Vec<Value>, index: usize, value: Value) -> Vec<Value> {
        if index < vector.len() {
            vector[index] = value;
        }
        vector
    }

    pub fn get(vector: Vec<Value>, index: usize) -> Option<Value> {
        vector.get(index).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Context, Process};

    #[test]
    fn test_create_vector() {
        let node = graph::Node::new("Test");
        let mut globals = crate::RuntimeGlobals::default();
        let process = VectorProcess;
        let result = process
            .execute(
                Context {
                    node: &node,
                    globals: &mut globals,
                    state: &mut None,
                    logs: &mut Vec::new(),
                },
                vec![],
            )
            .unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Some(Value::Vector(v)) => assert!(v.is_empty()),
            _ => panic!("Expected Vector"),
        }
    }

    #[test]
    fn test_vector_push() {
        let process = VectorPushProcess;
        let inputs = vec![Some(Value::Vector(vec![])), Some(Value::Number(42.0))];
        let node = graph::Node::new("Test");
        let mut globals = crate::RuntimeGlobals::default();
        let result = process
            .execute(
                Context {
                    node: &node,
                    globals: &mut globals,
                    state: &mut None,
                    logs: &mut Vec::new(),
                },
                inputs,
            )
            .unwrap();
        match &result[0] {
            Some(Value::Vector(v)) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0], Value::Number(42.0));
            }
            _ => panic!("Expected Vector"),
        }
    }

    #[test]
    fn test_vector_pop() {
        let process = VectorPopProcess;
        let inputs = vec![Some(Value::Vector(vec![Value::Number(42.0)]))];
        let node = graph::Node::new("Test");
        let mut globals = crate::RuntimeGlobals::default();
        let result = process
            .execute(
                Context {
                    node: &node,
                    globals: &mut globals,
                    state: &mut None,
                    logs: &mut Vec::new(),
                },
                inputs,
            )
            .unwrap();
        match &result[0] {
            Some(Value::Vector(v)) => assert!(v.is_empty()),
            _ => panic!("Expected Vector"),
        }
    }

    #[test]
    fn test_vector_remove() {
        let process = VectorRemoveProcess;
        let inputs = vec![
            Some(Value::Vector(vec![Value::Number(42.0)])),
            Some(Value::Number(0.0)),
        ];
        let node = graph::Node::new("Test");
        let mut globals = crate::RuntimeGlobals::default();
        let result = process
            .execute(
                Context {
                    node: &node,
                    globals: &mut globals,
                    state: &mut None,
                    logs: &mut Vec::new(),
                },
                inputs,
            )
            .unwrap();
        match &result[0] {
            Some(Value::Vector(v)) => assert!(v.is_empty()),
            _ => panic!("Expected Vector"),
        }
    }

    #[test]
    fn test_vector_insert() {
        let process = VectorInsertProcess;
        let inputs = vec![
            Some(Value::Vector(vec![])),
            Some(Value::Number(0.0)),
            Some(Value::Number(42.0)),
        ];
        let node = graph::Node::new("Test");
        let mut globals = crate::RuntimeGlobals::default();
        let result = process
            .execute(
                Context {
                    node: &node,
                    globals: &mut globals,
                    state: &mut None,
                    logs: &mut Vec::new(),
                },
                inputs,
            )
            .unwrap();
        match &result[0] {
            Some(Value::Vector(v)) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0], Value::Number(42.0));
            }
            _ => panic!("Expected Vector"),
        }
    }

    #[test]
    fn test_vector_replace() {
        let process = VectorReplaceProcess;
        let inputs = vec![
            Some(Value::Vector(vec![Value::Number(0.0)])),
            Some(Value::Number(0.0)),
            Some(Value::Number(42.0)),
        ];
        let node = graph::Node::new("Test");
        let mut globals = crate::RuntimeGlobals::default();
        let result = process
            .execute(
                Context {
                    node: &node,
                    globals: &mut globals,
                    state: &mut None,
                    logs: &mut Vec::new(),
                },
                inputs,
            )
            .unwrap();
        match &result[0] {
            Some(Value::Vector(v)) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0], Value::Number(42.0));
            }
            _ => panic!("Expected Vector"),
        }
    }

    #[test]
    fn test_vector_get() {
        let process = VectorGetProcess;
        let inputs = vec![
            Some(Value::Vector(vec![Value::Number(42.0)])),
            Some(Value::Number(0.0)),
        ];
        let node = graph::Node::new("Test");
        let mut globals = crate::RuntimeGlobals::default();
        let result = process
            .execute(
                Context {
                    node: &node,
                    globals: &mut globals,
                    state: &mut None,
                    logs: &mut Vec::new(),
                },
                inputs,
            )
            .unwrap();
        match &result[0] {
            Some(Value::Number(val)) => assert_eq!(*val, 42.0),
            _ => panic!("Expected Number 42.0"),
        }
    }
}
