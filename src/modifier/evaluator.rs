use crate::{Error, Result};
use serde_json::Value;
use super::function_parser::{FunctionArg, PathExpr, FunctionCall};
use super::functions::execute_function;

/// Evaluate a function argument against the current data and context
pub fn evaluate_arg(
    arg: &FunctionArg,
    current: &Value,
    root: &Value,
    context: Option<&Value>,
    path_stack: &[String],
) -> Result<Value> {
    match arg {
        FunctionArg::Literal(s) => Ok(Value::String(s.clone())),
        FunctionArg::Number(n) => Ok(Value::Number((*n).into())),
        FunctionArg::PathExpr(path_expr) => {
            evaluate_path_expr(path_expr, current, root, path_stack)
        }
        FunctionArg::ContextRef(key) => {
            // Context references like ^value look up in the context object
            if let Some(ctx) = context {
                ctx.get(key).cloned().ok_or_else(|| {
                    Error::PathEvaluationError(format!("Context key not found: {}", key))
                })
            } else {
                Err(Error::PathEvaluationError(
                    "No context provided".to_string(),
                ))
            }
        }
        FunctionArg::Identifier(ident) => {
            // Try to look up as a path in current
            Ok(current
                .get(ident)
                .cloned()
                .unwrap_or(Value::String(ident.clone())))
        }
        FunctionArg::Function(nested_call) => {
            // Recursively evaluate nested function call
            evaluate_function_call(nested_call, current, root, context, path_stack)
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
) -> Result<Value> {
    // Evaluate all arguments first
    let mut evaluated_args = Vec::new();
    for arg in &call.args {
        let evaluated = evaluate_arg(arg, current, root, context, path_stack)?;
        evaluated_args.push(evaluated);
    }

    // Execute the nested function with evaluated arguments
    execute_function(&call.name, &evaluated_args)
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

        // Start from root and navigate to the target depth
        if target_path.is_empty() {
            root
        } else {
            let path_str = target_path.join(".");
            match navigate_path(root, &path_str) {
                Ok(v) => {
                    return if path_expr.path.is_empty() {
                        Ok(v)
                    } else {
                        navigate_path(&v, &path_expr.path)
                    }
                }
                Err(_) => root, // Fall back to root if navigation fails
            }
        }
    };

    // Navigate the path
    if path_expr.path.is_empty() {
        return Ok(base.clone());
    }

    navigate_path(base, &path_expr.path)
}

/// Navigate a path like "name[0]" or "lower.leading"
fn navigate_path(base: &Value, path: &str) -> Result<Value> {
    let mut current = base;
    let mut remaining = path;

    while !remaining.is_empty() {
        // Check for array index notation: name[0]
        if let Some(open_bracket) = remaining.find('[') {
            // Get the key before the bracket
            let key = &remaining[..open_bracket];

            // Navigate to the key if not empty
            if !key.is_empty() {
                current = current
                    .get(key)
                    .ok_or_else(|| Error::PathEvaluationError(format!("Key not found: {}", key)))?;
            }

            // Find the closing bracket
            let close_bracket = remaining.find(']').ok_or_else(|| {
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
                current = current
                    .get(key)
                    .ok_or_else(|| Error::PathEvaluationError(format!("Key not found: {}", key)))?;
                remaining = &remaining[dot_pos + 1..];
            } else {
                // Last segment
                current = current.get(remaining).ok_or_else(|| {
                    Error::PathEvaluationError(format!("Key not found: {}", remaining))
                })?;
                break;
            }
        }
    }

    Ok(current.clone())
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
        assert_eq!(result, json!("John"));
    }

    #[test]
    fn test_navigate_nested() {
        let data = json!({
            "person": {
                "name": "John"
            }
        });

        let result = navigate_path(&data, "person.name").unwrap();
        assert_eq!(result, json!("John"));
    }

    #[test]
    fn test_navigate_array() {
        let data = json!({
            "name": ["John", "Doe"]
        });

        let result = navigate_path(&data, "name[0]").unwrap();
        assert_eq!(result, json!("John"));

        let result = navigate_path(&data, "name[1]").unwrap();
        assert_eq!(result, json!("Doe"));
    }

    #[test]
    fn test_navigate_complex() {
        let data = json!({
            "lower": {
                "leading": "hello"
            }
        });

        let result = navigate_path(&data, "lower.leading").unwrap();
        assert_eq!(result, json!("hello"));
    }
}
