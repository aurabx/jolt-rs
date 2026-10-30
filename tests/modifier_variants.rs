//! Write rules for the modifier operations, checked through `transform()` with parsed specs.
//! The write rules follow Java JOLT's `OpMode` and `DataType`. Tests named `..._only_replaces_...`
//! and `wildcard_keeps_the_array_...` pin deliberate differences from Java.

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
fn wildcard_over_an_array_skips_scalar_elements_and_follows_the_rule_for_null() {
    let spec = json!({"items": {"*": {"a": "=concat('new')"}}});
    let input = json!({"items": ["keep", 1, true, null]});
    let expected = json!({"items": ["keep", 1, true, {"a": "new"}]});

    assert_all(
        &[OVERWRITE_NAMES[0], OVERWRITE_NAMES[1], DEFAULT],
        input.clone(),
        spec.clone(),
        expected,
    );
    assert_all(&[DEFINE], input.clone(), spec, input);
}

#[test]
fn wildcard_over_an_array_keeps_a_nested_array_a_literal_spec_cannot_walk() {
    let input = json!({"items": [[1, 2]]});
    assert_all(
        &ALL_NAMES,
        input.clone(),
        json!({"items": {"*": {"a": "new"}}}),
        input,
    );
}

#[test]
fn wildcards_walk_nested_arrays() {
    assert_all(
        &ALL_NAMES,
        json!({"items": [[{"v": "A"}], [{"v": "B"}]]}),
        json!({"items": {"*": {"*": {"lower": "=toLower(@(1,v))"}}}}),
        json!({"items": [[{"v": "A", "lower": "a"}], [{"v": "B", "lower": "b"}]]}),
    );
}

#[test]
fn wildcard_walks_a_root_array_and_an_empty_array() {
    let spec = json!({"*": {"a": "new"}});
    assert_all(
        &ALL_NAMES,
        json!([{}, {}]),
        spec.clone(),
        json!([{"a": "new"}, {"a": "new"}]),
    );
    assert_all(&ALL_NAMES, json!([]), spec, json!([]));
}

#[test]
fn path_reference_treats_body_keys_literally() {
    assert_all(
        &ALL_NAMES,
        json!({
            "users.1": {"code": "own"},
            "users": [{"code": "u0"}, {"code": "u1"}]
        }),
        json!({"users.1": {"a": {"v": "=concat(@(2,code))"}}}),
        json!({
            "users.1": {"code": "own", "a": {"v": "own"}},
            "users": [{"code": "u0"}, {"code": "u1"}]
        }),
    );
}

#[test]
fn hostile_body_key_does_not_panic() {
    let spec = json!({"items": {"*": {"a": {"b": "=toLower(@(2,v))"}}}});
    let input = json!({"items.]": {}, "items": {"][": {"v": "OWN"}}});

    assert_all(
        &ALL_NAMES,
        input,
        spec,
        json!({"items.]": {}, "items": {"][": {"v": "OWN", "a": {"b": "own"}}}}),
    );
}

