use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{Map, Value};
use super::function_parser::FunctionCall;
use super::functions::execute_function;
use super::evaluator::evaluate_arg;

/// Modifier operation variant
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(clippy::enum_variant_names)]
pub enum ModifierVariant {
    OverwriteBeta,
    DefaultBeta,
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

    /// Apply the modifier transformation to the input
    pub fn apply(&self, mut input: Value) -> Result<Value> {
        // Get context if present
        let context = input.get("context").cloned();

        // Clone input to use as root for path resolution
        let root = input.clone();

        // Apply the spec recursively
        self.apply_spec_to_value(&mut input, &root, &self.spec, context.as_ref(), &[])?;

        Ok(input)
    }

    fn apply_spec_to_value(
        &self,
        target: &mut Value,
        root: &Value,
        spec: &Value,
        context: Option<&Value>,
        path: &[String],
    ) -> Result<()> {
        match spec {
            Value::Object(spec_map) => {
                // Ensure target is an object
                if !target.is_object() {
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
                        if let Value::Object(nested_spec) = &value {
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
                                let child_target = if target.is_array() {
                                    let idx: usize = child_key.parse().unwrap_or(0);
                                    target.as_array_mut().unwrap().get_mut(idx)
                                } else {
                                    target.as_object_mut().unwrap().get_mut(&child_key)
                                };

                                if let Some(child) = child_target {
                                    self.apply_spec_to_value(
                                        child,
                                        root,
                                        &Value::Object(nested_spec.clone()),
                                        context,
                                        &new_path,
                                    )?;
                                }
                            }
                        }
                        continue;
                    }

                    match &value {
                        Value::String(s) if s.starts_with('=') => {
                            // This is a function call
                            let function_call = FunctionCall::parse(s)?;

                            // Clone target for evaluation to avoid borrow issues
                            let target_clone = target.clone();

                            // Evaluate all arguments
                            let mut evaluated_args = Vec::new();
                            for arg in &function_call.args {
                                let evaluated =
                                    evaluate_arg(arg, &target_clone, root, context, path)?;
                                evaluated_args.push(evaluated);
                            }

                            // Execute the function
                            let result = execute_function(&function_call.name, &evaluated_args)?;

                            // Get mutable reference to target_map after evaluation
                            let target_map = target.as_object_mut().unwrap();

                            // Apply based on variant
                            match self.variant {
                                ModifierVariant::OverwriteBeta => {
                                    // Always write
                                    target_map.insert(key.clone(), result);
                                }
                                ModifierVariant::DefaultBeta => {
                                    // Only write if key doesn't exist
                                    target_map.entry(key.clone()).or_insert(result);
                                }
                                ModifierVariant::DefineBeta => {
                                    // Only write if key doesn't exist or is null
                                    if !target_map.contains_key(&key) || target_map[&key].is_null()
                                    {
                                        target_map.insert(key.clone(), result);
                                    }
                                }
                            }
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
                            )?;
                        }
                        _ => {
                            // Regular value, just copy it
                            let target_map = target.as_object_mut().unwrap();
                            match self.variant {
                                ModifierVariant::OverwriteBeta => {
                                    target_map.insert(key.clone(), value.clone());
                                }
                                ModifierVariant::DefaultBeta => {
                                    target_map.entry(key.clone()).or_insert(value.clone());
                                }
                                ModifierVariant::DefineBeta => {
                                    if !target_map.contains_key(&key) || target_map[&key].is_null()
                                    {
                                        target_map.insert(key.clone(), value.clone());
                                    }
                                }
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

// Deserialization support for ModifierSpec
impl<'de> Deserialize<'de> for ModifierSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(ModifierSpec {
            variant: ModifierVariant::OverwriteBeta, // Default variant
            spec: value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

        assert_eq!(result["temp"]["entry"]["0"]["resource"]["started"], json!("2000-01-01"));
        assert_eq!(result["temp"]["entry"]["1"]["resource"]["started"], json!("2024-12-25"));
    }
}
