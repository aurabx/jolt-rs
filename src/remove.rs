use serde_json::{Map, Value};
use crate::spec::Spec;

/// Applies the `remove` operation.
///
/// Follows Java JOLT's Removr. Spec keys match input keys literally, except that `*` matches any
/// run of characters (`*` alone matches every key). On arrays, keys match element indexes, so
/// `"0"` removes the first element and `"*"` walks every element. An object in the spec is walked
/// into; any other spec value removes the matched entries.
pub(crate) fn remove(mut input: Value, spec: &Spec) -> Value {
    if let Value::Object(spec) = spec.value() {
        remove_matching(&mut input, spec);
    }
    input
}

fn remove_matching(target: &mut Value, spec: &Map<String, Value>) {
    match target {
        Value::Object(map) => remove_from_object(map, spec),
        Value::Array(arr) => remove_from_array(arr, spec),
        _ => {}
    }
}

fn remove_from_object(map: &mut Map<String, Value>, spec: &Map<String, Value>) {
    for (pattern, spec_value) in spec {
        let keys: Vec<String> = map
            .keys()
            .filter(|key| key_matches(pattern, key))
            .cloned()
            .collect();
        for key in keys {
            match spec_value {
                Value::Object(child_spec) => {
                    if let Some(child) = map.get_mut(&key) {
                        remove_matching(child, child_spec);
                    }
                }
                _ => {
                    map.remove(&key);
                }
            }
        }
    }
}

fn remove_from_array(arr: &mut Vec<Value>, spec: &Map<String, Value>) {
    // Indexes are collected first and removed together, so that removing one element does not
    // shift the positions that later spec keys refer to.
    let mut doomed: Vec<usize> = Vec::new();
    for (pattern, spec_value) in spec {
        for index in 0..arr.len() {
            if !key_matches(pattern, &index.to_string()) {
                continue;
            }
            match spec_value {
                Value::Object(child_spec) => remove_matching(&mut arr[index], child_spec),
                _ => doomed.push(index),
            }
        }
    }
    doomed.sort_unstable();
    doomed.dedup();
    for index in doomed.into_iter().rev() {
        arr.remove(index);
    }
}

/// Matches a key against a pattern where `*` stands for any run of characters.
fn key_matches(pattern: &str, key: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == key;
    }

    let mut segments = pattern.split('*');
    // `split` always yields at least one item, and a pattern containing `*` yields at least two.
    let first = segments.next().unwrap_or("");
    let Some(mut rest) = key.strip_prefix(first) else {
        return false;
    };

    let mut segments: Vec<&str> = segments.collect();
    let last = segments.pop().unwrap_or("");
    for segment in segments {
        match rest.find(segment) {
            Some(pos) => rest = &rest[pos + segment.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

#[cfg(test)]
mod test {

    use serde_json::json;
    use super::*;

    fn run(spec: Value, input: Value) -> Value {
        let spec: Spec = serde_json::from_value(spec).expect("parsed spec");
        remove(input, &spec)
    }

    #[test]
    fn test_remove_if_absent() {
        //given
        let spec: Spec = serde_json::from_value(json!({
            "a" : "a",
            "d" : {
                "e" : "e"
            }
        }))
        .expect("parsed spec");

        let input: Value = serde_json::from_value(json!({
            "b" : "b",
            "c" : "c"
        }))
        .expect("parsed spec");

        //when
        let output = remove(input, &spec);

        //then
        assert_eq!(
            output,
            json!({
                "b" : "b",
                "c" : "c"
            })
        )
    }

    #[test]
    fn test_remove_if_present() {
        //given
        let spec: Spec = serde_json::from_value(json!({
            "a" : ""
        }))
        .expect("parsed spec");

        let input: Value = serde_json::from_value(json!({
            "a" : "a",
            "b" : "b"
        }))
        .expect("parsed spec");

        //when
        let output = remove(input, &spec);

        //then
        assert_eq!(
            output,
            json!({
                 "b" : "b"
            })
        )
    }

    #[test]
    fn test_remove_last_key_leaves_empty_object() {
        let output = run(json!({ "a": "" }), json!({ "a": "b" }));

        assert_eq!(output, json!({}));
    }

    #[test]
    fn test_empty_spec_changes_nothing() {
        let output = run(json!({}), json!({ "a": "b" }));

        assert_eq!(output, json!({ "a": "b" }));
    }

    #[test]
    fn test_star_matches_every_object_key() {
        let output = run(
            json!({ "*": { "secret": "" } }),
            json!({ "a": { "secret": 1, "k": 2 }, "b": { "secret": 3 }, "c": "scalar" }),
        );

        assert_eq!(output, json!({ "a": { "k": 2 }, "b": {}, "c": "scalar" }));
    }

    #[test]
    fn test_star_leaf_removes_every_key() {
        let output = run(json!({ "data": { "*": "" } }), json!({ "data": { "a": 1, "b": 2 }, "x": 1 }));

        assert_eq!(output, json!({ "data": {}, "x": 1 }));
    }

    #[test]
    fn test_partial_star_matches_prefix_and_suffix() {
        let output = run(
            json!({ "tmp_*": "", "*_id": "" }),
            json!({ "tmp_a": 1, "tmp_b": 2, "user_id": 3, "name": 4, "tmp": 5 }),
        );

        assert_eq!(output, json!({ "name": 4, "tmp": 5 }));
    }

    #[test]
    fn test_star_walks_array_elements() {
        let output = run(
            json!({ "items": { "*": { "secret": "" } } }),
            json!({ "items": [{ "secret": 1, "k": 2 }, { "secret": 3 }] }),
        );

        assert_eq!(output, json!({ "items": [{ "k": 2 }, {}] }));
    }

    #[test]
    fn test_array_indexes_are_removed_without_shifting() {
        let output = run(json!({ "items": { "0": "", "2": "" } }), json!({ "items": ["a", "b", "c", "d"] }));

        assert_eq!(output, json!({ "items": ["b", "d"] }));
    }

    #[test]
    fn test_star_leaf_empties_array() {
        let output = run(json!({ "items": { "*": "" } }), json!({ "items": [1, 2, 3] }));

        assert_eq!(output, json!({ "items": [] }));
    }

    #[test]
    fn test_key_with_slash_and_tilde_is_literal() {
        let output = run(json!({ "a/b": "", "c~d": "" }), json!({ "a/b": 1, "c~d": 2, "e": 3 }));

        assert_eq!(output, json!({ "e": 3 }));
    }

    #[test]
    fn test_key_matches() {
        assert!(key_matches("*", ""));
        assert!(key_matches("*", "anything"));
        assert!(key_matches("a*", "a"));
        assert!(key_matches("a*c", "abc"));
        assert!(key_matches("a*b*c", "a-b-c"));
        assert!(!key_matches("a*c", "ab"));
        // the prefix and suffix must not overlap
        assert!(!key_matches("ab*ba", "aba"));
        assert!(!key_matches("a", "ab"));
    }
}
