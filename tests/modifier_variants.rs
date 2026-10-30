//! Write rules for the modifier operations, checked through `transform()` with parsed specs.
//! The expected results follow Java JOLT's `OpMode` and `DataType`.

use harmony_jolt::{transform, Error, TransformSpec};
use serde_json::{json, Value};

const OVERWRITE_NAMES: [&str; 2] = ["modify", "modify-overwrite-beta"];
const DEFAULT: &str = "modify-default-beta";
const DEFINE: &str = "modify-define-beta";
const ALL_NAMES: [&str; 4] = ["modify", "modify-overwrite-beta", DEFAULT, DEFINE];

fn parse(operation: &str, spec: Value) -> TransformSpec {
    serde_json::from_value(json!([{ "operation": operation, "spec": spec }]))
        .unwrap_or_else(|err| panic!("spec for `{operation}` should parse: {err}"))
}

fn run(operation: &str, input: Value, spec: Value) -> Value {
    transform(input, &parse(operation, spec))
        .unwrap_or_else(|err| panic!("`{operation}` should succeed: {err:?}"))
}

fn assert_all(operations: &[&str], input: Value, spec: Value, expected: Value) {
    for operation in operations {
        assert_eq!(
            run(operation, input.clone(), spec.clone()),
            expected,
            "operation `{operation}`"
        );
    }
}

#[test]
fn absent_key_is_written_by_every_variant() {
    for spec in [json!({"a": "=concat('new')"}), json!({"a": "new"})] {
        assert_all(&ALL_NAMES, json!({}), spec, json!({"a": "new"}));
    }
}

#[test]
fn null_value_is_written_by_overwrite_and_default_only() {
    for spec in [json!({"a": "=concat('new')"}), json!({"a": "new"})] {
        let input = json!({"a": null});
        assert_all(
            &OVERWRITE_NAMES,
            input.clone(),
            spec.clone(),
            json!({"a": "new"}),
        );
        assert_all(&[DEFAULT], input.clone(), spec.clone(), json!({"a": "new"}));
        assert_all(&[DEFINE], input, spec, json!({"a": null}));
    }
}

#[test]
fn existing_value_is_written_by_overwrite_only() {
    for spec in [json!({"a": "=concat('new')"}), json!({"a": "new"})] {
        let input = json!({"a": "old"});
        assert_all(
            &OVERWRITE_NAMES,
            input.clone(),
            spec.clone(),
            json!({"a": "new"}),
        );
        assert_all(&[DEFAULT, DEFINE], input, spec, json!({"a": "old"}));
    }
}

#[test]
fn falsy_existing_values_are_kept_by_default_and_define() {
    for existing in [json!(""), json!(0), json!(false), json!([]), json!({})] {
        assert_all(
            &[DEFAULT, DEFINE],
            json!({"a": existing}),
            json!({"a": "=concat('new')"}),
            json!({"a": existing}),
        );
    }
}

#[test]
fn nested_spec_applies_the_rule_at_the_leaf() {
    let spec = json!({"a": {"b": "=concat('new')", "c": "=concat('new')"}});
    let input = json!({"a": {"b": "old"}});

    assert_all(
        &OVERWRITE_NAMES,
        input.clone(),
        spec.clone(),
        json!({"a": {"b": "new", "c": "new"}}),
    );
    assert_all(
        &[DEFAULT, DEFINE],
        input,
        spec,
        json!({"a": {"b": "old", "c": "new"}}),
    );
}

#[test]
fn nested_spec_creates_an_absent_parent() {
    assert_all(
        &ALL_NAMES,
        json!({}),
        json!({"a": {"b": "=concat('new')"}}),
        json!({"a": {"b": "new"}}),
    );
}

#[test]
fn nested_spec_over_a_scalar_only_replaces_it_under_overwrite() {
    let spec = json!({"a": {"b": "=concat('new')"}});
    let input = json!({"a": "old"});

    assert_all(
        &OVERWRITE_NAMES,
        input.clone(),
        spec.clone(),
        json!({"a": {"b": "new"}}),
    );
    assert_all(&[DEFAULT, DEFINE], input, spec, json!({"a": "old"}));
}

#[test]
fn nested_spec_over_null_follows_the_write_rule() {
    let spec = json!({"a": {"b": "=concat('new')"}});
    let input = json!({"a": null});

    assert_all(
        &[OVERWRITE_NAMES[0], OVERWRITE_NAMES[1], DEFAULT],
        input.clone(),
        spec.clone(),
        json!({"a": {"b": "new"}}),
    );
    assert_all(&[DEFINE], input, spec, json!({"a": null}));
}

