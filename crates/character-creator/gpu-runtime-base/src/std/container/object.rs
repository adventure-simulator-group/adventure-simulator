use crate::Value;
use indexmap::IndexMap;

#[derive(Debug, Clone, Copy, Default)]
pub struct Object;

impl Object {
    pub fn new() -> IndexMap<String, Value> {
        IndexMap::new()
    }

    pub fn insert(
        mut object: IndexMap<String, Value>,
        key: String,
        value: Value,
    ) -> IndexMap<String, Value> {
        object.insert(key, value);
        object
    }

    pub fn remove(mut object: IndexMap<String, Value>, key: String) -> IndexMap<String, Value> {
        object.shift_remove(&key);
        object
    }

    pub fn get(object: IndexMap<String, Value>, key: String) -> Option<Value> {
        object.get(&key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Context, Process};

    #[test]
    fn test_create_object() {
        let node = graph::Node::new("Test");
        let mut globals = crate::RuntimeGlobals::default();
        let process = ObjectProcess;
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
            Some(Value::Object(map)) => assert!(map.is_empty()),
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_object_insert() {
        let process = ObjectInsertProcess;
        let inputs = vec![
            Some(Value::Object(IndexMap::new())),
            Some(Value::String("key".to_string())),
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
            Some(Value::Object(map)) => {
                assert_eq!(map.len(), 1);
                assert_eq!(map.get("key"), Some(&Value::Number(42.0)));
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_object_remove() {
        let process = ObjectRemoveProcess;
        let mut map = IndexMap::new();
        map.insert("key".to_string(), Value::Number(42.0));
        let inputs = vec![
            Some(Value::Object(map)),
            Some(Value::String("key".to_string())),
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
            Some(Value::Object(map)) => assert!(map.is_empty()),
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_object_get() {
        let process = ObjectGetProcess;
        let mut map = IndexMap::new();
        map.insert("key".to_string(), Value::Number(42.0));
        let inputs = vec![
            Some(Value::Object(map)),
            Some(Value::String("key".to_string())),
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
