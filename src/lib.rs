mod spec;
mod shift;
mod default;
mod remove;
mod modifier;
mod pointer;
mod transform;
mod error;
pub mod dsl;
pub mod fuzz;

use serde_json::Value;
use transform::Transform;

use crate::default::default;
use crate::remove::remove;
use crate::modifier::modify;
use crate::spec::SpecEntry;

pub use spec::TransformSpec;

pub use error::{Error, MissingKey, Result};

/// Perform JSON to JSON transformation where the "specification" is a JSON.
///
/// Inspired by Java library [Jolt](https://github.com/bazaarvoice/jolt).
///
/// The transformation can compose of many operations that are chained together.
///
/// ### Operations
/// 1. [`shift`](TransformSpec#shift-operation): copy data from the input tree and put it in the output tree
/// 2. [`default`](TransformSpec#default-operation): apply default values to the tree
/// 3. [`remove`](TransformSpec#remove-operation): remove data from the tree
/// 4. [`modify`](TransformSpec#modify-operation): modify data using functions like concat, toLower, etc.
///
/// For example, if you want to repack your JSON record, you can do the following:
/// ```
/// use serde_json::{json, Value};
/// use harmony_jolt::{transform, TransformSpec};
///
/// let input: Value = serde_json::from_str(r#"
///     {
///         "id": 1,
///         "name": "John Smith",
///         "account": {
///             "id": 1000,
///             "type": "Checking"
///         }
///     }
/// "#).unwrap();
///
/// let spec: TransformSpec =
/// serde_json::from_str(r#"[
///     {
///       "operation": "shift",
///       "spec": {
///         "name": "data.name",
///         "account": "data.account"
///       }
///     }
///   ]"#).unwrap();
///
/// let output = transform(input, &spec).unwrap();
///
/// assert_eq!(output, json!({
///     "data" : {
///       "name": "John Smith",
///       "account": {
///         "id": 1000,
///         "type": "Checking"
///       }
///     }
/// }));
/// ```
///
/// Checkout supported operations in [TransformSpec] docs.
pub fn transform(input: Value, spec: &TransformSpec) -> Result<Value> {
    let mut result = input;
    for entry in spec.entries() {
        match entry {
            SpecEntry::Shift(shift) => result = shift.apply(&result)?,
            SpecEntry::Default(spec) => result = default(result, spec),
            SpecEntry::Modify(spec)
            | SpecEntry::ModifyDefault(spec)
            | SpecEntry::ModifyDefine(spec) => result = modify(result, spec)?,
            SpecEntry::Remove(spec) => result = remove(result, spec),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod test {

    use serde_json::json;
    use super::*;

    #[test]
    fn test_transform() {
        let spec: TransformSpec = serde_json::from_value(json!(
            [
                {
                  "operation": "shift",
                  "spec": {
                    "a": "a_new",
                    "c": "c_new"
                  }
                }
            ]
        ))
        .expect("parsed spec");

        let source = json!({
            "a": "b",
            "c": "d"
        });
        let result = transform(source, &spec).unwrap();

        assert_eq!(
            result,
            json!({
                "a_new": "b",
                "c_new": "d"
            })
        );
    }

    #[test]
    fn test_invalid_array_index_error_includes_raw_value() {
        // Using wildcard LHS, the captured key (e.g. "not_numeric") is used as an array index in RHS via &(0,0)
        // This should fail and the error should include the raw invalid value.
        let spec: TransformSpec = serde_json::from_value(json!(
            [
                {
                  "operation": "shift",
                  "spec": {
                    "*": "out[&(0,0)]"
                  }
                }
            ]
        ))
        .expect("parsed spec");

        let source = json!({
            "not_numeric": 1
        });

        let result = transform(source, &spec);
        assert!(
            result.is_err(),
            "Expected error when using non-numeric key as array index"
        );

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not_numeric"),
            "Error should include the invalid index value, got: {}",
            err_msg
        );
        assert!(
            err_msg.contains("Invalid index in expression"),
            "Error should mention 'Invalid index in expression', got: {}",
            err_msg
        );
    }
}
