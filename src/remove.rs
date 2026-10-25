use std::borrow::Cow;

use serde_json::{Map, Value};
use crate::spec::Spec;

/// Applies the `remove` operation.
///
/// Spec keys match input keys literally, except that `*` matches any run of characters (`*` alone
/// matches every key) and `\*` matches a literal `*`. On arrays, keys match element indexes, so
/// `"0"` removes the first element and `"*"` walks every element. An object in the spec is walked
/// into, and so is an array, whose elements stand for indexes `0`, `1` and so on. Any other spec
/// value removes the matched entries.
///
/// Like Java JOLT's Removr, but leaf values other than `""` are accepted rather than rejected.
pub(crate) fn remove(mut input: Value, spec: &Spec) -> Value {
    if let Some(spec) = child_spec(spec.value()) {
        remove_matching(&mut input, &spec);
    }
    input
}

/// Returns the nested spec to walk into, or `None` when the spec value is a leaf that removes.
fn child_spec(spec_value: &Value) -> Option<Cow<'_, Map<String, Value>>> {
    match spec_value {
        Value::Object(map) => Some(Cow::Borrowed(map)),
        Value::Array(items) => Some(Cow::Owned(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| (index.to_string(), item.clone()))
                .collect(),
        )),
        _ => None,
    }
}

fn remove_matching(target: &mut Value, spec: &Map<String, Value>) {
    match target {
        Value::Object(map) => remove_from_object(map, spec),
        Value::Array(arr) => remove_from_array(arr, spec),
        _ => {}
    }
}

fn remove_from_object(map: &mut Map<String, Value>, spec: &Map<String, Value>) {
    for (raw_pattern, spec_value) in spec {
        let pattern = KeyPattern::parse(raw_pattern);
        let keys: Vec<String> = map
            .keys()
            .filter(|key| pattern.matches(key))
            .cloned()
            .collect();
        let child = child_spec(spec_value);
        for key in keys {
            match &child {
                Some(child) => {
                    if let Some(value) = map.get_mut(&key) {
                        remove_matching(value, child);
                    }
                }
                None => {
                    map.remove(&key);
                }
            }
        }
    }
}

fn remove_from_array(arr: &mut Vec<Value>, spec: &Map<String, Value>) {
    // Elements are marked first and dropped in one pass. That keeps the positions later spec keys
    // refer to stable, and avoids the quadratic cost of removing elements one at a time.
    let mut doomed = vec![false; arr.len()];
    for (raw_pattern, spec_value) in spec {
        let pattern = KeyPattern::parse(raw_pattern);
        let child = child_spec(spec_value);
        for index in pattern.matching_indexes(arr.len()) {
            match &child {
                Some(child) => remove_matching(&mut arr[index], child),
                None => doomed[index] = true,
            }
        }
    }

    let mut doomed = doomed.into_iter();
    arr.retain(|_| !doomed.next().unwrap_or(false));
}

/// A remove spec key: either a literal, or a glob split on unescaped `*`.
enum KeyPattern {
    Literal(String),
    Glob(Vec<String>),
}

impl KeyPattern {
    fn parse(raw: &str) -> Self {
        let mut segments = vec![String::new()];
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '*' => segments.push(String::new()),
                '\\' if matches!(chars.peek(), Some('*') | Some('\\')) => {
                    if let (Some(escaped), Some(segment)) = (chars.next(), segments.last_mut()) {
                        segment.push(escaped);
                    }
                }
                other => {
                    if let Some(segment) = segments.last_mut() {
                        segment.push(other);
                    }
                }
            }
        }

        if segments.len() == 1 {
            KeyPattern::Literal(segments.remove(0))
        } else {
            KeyPattern::Glob(segments)
        }
    }

    fn matches(&self, key: &str) -> bool {
        match self {
            KeyPattern::Literal(literal) => literal == key,
            KeyPattern::Glob(segments) => glob_matches(segments, key),
        }
    }

    /// Lists the array indexes below `len` that this pattern selects.
    fn matching_indexes(&self, len: usize) -> Vec<usize> {
        match self {
            // Parsed as a number, as Java does, so "01" selects index 1.
            KeyPattern::Literal(literal) => literal
                .parse::<usize>()
                .ok()
                .filter(|index| *index < len)
                .into_iter()
                .collect(),
            KeyPattern::Glob(segments) if segments.iter().all(|s| s.is_empty()) => (0..len).collect(),
            KeyPattern::Glob(segments) => (0..len)
                .filter(|index| glob_matches(segments, &index.to_string()))
                .collect(),
        }
    }
}

/// Matches a key against glob segments. The segments are the text between the `*`s, so there are
/// always at least two.
fn glob_matches(segments: &[String], key: &str) -> bool {
    let Some((first, rest_segments)) = segments.split_first() else {
        return false;
    };
    let Some((last, middle)) = rest_segments.split_last() else {
        return false;
    };
    let Some(mut rest) = key.strip_prefix(first.as_str()) else {
        return false;
    };

    for segment in middle {
        match rest.find(segment.as_str()) {
            Some(pos) => rest = &rest[pos + segment.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last.as_str())
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

    fn key_matches(pattern: &str, key: &str) -> bool {
        KeyPattern::parse(pattern).matches(key)
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
        assert!(key_matches("é*ü", "éxü"));
    }

    #[test]
    fn test_escaped_star_is_literal() {
        assert!(key_matches("a\\*", "a*"));
        assert!(!key_matches("a\\*", "ab"));
        assert!(key_matches("a\\**", "a*b"));
        assert!(key_matches("a\\\\b", "a\\b"));
        // a backslash before any other character is kept as written
        assert!(key_matches("a\\b", "a\\b"));
    }

    #[test]
    fn test_escaped_star_removes_only_literal_key() {
        let output = run(json!({ "a\\*": "" }), json!({ "a*": 1, "ab": 2 }));

        assert_eq!(output, json!({ "ab": 2 }));
    }

    #[test]
    fn test_array_in_spec_walks_indexes() {
        let output = run(
            json!({ "patient": [{ "ssn": "" }] }),
            json!({ "patient": [{ "ssn": 1, "id": "a" }, { "ssn": 2, "id": "b" }] }),
        );

        assert_eq!(output, json!({ "patient": [{ "id": "a" }, { "ssn": 2, "id": "b" }] }));
    }

    #[test]
    fn test_array_index_key_is_parsed_as_number() {
        let output = run(json!({ "items": { "01": "" } }), json!({ "items": ["a", "b", "c"] }));

        assert_eq!(output, json!({ "items": ["a", "c"] }));
    }

    #[test]
    fn test_partial_glob_on_array_indexes() {
        let input: Vec<u32> = (0..25).collect();
        let output = run(json!({ "items": { "*0": "", "1*": "" } }), json!({ "items": input }));

        let expected: Vec<u32> = (0..25).filter(|i| i % 10 != 0 && !i.to_string().starts_with('1')).collect();
        assert_eq!(output, json!({ "items": expected }));
    }

    #[test]
    fn test_partial_glob_on_large_array_is_linear() {
        let input: Vec<u32> = (0..200_000).collect();
        let started = std::time::Instant::now();
        let output = run(json!({ "items": { "*0": "" } }), json!({ "items": input }));

        assert_eq!(output["items"].as_array().map(Vec::len), Some(180_000));
        // generous bound: the old one-at-a-time removal took several seconds here
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
