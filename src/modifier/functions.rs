use crate::{Error, Result};
use serde_json::Value;

/// Execute a function by name with given arguments
pub fn execute_function(name: &str, args: &[Value]) -> Result<Value> {
    match name {
        "concat" => concat(args),
        "toLower" => to_lower(args),
        "toUpper" => to_upper(args),
        "substring" => substring(args),
        "trim" => trim(args),
        "join" => join(args),
        _ => Err(Error::InvalidFunction(name.to_string())),
    }
}

/// Concatenate multiple values into a single string
fn concat(args: &[Value]) -> Result<Value> {
    let mut result = String::new();

    for arg in args {
        result.push_str(&value_to_string(arg));
    }

    Ok(Value::String(result))
}

/// Convert a value to lowercase string
fn to_lower(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::FunctionArgumentError(format!(
            "toLower expects 1 argument, got {}",
            args.len()
        )));
    }

    let s = value_to_string(&args[0]);
    Ok(Value::String(s.to_lowercase()))
}

/// Convert a value to uppercase string
fn to_upper(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::FunctionArgumentError(format!(
            "toUpper expects 1 argument, got {}",
            args.len()
        )));
    }

    let s = value_to_string(&args[0]);
    Ok(Value::String(s.to_uppercase()))
}

/// Extract substring from a string
fn substring(args: &[Value]) -> Result<Value> {
    if args.len() != 3 {
        return Err(Error::FunctionArgumentError(format!(
            "substring expects 3 arguments (string, start, end), got {}",
            args.len()
        )));
    }

    let s = value_to_string(&args[0]);
    let start = args[1]
        .as_i64()
        .ok_or_else(|| Error::FunctionArgumentError("start index must be a number".to_string()))?
        as usize;
    let end = args[2]
        .as_i64()
        .ok_or_else(|| Error::FunctionArgumentError("end index must be a number".to_string()))?
        as usize;

    // Validate indices
    if start > end {
        return Err(Error::FunctionArgumentError(format!(
            "start index ({}) must be <= end index ({})",
            start, end
        )));
    }

    if end > s.len() {
        return Err(Error::FunctionArgumentError(format!(
            "end index ({}) is out of bounds (string length: {})",
            end,
            s.len()
        )));
    }

    if start == end {
        return Err(Error::FunctionArgumentError(
            "start and end indices cannot be equal".to_string(),
        ));
    }

    let result = s.chars().skip(start).take(end - start).collect::<String>();
    Ok(Value::String(result))
}

/// Trim whitespace from a string
fn trim(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::FunctionArgumentError(format!(
            "trim expects 1 argument, got {}",
            args.len()
        )));
    }

    let s = value_to_string(&args[0]);
    Ok(Value::String(s.trim().to_string()))
}

/// Join array elements with a separator
fn join(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(Error::FunctionArgumentError(format!(
            "join expects at least 2 arguments (separator, values...), got {}",
            args.len()
        )));
    }

    let separator = value_to_string(&args[0]);
    let parts: Vec<String> = args[1..]
        .iter()
        .filter(|v| !v.is_null())
        .map(value_to_string)
        .collect();

    Ok(Value::String(parts.join(&separator)))
}

/// Convert a JSON value to a string representation
fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(value_to_string).collect();
            items.join(",")
        }
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_concat() {
        let args = vec![json!("Hello"), json!(" "), json!("World")];
        let result = concat(&args).unwrap();
        assert_eq!(result, json!("Hello World"));
    }

    #[test]
    fn test_to_lower() {
        let args = vec![json!("HELLO")];
        let result = to_lower(&args).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn test_to_upper() {
        let args = vec![json!("hello")];
        let result = to_upper(&args).unwrap();
        assert_eq!(result, json!("HELLO"));
    }

    #[test]
    fn test_substring() {
        let args = vec![json!("the quick brown fox"), json!(0), json!(9)];
        let result = substring(&args).unwrap();
        assert_eq!(result, json!("the quick"));
    }

    #[test]
    fn test_trim() {
        let args = vec![json!("  hello  ")];
        let result = trim(&args).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn test_join() {
        let args = vec![json!("_"), json!("a"), json!("b"), json!("c")];
        let result = join(&args).unwrap();
        assert_eq!(result, json!("a_b_c"));
    }
}
