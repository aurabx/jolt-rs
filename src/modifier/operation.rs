use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{Map, Value};
use super::function_parser::FunctionCall;
use super::functions::execute_function;
use super::evaluator::evaluate_arg;

/// Upper bound on the estimated bytes one modifier run writes. A wildcard repeats its spec for
/// every element, so a small body can otherwise multiply a large value many thousands of times.
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

/// Estimated memory cost of a JSON value, on top of its text.
const VALUE_OVERHEAD_BYTES: usize = 32;

// Tracks the output written by one modifier run. See `MAX_OUTPUT_BYTES`.
#[derive(Default)]
struct OutputBudget {
    used: usize,
}

impl OutputBudget {
    fn charge(&mut self, key: &str, value: &Value) -> Result<()> {
        let used = self
            .used
            .saturating_add(key.len())
            .saturating_add(estimated_size(value));
        if used > MAX_OUTPUT_BYTES {
            return Err(Error::ModifierOutputLimit {
                used,
                max: MAX_OUTPUT_BYTES,
            });
        }
        self.used = used;
        Ok(())
    }
}

fn estimated_size(value: &Value) -> usize {
    let content = match value {
        Value::String(s) => s.len(),
        Value::Array(items) => items.iter().fold(0, |sum: usize, item| {
            sum.saturating_add(estimated_size(item))
        }),
        Value::Object(map) => map.iter().fold(0, |sum: usize, (key, item)| {
            sum.saturating_add(key.len())
                .saturating_add(estimated_size(item))
        }),
        _ => 0,
    };
    content.saturating_add(VALUE_OVERHEAD_BYTES)
}

/// Modifier operation variant
///
/// The write rules match Java JOLT's `OpMode`.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum ModifierVariant {
    /// Always writes.
    OverwriteBeta,
    /// Writes when the key is absent or its value is null.
    DefaultBeta,
    /// Writes only when the key is absent.
    DefineBeta,
}

/// Modifier specification
#[derive(Debug, Clone, PartialEq)]
pub struct ModifierSpec {
    pub variant: ModifierVariant,
    pub spec: Value,
}

impl ModifierSpec {
    #[cfg(test)]
    pub fn new(variant: ModifierVariant, spec: Value) -> Self {
        Self { variant, spec }
    }