// Each element used to clone its whole ancestor, so this size took many minutes.
#[test]
fn ancestor_reference_over_a_large_array_is_linear() {
    let input = json!({"list": vec![json!({}); 40_000], "name": "n"});
    let spec = json!({"list": {"*": {"s": {"x": "=concat(@(4,name))"}}}});

    let result = run("modify", input, spec);

    assert_eq!(result["list"][39_999]["s"]["x"], json!("n"));
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
fn path_reference_is_correct_after_visiting_a_sibling() {
    assert_all(
        &ALL_NAMES,
        json!({
            "first": {"name": "one", "deep": {}},
            "second": {"name": "two", "deep": {}}
        }),
        json!({
            "first": {"deep": {"x": "=concat(@(2,name))"}},
            "second": {"deep": {"x": "=concat(@(2,name))"}}
        }),
        json!({
            "first": {"name": "one", "deep": {"x": "one"}},
            "second": {"name": "two", "deep": {"x": "two"}}
        }),
    );
}

#[test]
fn wildcard_only_spec_leaves_null_absent_and_scalar_values_alone() {
    let spec = json!({"a": {"*": {"x": "new"}}});
    for input in [json!({"a": null}), json!({}), json!({"a": "old"})] {
        assert_all(&ALL_NAMES, input.clone(), spec.clone(), input);
    }
}

#[test]
fn wildcard_only_spec_leaves_nested_nulls_alone() {
    let spec = json!({"a": {"*": {"b": {"*": {"x": "new"}}}}});
    let input = json!({"a": [null, {"b": null}]});

    // The element spec has the literal key `b`, so a null element follows the write rule
    assert_all(
        &[OVERWRITE_NAMES[0], OVERWRITE_NAMES[1], DEFAULT],
        input.clone(),
        spec.clone(),
        json!({"a": [{}, {"b": null}]}),
    );
    assert_all(&[DEFINE], input.clone(), spec, input);
}

#[test]
fn spec_with_a_literal_key_still_creates_its_parent() {
    assert_all(
        &ALL_NAMES,
        json!({}),
        json!({"a": {"*": {"x": "star"}, "b": "lit"}}),
        json!({"a": {"b": "lit"}}),
    );
}

#[test]
fn literal_key_runs_before_the_wildcard() {
    let spec = json!({"a": {"*": {"x": "star"}, "b": {"x": "lit"}}});
    let input = json!({"a": {"b": {}, "c": {}}});

    assert_all(
        &OVERWRITE_NAMES,
        input.clone(),
        spec.clone(),
        json!({"a": {"b": {"x": "star"}, "c": {"x": "star"}}}),
    );
    assert_all(
        &[DEFAULT, DEFINE],
        input,
        spec,
        json!({"a": {"b": {"x": "lit"}, "c": {"x": "star"}}}),
    );
}

#[test]
fn literal_key_listed_first_also_runs_before_the_wildcard() {
    assert_all(
        &[DEFAULT, DEFINE],
        json!({"a": {"b": {}}}),
        json!({"a": {"b": {"x": "lit"}, "*": {"x": "star"}}}),
        json!({"a": {"b": {"x": "lit"}}}),
    );
}

#[test]
fn wildcard_and_literal_with_different_leaves_both_apply() {
    assert_all(
        &ALL_NAMES,
        json!({"a": {"b": {}}}),
        json!({"a": {"*": {"y": "star"}, "b": {"x": "lit"}}}),
        json!({"a": {"b": {"x": "lit", "y": "star"}}}),
    );
}

#[test]
fn wildcard_does_not_enter_a_leaf_literal_key() {
    assert_all(
        &ALL_NAMES,
        json!({"a": {"b": {"v": "V"}}}),
        json!({"a": {"*": {"l": "=toLower(@(1,v))"}, "c": "new"}}),
        json!({"a": {"b": {"v": "V", "l": "v"}, "c": "new"}}),
    );
}

#[test]
fn overwrite_wildcard_still_replaces_a_scalar_from_the_input() {
    assert_all(
        &OVERWRITE_NAMES,
        json!({"a": {"p": "s"}}),
        json!({"a": {"*": {"x": "new"}}}),
        json!({"a": {"p": {"x": "new"}}}),
    );
}

#[test]
fn wildcard_only_spec_leaves_root_null_and_scalar_alone() {
    for input in [json!(null), json!("text")] {
        assert_all(&ALL_NAMES, input.clone(), json!({"*": {"x": "new"}}), input);
    }
}

#[test]
fn path_reference_is_correct_after_a_child_returns_early() {
    for skipped in [json!({"b": "x"}), json!({"*": {"b": "x"}})] {
        assert_all(
            &[DEFAULT, DEFINE],
            json!({"items": [{"a": "keep", "v": "V"}]}),
            json!({"items": {"*": {"a": skipped, "s": {"y": "=concat(@(2,v))"}}}}),
            json!({"items": [{"a": "keep", "v": "V", "s": {"y": "V"}}]}),
        );
    }
}

#[test]
fn kept_value_skips_a_path_that_would_fail() {
    assert_all(
        &[DEFAULT, DEFINE],
        json!({"a": {"name": "old"}}),
        json!({"a": {"name": "=concat(@(9,name))"}}),
        json!({"a": {"name": "old"}}),
    );
}

#[test]
fn ancestor_missing_from_the_input_is_an_error_not_a_read_from_root() {
    let spec = json!({"new": {"sub": {"x": "=concat(@(2,name))"}}});
    for operation in ALL_NAMES {
        let result = transform(json!({"name": "root"}), &parse(operation, spec.clone()));
        assert!(
            matches!(result, Err(Error::PathEvaluationError(_))),
            "operation `{operation}` got {result:?}"
        );
    }
}

#[test]
fn element_cannot_redirect_an_ancestor_path_to_the_root() {
    let spec = json!({"items": {"*": {"meta": {"tag": {"v": "=concat(@(2,code))"}}}}});
    let input = json!({"code": "ROOT", "items": [{"x": 1}]});

    for operation in ALL_NAMES {
        let result = transform(input.clone(), &parse(operation, spec.clone()));
        assert!(
            matches!(result, Err(Error::PathEvaluationError(_))),
            "operation `{operation}` got {result:?}"
        );
    }
}

#[test]
fn path_level_above_the_root_is_an_error() {
    for operation in ALL_NAMES {
        let result = transform(
            json!({"name": "root"}),
            &parse(operation, json!({"x": "=concat(@(5,name))"})),
        );
        assert!(
            matches!(result, Err(Error::PathEvaluationError(_))),
            "operation `{operation}` got {result:?}"
        );
    }
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