#[test]
fn literal_key_over_an_array_only_replaces_it_under_overwrite() {
    let spec = json!({"a": {"0": "=concat('new')"}});
    let input = json!({"a": [1, 2]});

    assert_all(
        &OVERWRITE_NAMES,
        input.clone(),
        spec.clone(),
        json!({"a": {"0": "new"}}),
    );
    assert_all(&[DEFAULT, DEFINE], input, spec, json!({"a": [1, 2]}));
}

#[test]
fn wildcard_iterates_array_elements_under_every_variant() {
    assert_all(
        &ALL_NAMES,
        json!({"items": [{"v": "A"}, {"v": "B"}]}),
        json!({"items": {"*": {"lower": "=toLower(@(1,v))"}}}),
        json!({"items": [{"v": "A", "lower": "a"}, {"v": "B", "lower": "b"}]}),
    );
}

#[test]
fn wildcard_over_an_array_applies_the_write_rule_per_element() {
    let spec = json!({"items": {"*": {"a": "=concat('new')"}}});
    let input = json!({"items": [{"a": "old"}, {"a": null}, {}]});

    assert_all(
        &OVERWRITE_NAMES,
        input.clone(),
        spec.clone(),
        json!({"items": [{"a": "new"}, {"a": "new"}, {"a": "new"}]}),
    );
    assert_all(
        &[DEFAULT],
        input.clone(),
        spec.clone(),
        json!({"items": [{"a": "old"}, {"a": "new"}, {"a": "new"}]}),
    );
    assert_all(
        &[DEFINE],
        input,
        spec,
        json!({"items": [{"a": "old"}, {"a": null}, {"a": "new"}]}),
    );
}

#[test]
fn wildcard_over_an_array_leaves_scalar_elements_to_the_write_rule() {
    let spec = json!({"items": {"*": {"a": "=concat('new')"}}});
    let input = json!({"items": ["keep", null]});

    assert_all(
        &OVERWRITE_NAMES,
        input.clone(),
        spec.clone(),
        json!({"items": [{"a": "new"}, {"a": "new"}]}),
    );
    assert_all(
        &[DEFAULT],
        input.clone(),
        spec.clone(),
        json!({"items": ["keep", {"a": "new"}]}),
    );
    assert_all(&[DEFINE], input.clone(), spec, input);
}

#[test]
fn wildcard_keeps_the_array_when_the_spec_also_has_a_literal_key() {
    assert_all(
        &ALL_NAMES,
        json!({"items": [{"v": "A"}]}),
        json!({"items": {"*": {"lower": "=toLower(@(1,v))"}, "0": "=concat('ignored')"}}),
        json!({"items": [{"v": "A", "lower": "a"}]}),
    );
}

#[test]
fn path_reference_from_inside_an_array_element_resolves_through_the_index() {
    assert_all(
        &ALL_NAMES,
        json!({
            "name": "root",
            "items": [{"name": "first", "sub": {}}, {"name": "second", "sub": {}}]
        }),
        json!({"items": {"*": {"sub": {"owner": "=toUpper(@(2,name))"}}}}),
        json!({
            "name": "root",
            "items": [
                {"name": "first", "sub": {"owner": "FIRST"}},
                {"name": "second", "sub": {"owner": "SECOND"}}
            ]
        }),
    );
}

#[test]
fn kept_value_does_not_run_its_function() {
    let spec = json!({"a": "=concat(@(1,missing))"});
    assert_all(
        &[DEFAULT, DEFINE],
        json!({"a": "old"}),
        spec.clone(),
        json!({"a": "old"}),
    );

    for operation in OVERWRITE_NAMES {
        let result = transform(json!({"a": "old"}), &parse(operation, spec.clone()));
        assert!(
            matches!(result, Err(Error::PathEvaluationError(_))),
            "operation `{operation}` got {result:?}"
        );
    }
}

#[test]
fn malformed_function_is_rejected_even_when_the_value_is_kept() {
    let spec = parse(DEFAULT, json!({"a": "=concat("}));
    assert!(transform(json!({"a": "old"}), &spec).is_err());
}

#[test]
fn unknown_modifier_name_is_rejected() {
    let parsed: Result<TransformSpec, _> =
        serde_json::from_value(json!([{ "operation": "modify-unknown-beta", "spec": {} }]));
    assert!(parsed.is_err());
}