    /// Deserialize the spec of a `modify-default-beta` operation
    pub fn deserialize_default_beta<'de, D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let spec = Value::deserialize(deserializer)?;
        Ok(Self {
            variant: ModifierVariant::DefaultBeta,
            spec,
        })
    }

    /// Deserialize the spec of a `modify-define-beta` operation
    pub fn deserialize_define_beta<'de, D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let spec = Value::deserialize(deserializer)?;
        Ok(Self {
            variant: ModifierVariant::DefineBeta,
            spec,
        })
    }

    /// Apply the modifier transformation to the input
    pub fn apply(&self, mut input: Value) -> Result<Value> {
        // Get context if present
        let context = input.get("context").cloned();

        // Clone input to use as root for path resolution
        let root = input.clone();

        // Apply the spec recursively
        let mut budget = OutputBudget::default();
        self.apply_spec_to_value(
            &mut input,
            &root,
            &self.spec,
            context.as_ref(),
            &[],
            &mut budget,
        )?;

        Ok(input)
    }

    /// Whether this variant may write to `key` of the object `target`
    fn should_write(&self, target: &Value, key: &str) -> bool {
        match self.variant {
            ModifierVariant::OverwriteBeta => true,
            ModifierVariant::DefaultBeta => matches!(target.get(key), None | Some(Value::Null)),
            ModifierVariant::DefineBeta => target.get(key).is_none(),
        }
    }

    /// Whether this variant may replace a non-object value with a new object.
    ///
    /// Java JOLT skips any non-null value that does not fit the spec. Overwrite still replaces
    /// it on object members, which is this crate's established behaviour. Array elements are
    /// filtered first by `fits_array_walk`.
    fn should_replace_with_object(&self, target: &Value) -> bool {
        match self.variant {
            ModifierVariant::OverwriteBeta => true,
            ModifierVariant::DefaultBeta => target.is_null(),
            ModifierVariant::DefineBeta => false,
        }
    }

    /// Whether a wildcard walk over an array should enter `element` with `spec`.
    ///
    /// Scalars do not fit an object spec, so a walk skips them under every variant, as Java
    /// JOLT does. A nested array fits only a spec that can walk it.
    fn fits_array_walk(element: &Value, spec: &Value) -> bool {
        match element {
            Value::Null | Value::Object(_) => true,
            Value::Array(_) => spec.get("*").is_some(),
            _ => false,
        }
    }

    fn apply_spec_to_value(
        &self,
        target: &mut Value,
        root: &Value,
        spec: &Value,
        context: Option<&Value>,
        path: &[String],
        budget: &mut OutputBudget,
    ) -> Result<()> {
        match spec {
            Value::Object(spec_map) => {
                // A wildcard walks an array in place. Anything else needs an object.
                let walks_array = target.is_array() && spec_map.contains_key("*");
                if !target.is_object() && !walks_array {
                    if !self.should_replace_with_object(target) {
                        return Ok(());
                    }
                    *target = Value::Object(Map::new());
                }

                // Clone spec_map to avoid borrow issues
                let spec_entries: Vec<(String, Value)> = spec_map
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                for (key, value) in spec_entries {
                    // Handle wildcard "*" to iterate over all children
                    if key == "*" {
                        if value.is_object() {
                            // Get keys to iterate over (from target, not spec)
                            let target_keys: Vec<String> = if let Some(obj) = target.as_object() {
                                obj.keys().cloned().collect()
                            } else if let Some(arr) = target.as_array() {
                                (0..arr.len()).map(|i| i.to_string()).collect()
                            } else {
                                vec![]
                            };

                            // Apply nested spec to each child
                            for child_key in target_keys {
                                let mut new_path = path.to_vec();
                                new_path.push(child_key.clone());

                                // Get the child value
                                let child_target =
                                    if target.is_array() {
                                        let idx: usize = child_key.parse().unwrap_or(0);
                                        target.as_array_mut().unwrap().get_mut(idx).filter(
                                            |element| Self::fits_array_walk(element, &value),
                                        )
                                    } else {
                                        target.as_object_mut().unwrap().get_mut(&child_key)
                                    };

                                if let Some(child) = child_target {
                                    self.apply_spec_to_value(
                                        child, root, &value, context, &new_path, budget,
                                    )?;
                                }
                            }
                        }
                        continue;
                    }

                    // Literal keys address object members. Array index keys are not supported.
                    if target.is_array() {
                        continue;
                    }

                    match &value {
                        Value::String(s) if s.starts_with('=') => {
                            // This is a function call
                            let function_call = FunctionCall::parse(s)?;

                            // A kept value must not fail because its function cannot run
                            if !self.should_write(target, &key) {
                                continue;
                            }

                            // Evaluate all arguments
                            let mut evaluated_args = Vec::new();
                            for arg in &function_call.args {
                                let evaluated = evaluate_arg(arg, target, root, context, path)?;
                                evaluated_args.push(evaluated);
                            }

                            // Execute the function
                            let result = execute_function(&function_call.name, &evaluated_args)?;
                            budget.charge(&key, &result)?;

                            let target_map = target.as_object_mut().unwrap();
                            target_map.insert(key.clone(), result);
                        }
                        Value::Object(_) => {
                            // Nested spec, recurse
                            let mut new_path = path.to_vec();
                            new_path.push(key.clone());

                            // Get or create the nested value
                            let target_map = target.as_object_mut().unwrap();
                            let nested_target = target_map
                                .entry(key.clone())
                                .or_insert(Value::Object(Map::new()));

                            self.apply_spec_to_value(
                                nested_target,
                                root,
                                &value,
                                context,
                                &new_path,
                                budget,
                            )?;
                        }
                        _ => {
                            // Regular value, just copy it
                            if self.should_write(target, &key) {
                                budget.charge(&key, &value)?;
                                let target_map = target.as_object_mut().unwrap();
                                target_map.insert(key.clone(), value.clone());
                            }
                        }
                    }
                }

                Ok(())
            }
            _ => {
                // Spec is not an object, can't apply
                Err(Error::ParseError(format!(
                    "Modifier spec must be an object, got: {:?}",
                    spec
                )))
            }
        }
    }
}

