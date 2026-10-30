use serde::Deserialize;
use serde_json::Value;
use crate::{shift::Shift, modifier::ModifierSpec};

/// The JSON transformation specification.
///
/// Composes a list of operation specifications. Each operation has its own DSL (Domain Specific
/// Language) in order to facilitate its narrow job.
///
/// ```
/// use harmony_jolt::TransformSpec;
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
/// ```
///
/// ### `Shift` operation
/// Specifies where the data from the input JSON should be placed in the output JSON, or in other
/// words, how the input JSON/data should be shifted around to make the output JSON/data.
///
/// At a base level, a single `shift` operation is a mapping from an input path to an output path,
/// similar to the `mv` command in Unix, `mv /var/data /var/backup/data`.
///
/// The input path is a JSON tree structure, and the output path is flattened "dot notation" path
/// notation.
///
///  For example, given this simple input JSON:
///  <pre>
/// {
///     "id": 1,
///     "name": "John Smith",
///     "account": {
///         "id": 1000,
///         "type": "Checking"
///     }
/// }
/// </pre>
/// A simple spec could be constructed by copying that input, and modifying it to supply an output
/// path for each piece of data:
/// <pre>
/// {
///     "id": "data.id",
///     "name": "data.name",
///     "account": "data.account"
/// }
/// </pre>
/// would produce the following output JSON:
/// <pre>
/// {
///     "data" : {
///         "id": 1,
///         "name": "John Smith",
///         "account": {
///             "id": 1000,
///             "type": "Checking"
///         }
///     }
/// }
/// </pre>
/// #### Wildcards
/// The `shift` specification on the keys level supports wildcards and conditions:  
///     1. `*` - match everything  
///     2. `name1|name2|nameN` - match any of the specified names
///
/// `&` lookup allows referencing the values captured by the `*` or `|`.  
/// It allows for specs to be more compact. For example, for this input:
///  <pre>
/// {
///     "id": 1,
///     "name": "John Smith",
///     "account": {
///         "id": 1000,
///         "type": "Checking"
///     }
/// }
/// </pre>
/// to get the output:
/// <pre>
/// {
///     "data" : {
///         "id": 1,
///         "name": "John Smith",
///         "account": {
///             "id": 1000,
///             "type": "Checking"
///         }
///     }
/// }
/// </pre>
/// the spec with wildcards would be:
/// <pre>
/// {
///     "*": "data.&0"
/// }
/// </pre>
/// If you want only `id` and `name` in the output, the spec is:
/// <pre>
/// {
///     "id|name": "data.&0"
/// }
/// </pre>
///
///
/// On the right hand side, `[#n]` uses a match count as an array index. It counts how many
/// matches the spec level `n` steps up has made so far, where the current leaf is level 0.
///
/// `&` wildcard also allows to dereference any level of the path of given node:
/// <pre>
/// {
///     "foo": {
///         "bar" : {
///             "baz": "new_location.&0.&1.&2" // &0 = baz, &1 = bar, &2 = foo
///             }
///         }
///     }
/// </pre>
/// for the input:
/// <pre>
/// {
///     "foo": {
///       "bar": {
///         "baz": "value"
///       }
///     }
///   }
/// </pre>
/// will produce:
/// <pre>
/// {
///     "new_location": {
///       "baz": {
///         "bar": {
///           "foo": "value"
///         }
///       }
///     }
/// }
/// </pre>
///
/// ### `Default` operation
/// Applies default values where a value is missing or `null` in the input JSON. Arrays in the
/// spec are literal values, so they are written as is.
///
///  For example, given this simple input JSON:
///  <pre>
/// {
///     "phones": {
///         "mobile": "01234567",
///         "country": "US"
///     }
/// }
/// </pre>
/// with the following specification for `default` operation:
/// <pre>
/// {
///     "phones": {
///         "mobile": "0000000",
///         "code": "+1"
///     }
/// }
/// </pre>
/// the output JSON will be:
/// <pre>
/// {
///     "phones": {
///         "mobile": "01234567",
///         "country": "US",
///         "code": "+1"
///     }
/// }
/// </pre>
/// As you can see, the field `mobile` remains not affected while the `code` has a default '+1' value.
///
/// ### `Modify` operation
/// Sets values using functions: `concat`, `toLower`, `toUpper`, `substring`, `trim`, `join` and
/// `split`. The operation can be named `modify`, `modify-overwrite-beta`, `modify-default-beta`
/// or `modify-define-beta`.
///
/// For example, given this input JSON:
/// <pre>
/// {
///     "first": "John",
///     "last": "Doe"
/// }
/// </pre>
/// this specification adds a `name` field:
/// <pre>
/// {
///     "name": "=concat(@(1,first),' ',@(1,last))"
/// }
/// </pre>
///
/// ### `Remove` operation
/// Removes content from the input JSON.
/// The spec structure matches the input JSON structure. The value of fields is ignored.
/// A `*` in a key matches any run of characters, and keys match array indexes on arrays.
///
///  For example, given this simple input JSON:
///  <pre>
/// {
///     "phones": {
///         "mobile": "01234567",
///         "country": "US"
///     }
/// }
/// </pre>
/// you can remove the `country` by the following specification for `remove` operation:
/// <pre>
/// {
///     "phones": {
///         "country": ""
///     }
/// }
/// </pre>
/// the output JSON will be:
/// <pre>
/// {
///     "phones": {
///         "mobile": "01234567"
///     }
/// }
/// </pre>
#[derive(Debug, Deserialize, Default, Clone, PartialEq)]
pub struct TransformSpec(Vec<SpecEntry>);

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(tag = "operation", content = "spec")]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SpecEntry {
    Shift(Shift),
    Default(Spec),
    #[serde(
        alias = "modify-overwrite-beta",
        alias = "modify-default-beta",
        alias = "modify-define-beta"
    )]
    Modify(ModifierSpec),
    Remove(Spec),
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub(crate) struct Spec(Value);

impl TransformSpec {
    pub(crate) fn entries(&self) -> impl Iterator<Item = &SpecEntry> {
        self.0.iter()
    }
}

impl Spec {
    pub(crate) fn value(&self) -> &Value {
        &self.0
    }
}

#[cfg(test)]
mod test {

    use serde_json::json;
    use super::*;

    #[test]
    fn test_de_from_str() {
        let spec = r#"
        [
            {
                "operation": "shift",
                "spec": {
                    "id": "__data.id",
                    "name": "__data.name",
                    "account": "__data.account"
                }
            }
        ]"#;
        let result: TransformSpec = serde_json::from_str(spec).expect("parsed transform spec");

        assert_eq!(
            result,
            TransformSpec(vec![SpecEntry::Shift(
                serde_json::from_value(json!({
                    "id": "__data.id",
                    "name": "__data.name",
                    "account": "__data.account"
                }))
                .unwrap()
            )])
        );
    }
}
