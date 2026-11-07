use crate::error::json_kind;
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
        "split" => split(args),
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

/// Upper bound on the number of parts `split` returns. Each part becomes its own JSON string, so
/// an unbounded split of a large, attacker-supplied value would use many times its size in memory.
const MAX_SPLIT_PARTS: usize = 100_000;

/// Split a string into an array of strings on a separator.
///
/// Matches Java JOLT's `split(separator, string)` argument order, and like Java's `String.split`
/// it drops trailing empty parts. Java treats the separator as a regular expression; here it is a
/// plain string. A null input yields null, as in Java. Numbers and booleans are split as text.
fn split(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::FunctionArgumentError(format!(
            "split expects 2 arguments (separator, string), got {}",
            args.len()
        )));
    }

    let separator = value_to_string(&args[0]);
    if separator.is_empty() {
        return Err(Error::FunctionArgumentError(
            "split separator must not be empty".to_string(),
        ));
    }

    let s = match &args[1] {
        Value::Null => return Ok(Value::Null),
        Value::Array(_) | Value::Object(_) => {
            // Name only the type: the value comes from the request body, and callers log it.
            return Err(Error::FunctionArgumentError(format!(
                "split expects a string to split, got {}",
                json_kind(&args[1])
            )));
        }
        scalar => value_to_string(scalar),
    };

    let mut parts: Vec<&str> = s
        .split(separator.as_str())
        .take(MAX_SPLIT_PARTS + 1)
        .collect();
    if parts.len() > MAX_SPLIT_PARTS {
        return Err(Error::FunctionArgumentError(format!(
            "split would produce more than {} parts",
            MAX_SPLIT_PARTS
        )));
    }
    // Java keeps the input as the only part when the separator never occurs, even if it is empty
    if parts.len() > 1 {
        while parts.last().is_some_and(|part| part.is_empty()) {
            parts.pop();
        }
    }

    Ok(Value::Array(
        parts
            .into_iter()
            .map(|part| Value::String(part.to_string()))
            .collect(),
    ))
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
    fn test_split_error_names_only_the_argument_kind() {
        for (value, kind) in [
            (json!({ "secret": "PHI-SECRET" }), "object"),
            (json!(["PHI-SECRET"]), "array"),
        ] {
            let err = split(&[json!(","), value]).unwrap_err();
            let message = err.to_string();

            assert!(message.ends_with(kind), "got {message}");
            assert!(!message.contains("PHI-SECRET"), "Display leaked: {message}");
            assert!(
                !format!("{err:?}").contains("PHI-SECRET"),
                "Debug leaked: {err:?}"
            );
        }
    }

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

    #[test]
    fn test_split() {
        let args = vec![json!(","), json!("a,b,,c")];
        let result = split(&args).unwrap();
        assert_eq!(result, json!(["a", "b", "", "c"]));
    }

    #[test]
    fn test_split_drops_trailing_empty_parts() {
        assert_eq!(
            split(&[json!(","), json!("a,b,,")]).unwrap(),
            json!(["a", "b"])
        );
        assert_eq!(split(&[json!(","), json!(",")]).unwrap(), json!([]));
        assert_eq!(split(&[json!(","), json!(",a")]).unwrap(), json!(["", "a"]));
        assert_eq!(split(&[json!(","), json!("")]).unwrap(), json!([""]));
    }

    #[test]
    fn test_split_multi_char_separator_is_not_a_regex() {
        let args = vec![json!(". "), json!("a. b.c")];
        let result = split(&args).unwrap();
        assert_eq!(result, json!(["a", "b.c"]));
    }

    #[test]
    fn test_split_without_separator_match() {
        let args = vec![json!("|"), json!("abc")];
        let result = split(&args).unwrap();
        assert_eq!(result, json!(["abc"]));
    }

    #[test]
    fn test_split_null_input_is_null() {
        let args = vec![json!(","), Value::Null];
        assert_eq!(split(&args).unwrap(), Value::Null);
    }

    #[test]
    fn test_split_scalars_are_split_as_text() {
        assert_eq!(split(&[json!("."), json!(1.5)]).unwrap(), json!(["1", "5"]));
        assert_eq!(split(&[json!(","), json!(12)]).unwrap(), json!(["12"]));
        assert_eq!(
            split(&[json!("u"), json!(true)]).unwrap(),
            json!(["tr", "e"])
        );
    }

    #[test]
    fn test_split_rejects_bad_arguments() {
        assert!(split(&[json!(",")]).is_err());
        assert!(split(&[json!(""), json!("abc")]).is_err());
        assert!(split(&[json!(","), json!(["a,b"])]).is_err());
        assert!(split(&[json!(","), json!({"a": "b"})]).is_err());
    }

    #[test]
    fn test_split_part_limit() {
        let at_limit = ",".repeat(MAX_SPLIT_PARTS - 1) + "x";
        assert!(split(&[json!(","), json!(at_limit)]).is_ok());

        let over_limit = ",".repeat(MAX_SPLIT_PARTS) + "x";
        let err = split(&[json!(","), json!(over_limit)]).unwrap_err();
        assert!(err.to_string().contains("more than"), "got: {}", err);
    }
}
