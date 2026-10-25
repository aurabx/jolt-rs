use serde_json::{Map, Value};
use crate::spec::Spec;

/// Applies the `default` operation.
///
/// Follows Java JOLT's Defaultr: an object in the spec is walked into, and any other spec value
/// (string, number, bool, null or array) is a literal that is written where the input key is
/// missing or `null`. Existing non-null values are never changed.
pub(crate) fn default(mut input: Value, spec: &Spec) -> Value {
    if let Value::Object(spec) = spec.value() {
        apply_defaults(&mut input, spec);
    }
    input
}

fn apply_defaults(target: &mut Value, spec: &Map<String, Value>) {
    if target.is_null() {
        *target = Value::Object(Map::new());
    }
    // Defaults only apply to objects. A scalar or array at this level is left untouched.
    let Value::Object(target) = target else {
        return;
    };

    for (key, spec_value) in spec {
        let slot = target.entry(key.as_str()).or_insert(Value::Null);
        match spec_value {
            // An empty spec object on an absent key still defaults an empty object, as in Java.
            Value::Object(child_spec) => apply_defaults(slot, child_spec),
            literal => {
                if slot.is_null() {
                    *slot = literal.clone();
                }
            }
        }
    }
}

#[cfg(test)]
mod test {

    use serde_json::json;
    use super::*;

    fn run(spec: Value, input: Value) -> Value {
        let spec: Spec = serde_json::from_value(spec).expect("parsed spec");
        default(input, &spec)
    }

    #[test]
    fn test_insert_if_absent() {
        //given
        let spec: Spec = serde_json::from_value(json!({
            "a" : "default_value",
            "d" : {
                "e" : "default_value"
            }
        }))
        .expect("parsed spec");

        let input: Value = serde_json::from_value(json!({
            "b" : "b",
            "c" : "c"
        }))
        .expect("parsed spec");

        //when
        let output = default(input, &spec);

        //then
        assert_eq!(
            output,
            json!({
                "a" : "default_value",
                "b" : "b",
                "c" : "c",
                "d" : {
                    "e" : "default_value"
                }
            })
        )
    }

    #[test]
    fn test_skip_insert_if_present() {
        //given
        let spec: Spec = serde_json::from_value(json!({
            "a" : "default_value"
        }))
        .expect("parsed spec");

        let input: Value = serde_json::from_value(json!({
            "a" : "a",
            "b" : "b"
        }))
        .expect("parsed spec");

        //when
        let output = default(input, &spec);

        //then
        assert_eq!(
            output,
            json!({
                "a" : "a",
                "b" : "b"
            })
        )
    }

    #[test]
    fn test_array_literal_is_inserted_as_array() {
        let output = run(
            json!({ "meta": { "profile": ["http://example.org/StructureDefinition/x"] } }),
            json!({}),
        );

        assert_eq!(
            output,
            json!({ "meta": { "profile": ["http://example.org/StructureDefinition/x"] } })
        );
    }

    #[test]
    fn test_array_of_objects_literal_is_inserted_as_array() {
        let output = run(json!({ "coding": [{ "code": "a" }, { "code": "b" }] }), json!({}));

        assert_eq!(output, json!({ "coding": [{ "code": "a" }, { "code": "b" }] }));
    }

    #[test]
    fn test_existing_array_is_not_merged_with_default() {
        let output = run(json!({ "tags": ["x", "y"] }), json!({ "tags": ["z"] }));

        assert_eq!(output, json!({ "tags": ["z"] }));
    }

    #[test]
    fn test_null_value_is_replaced() {
        let output = run(json!({ "a": "d", "b": ["x"] }), json!({ "a": null, "b": null }));

        assert_eq!(output, json!({ "a": "d", "b": ["x"] }));
    }

    #[test]
    fn test_null_parent_is_replaced_with_object() {
        let output = run(json!({ "meta": { "source": "s" } }), json!({ "meta": null }));

        assert_eq!(output, json!({ "meta": { "source": "s" } }));
    }

    #[test]
    fn test_nested_defaults_merge_into_existing_object() {
        let output = run(
            json!({ "some": { "a": "b" } }),
            json!({ "some": { "b": "bb", "c": "cc" } }),
        );

        assert_eq!(output, json!({ "some": { "b": "bb", "c": "cc", "a": "b" } }));
    }

    #[test]
    fn test_deep_path_is_created_in_empty_input() {
        let output = run(json!({ "level1": { "level2": { "new": { "a": "b" } } } }), json!({}));

        assert_eq!(output, json!({ "level1": { "level2": { "new": { "a": "b" } } } }));
    }

    #[test]
    fn test_scalar_in_input_blocks_nested_default() {
        let output = run(json!({ "a": { "b": "c" } }), json!({ "a": "scalar" }));

        assert_eq!(output, json!({ "a": "scalar" }));
    }

    #[test]
    fn test_key_with_slash_and_tilde_is_literal() {
        let output = run(json!({ "a/b": "x", "c~d": "y" }), json!({}));

        assert_eq!(output, json!({ "a/b": "x", "c~d": "y" }));
    }

    #[test]
    fn test_empty_spec_object_defaults_empty_object() {
        let output = run(json!({ "a": {} }), json!({}));

        assert_eq!(output, json!({ "a": {} }));
    }
}