// `modify` and `modify-overwrite-beta` deserialize here. The other names have their own
// functions, because the variant comes from the operation name, not from the spec body.
impl<'de> Deserialize<'de> for ModifierSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(ModifierSpec {
            variant: ModifierVariant::OverwriteBeta,
            spec: value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_output_budget_stops_wildcard_amplification() {
        let elements = (MAX_OUTPUT_BYTES / 50_000) + 1;
        let input = json!({ "name": "x".repeat(50_000), "items": vec![json!({}); elements] });
        let spec = json!({ "items": { "*": { "x": "=concat(@(3,name))" } } });

        let result = ModifierSpec::new(ModifierVariant::OverwriteBeta, spec).apply(input);

        assert!(
            matches!(
                result,
                Err(Error::ModifierOutputLimit {
                    max: MAX_OUTPUT_BYTES,
                    ..
                })
            ),
            "got {result:?}"
        );
    }

    #[test]
    fn test_output_budget_allows_output_under_the_limit() {
        let input = json!({ "name": "x".repeat(50_000), "items": vec![json!({}); 100] });
        let spec = json!({ "items": { "*": { "x": "=concat(@(3,name))" } } });

        let result = ModifierSpec::new(ModifierVariant::OverwriteBeta, spec)
            .apply(input)
            .unwrap();

        assert_eq!(result["items"][99]["x"].as_str().unwrap().len(), 50_000);
    }

    #[test]
    fn test_kept_value_is_not_charged() {
        let mut budget = OutputBudget::default();
        let modifier = ModifierSpec::new(ModifierVariant::DefineBeta, json!({ "a": "new" }));
        let mut target = json!({ "a": "old" });

        modifier
            .apply_spec_to_value(
                &mut target,
                &json!({}),
                &modifier.spec,
                None,
                &[],
                &mut budget,
            )
            .unwrap();

        assert_eq!(budget.used, 0);
    }

    #[test]
    fn test_simple_concat() {
        let input = json!({
            "name": ["John", "Doe"]
        });

        let spec = json!({
            "fullName": "=concat(@(1,name[0]),' ',@(1,name[1]))"
        });

        let modifier = ModifierSpec::new(ModifierVariant::OverwriteBeta, spec);
        let result = modifier.apply(input).unwrap();

        assert_eq!(result["fullName"], json!("John Doe"));
        assert_eq!(result["name"], json!(["John", "Doe"]));
    }

    #[test]
    fn test_to_lower() {
        let input = json!({
            "text": "HELLO WORLD"
        });

        let spec = json!({
            "lower": "=toLower(@(1,text))"
        });

        let modifier = ModifierSpec::new(ModifierVariant::OverwriteBeta, spec);
        let result = modifier.apply(input).unwrap();

        assert_eq!(result["lower"], json!("hello world"));
    }

    #[test]
    fn test_nested_spec() {
        let input = json!({
            "string": "the QuIcK brOwn fox"
        });

        let spec = json!({
            "lower": {
                "leading": "=toLower(@(2,string))"
            }
        });

        let modifier = ModifierSpec::new(ModifierVariant::OverwriteBeta, spec);
        let result = modifier.apply(input).unwrap();

        assert_eq!(result["lower"]["leading"], json!("the quick brown fox"));
    }

    #[test]
    fn test_nested_function_call() {
        let input = json!({
            "text": "hello world"
        });

        let spec = json!({
            "result": "=toUpper(=toLower(@(1,text)))"
        });

        let modifier = ModifierSpec::new(ModifierVariant::OverwriteBeta, spec);
        let result = modifier.apply(input).unwrap();

        assert_eq!(result["result"], json!("HELLO WORLD"));
    }

    #[test]
    fn test_substring_with_numeric_args() {
        let input = json!({
            "date": "20250605"
        });

        let spec = json!({
            "year": "=substring(@(1,date),0,4)",
            "month": "=substring(@(1,date),4,6)",
            "day": "=substring(@(1,date),6,8)"
        });

        let modifier = ModifierSpec::new(ModifierVariant::OverwriteBeta, spec);
        let result = modifier.apply(input).unwrap();

        assert_eq!(result["year"], json!("2025"));
        assert_eq!(result["month"], json!("06"));
        assert_eq!(result["day"], json!("05"));
    }

    #[test]
    fn test_nested_concat_substring() {
        let input = json!({
            "date": "20250605"
        });

        let spec = json!({
            "formatted": "=concat(=substring(@(1,date),0,4),'-',=substring(@(1,date),4,6),'-',=substring(@(1,date),6,8))"
        });

        let modifier = ModifierSpec::new(ModifierVariant::OverwriteBeta, spec);
        let result = modifier.apply(input).unwrap();

        assert_eq!(result["formatted"], json!("2025-06-05"));
    }

    #[test]
    fn test_wildcard_object_iteration() {
        let input = json!({
            "entries": {
                "0": { "date": "20250101" },
                "1": { "date": "20251231" }
            }
        });

        let spec = json!({
            "entries": {
                "*": {
                    "formatted": "=concat(=substring(@(1,date),0,4),'-',=substring(@(1,date),4,6),'-',=substring(@(1,date),6,8))"
                }
            }
        });

        let modifier = ModifierSpec::new(ModifierVariant::OverwriteBeta, spec);
        let result = modifier.apply(input).unwrap();

        assert_eq!(result["entries"]["0"]["formatted"], json!("2025-01-01"));
        assert_eq!(result["entries"]["1"]["formatted"], json!("2025-12-31"));
    }

    #[test]
    fn test_wildcard_nested_object() {
        let input = json!({
            "temp": {
                "entry": {
                    "0": { "resource": { "started": "20000101" } },
                    "1": { "resource": { "started": "20241225" } }
                }
            }
        });

        let spec = json!({
            "temp": {
                "entry": {
                    "*": {
                        "resource": {
                            "started": "=concat(=substring(@(1,started),0,4),'-',=substring(@(1,started),4,6),'-',=substring(@(1,started),6,8))"
                        }
                    }
                }
            }
        });

        let modifier = ModifierSpec::new(ModifierVariant::OverwriteBeta, spec);
        let result = modifier.apply(input).unwrap();

        assert_eq!(
            result["temp"]["entry"]["0"]["resource"]["started"],
            json!("2000-01-01")
        );
        assert_eq!(
            result["temp"]["entry"]["1"]["resource"]["started"],
            json!("2024-12-25")
        );
    }
}
