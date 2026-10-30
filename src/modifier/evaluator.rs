use crate::{Error, Result};
use serde_json::Value;
use super::budget::RunBudget;
use super::function_parser::{FunctionArg, PathExpr, FunctionCall};
use super::functions::execute_function;

/// Evaluate a function argument against the current data and context
pub fn evaluate_arg(
    arg: &FunctionArg,
    current: &Value,
    root: &Value,
    context: Option<&Value>,
    path_stack: &[String],
    budget: &mut RunBudget,
) -> Result<Value> {
    match arg {
        FunctionArg::Literal(s) => Ok(Value::String(s.clone())),
        FunctionArg::Number(n) => Ok(Value::Number((*n).into())),
        FunctionArg::PathExpr(path_expr) => {
            evaluate_path_expr(path_expr, current, root, path_stack, budget)
        }
        FunctionArg::ContextRef(key) => {
            // Context references like ^value look up in the context object
            if let Some(ctx) = context {
                let value = ctx.get(key).ok_or_else(|| {
                    Error::PathEvaluationError(format!("Context key not found: {}", key))
                })?;
                budget.charge_value(value)?;
                Ok(value.clone())
            } else {
                Err(Error::PathEvaluationError(
                    "No context provided".to_string(),
                ))
            }
        }
        FunctionArg::Identifier(ident) => {
            // Try to look up as a path in current
            match current.get(ident) {
                Some(value) => {
                    budget.charge_value(value)?;
                    Ok(value.clone())
                }
                None => Ok(Value::String(ident.clone())),
            }
        }
        FunctionArg::Function(nested_call) => {
            // Recursively evaluate nested function call
            evaluate_function_call(nested_call, current, root, context, path_stack, budget)
        }
    }
}

/// Evaluate a nested function call recursively
fn evaluate_function_call(
    call: &FunctionCall,
    current: &Value,
    root: &Value,
    context: Option<&Value>,
    path_stack: &[String],
    budget: &mut RunBudget,
) -> Result<Value> {
    // Evaluate all arguments first
    let mut evaluated_args = Vec::new();
    for arg in &call.args {
        let evaluated = evaluate_arg(arg, current, root, context, path_stack, budget)?;
        evaluated_args.push(evaluated);
    }

    // Execute the nested function with evaluated arguments
    let result = execute_function(&call.name, &evaluated_args)?;
    budget.charge_value(&result)?;
    Ok(result)
}

/// Walk exact path segments from `base`. Segments can come from body keys, so they are never
/// parsed as path syntax.
fn resolve_segments<'a>(base: &'a Value, segments: &[String]) -> Option<&'a Value> {
    segments
        .iter()
        .try_fold(base, |current, segment| child(current, segment))
}

/// Evaluate a path expression like @(1,name[0])
///
/// In JOLT modifier semantics, @(n,path) navigates the context stack:
/// - @(1,path): reference from current object (the one being modified)
/// - @(2,path): go up 1 level (parent), then navigate path  
/// - @(n,path): go up n-1 levels from current, then navigate path
///
/// Note: @(0,...) is non-standard; we treat it as root for compatibility but
/// it should generally not be used.
fn evaluate_path_expr(
    path_expr: &PathExpr,
    current: &Value,
    root: &Value,
    path_stack: &[String],
    budget: &mut RunBudget,
) -> Result<Value> {
    // Get the base value by going up `level` levels
    let base = if path_expr.level == 0 {
        // @(0,...) means reference from root
        root
    } else if path_expr.level == 1 {
        // @(1,...) means reference from current object
        current
    } else if path_expr.level > path_stack.len() + 1 {
        // If level is greater than path depth + 1, use root
        root
    } else {
        // Navigate to the appropriate level by going up from current position
        // path_stack represents where we are in the spec hierarchy
        // level=2 means go up 1 from current, level=3 means go up 2, etc.
        let levels_up = path_expr.level - 1;
        let target_depth = path_stack.len().saturating_sub(levels_up);
        let target_path = &path_stack[..target_depth];

        // Start from root and navigate to the target depth, falling back to root if that fails
        resolve_segments(root, target_path).unwrap_or(root)
    };

    // Navigate the path
    let value = if path_expr.path.is_empty() {
        base
    } else {
        navigate_path(base, &path_expr.path)?
    };

    budget.charge_value(value)?;
    Ok(value.clone())
}

/// Get a child by key. An array takes the key as an index, because wildcard walks record
/// array positions in the path stack. Only the canonical form of an index matches.
fn child<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Array(items) => key
            .parse::<usize>()
            .ok()
            .filter(|index| index.to_string() == key)
            .and_then(|index| items.get(index)),
        _ => value.get(key),
    }
}

/// Navigate a path like "name[0]" or "lower.leading"
fn navigate_path<'a>(base: &'a Value, path: &str) -> Result<&'a Value> {
    let mut current = base;
    let mut remaining = path;

    while !remaining.is_empty() {
        // Check for array index notation: name[0]
        if let Some(open_bracket) = remaining.find('[') {
            // Get the key before the bracket
            let key = &remaining[..open_bracket];

            // Navigate to the key if not empty
            if !key.is_empty() {
                current = child(current, key)
                    .ok_or_else(|| Error::PathEvaluationError(format!("Key not found: {}", key)))?;
            }

            // Find the closing bracket
            let close_bracket = remaining[open_bracket..]
                .find(']')
                .map(|offset| open_bracket + offset)
                .ok_or_else(|| {
                    Error::PathEvaluationError(format!("Unclosed bracket in path: {}", path))
                })?;

            // Extract the index
            let index_str = &remaining[open_bracket + 1..close_bracket];
            let index: usize = index_str.parse().map_err(|_| {
                Error::PathEvaluationError(format!("Invalid array index: {}", index_str))
            })?;

            // Navigate to the array element
            current = current.get(index).ok_or_else(|| {
                Error::PathEvaluationError(format!("Array index out of bounds: {}", index))
            })?;

            // Update remaining path (skip past the ']')
            remaining = &remaining[close_bracket + 1..];

            // Skip the dot if present
            if remaining.starts_with('.') {
                remaining = &remaining[1..];
            }
        } else {
            // No array notation, check for dot notation
            if let Some(dot_pos) = remaining.find('.') {
                let key = &remaining[..dot_pos];
                current = child(current, key)
                    .ok_or_else(|| Error::PathEvaluationError(format!("Key not found: {}", key)))?;
                remaining = &remaining[dot_pos + 1..];
            } else {
                // Last segment
                current = child(current, remaining).ok_or_else(|| {
                    Error::PathEvaluationError(format!("Key not found: {}", remaining))
                })?;
                break;
            }
        }
    }

    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_navigate_simple_key() {
        let data = json!({
            "name": "John",
            "age": 30
        });

        let result = navigate_path(&data, "name").unwrap();
        assert_eq!(result, &json!("John"));
    }

    #[test]
    fn test_navigate_nested() {
        let data = json!({
            "person": {
                "name": "John"
            }
        });

        let result = navigate_path(&data, "person.name").unwrap();
        assert_eq!(result, &json!("John"));
    }

    #[test]
    fn test_navigate_array() {
        let data = json!({
            "name": ["John", "Doe"]
        });

        let result = navigate_path(&data, "name[0]").unwrap();
        assert_eq!(result, &json!("John"));

        let result = navigate_path(&data, "name[1]").unwrap();
        assert_eq!(result, &json!("Doe"));
    }

    #[test]
    fn test_navigate_array_index_segment() {
        let data = json!({
            "items": [{ "name": "first" }, { "name": "second" }]
        });

        let result = navigate_path(&data, "items.1.name").unwrap();
        assert_eq!(result, &json!("second"));

        assert!(navigate_path(&data, "items.2.name").is_err());
        assert!(navigate_path(&data, "items.x").is_err());
    }

    #[test]
    fn test_navigate_rejects_non_canonical_array_index() {
        let data = json!({ "items": ["p", "q"] });

        assert!(navigate_path(&data, "items.+1").is_err());
        assert!(navigate_path(&data, "items.01").is_err());
    }

    #[test]
    fn test_navigate_closing_bracket_before_opening_bracket_is_an_error() {
        let data = json!({ "a]b": ["p"], "a": { "]": 1 } });

        assert!(navigate_path(&data, "a]b[").is_err());
        assert_eq!(navigate_path(&data, "a]b[0]").unwrap(), &json!("p"));
    }

    #[test]
    fn test_resolve_segments_reads_keys_literally() {
        let data = json!({ "a.b": { "c[0]": "exact" }, "a": { "b": "wrong" } });
        let segments = ["a.b".to_string(), "c[0]".to_string()];

        assert_eq!(resolve_segments(&data, &segments), Some(&json!("exact")));
    }

    #[test]
    fn test_navigate_complex() {
        let data = json!({
            "lower": {
                "leading": "hello"
            }
        });

        let result = navigate_path(&data, "lower.leading").unwrap();
        assert_eq!(result, &json!("hello"));
    }
}
