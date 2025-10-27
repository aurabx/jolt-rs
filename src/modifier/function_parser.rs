use crate::{Error, Result};

/// Represents a parsed function call like `=concat(@(1,name[0]),' ',@(1,name[1]))`
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    pub name: String,
    pub args: Vec<FunctionArg>,
}

/// Represents an argument to a function
#[derive(Debug, Clone, PartialEq)]
pub enum FunctionArg {
    /// String literal in single quotes like 'hello'
    Literal(String),
    /// Path expression like @(1,name[0])
    PathExpr(PathExpr),
    /// Context reference like ^value
    ContextRef(String),
    /// Simple identifier like bazinga
    Identifier(String),
}

/// Represents a path expression like @(1,name[0])
#[derive(Debug, Clone, PartialEq)]
pub struct PathExpr {
    /// Levels to go up (the first number in @(x,path))
    pub level: usize,
    /// Path to access (the second part in @(x,path))
    pub path: String,
}

impl FunctionCall {
    /// Parse a function call from a string like "=concat(@(1,name[0]),' ',@(1,name[1]))"
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();

        // Check if it starts with =
        if !input.starts_with('=') {
            return Err(Error::ParseError(format!(
                "Function call must start with '=', got: {}",
                input
            )));
        }

        let input = &input[1..]; // Remove leading =

        // Find the function name (everything before the first '(' or end of string)
        let open_paren_pos = input.find('(');

        let (name, args_str) = if let Some(pos) = open_paren_pos {
            let name = input[..pos].trim().to_string();
            let rest = &input[pos..];

            // Check if there are parentheses
            if !rest.starts_with('(') || !rest.ends_with(')') {
                return Err(Error::ParseError(format!(
                    "Malformed function call: {}",
                    input
                )));
            }

            // Extract arguments (everything between parentheses)
            let args_str = &rest[1..rest.len() - 1];
            (name, Some(args_str))
        } else {
            // No parentheses, just a function name (shorthand syntax like "=trim")
            (input.trim().to_string(), None)
        };

        // Parse arguments if present
        let args = if let Some(args_str) = args_str {
            parse_function_args(args_str)?
        } else {
            vec![]
        };

        Ok(FunctionCall { name, args })
    }
}

/// Parse function arguments from a comma-separated string
fn parse_function_args(input: &str) -> Result<Vec<FunctionArg>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_paren = 0;
    let mut escape_next = false;

    for ch in input.chars() {
        if escape_next {
            current.push(ch);
            escape_next = false;
            continue;
        }

        match ch {
            '\\' => {
                escape_next = true;
            }
            '\'' if !in_single_quote => {
                in_single_quote = true;
                current.push(ch);
            }
            '\'' if in_single_quote => {
                in_single_quote = false;
                current.push(ch);
            }
            '(' if !in_single_quote => {
                in_paren += 1;
                current.push(ch);
            }
            ')' if !in_single_quote => {
                in_paren -= 1;
                current.push(ch);
            }
            ',' if !in_single_quote && in_paren == 0 => {
                // Found argument separator
                let arg = current.trim();
                if !arg.is_empty() {
                    args.push(parse_single_arg(arg)?);
                }
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    // Don't forget the last argument
    let arg = current.trim();
    if !arg.is_empty() {
        args.push(parse_single_arg(arg)?);
    }

    Ok(args)
}

/// Parse a single function argument
fn parse_single_arg(input: &str) -> Result<FunctionArg> {
    let input = input.trim();

    // String literal in single quotes
    if input.starts_with('\'') && input.ends_with('\'') {
        let literal = &input[1..input.len() - 1];
        return Ok(FunctionArg::Literal(literal.to_string()));
    }

    // Path expression: @(level,path) or @(path)
    if input.starts_with('@') {
        return parse_path_expr(input);
    }

    // Context reference: ^identifier
    if let Some(stripped) = input.strip_prefix('^') {
        let ident = stripped.trim().to_string();
        return Ok(FunctionArg::ContextRef(ident));
    }

    // Try to parse as a number
    if let Ok(num) = input.parse::<i64>() {
        return Ok(FunctionArg::Literal(num.to_string()));
    }

    // Otherwise treat as identifier
    Ok(FunctionArg::Identifier(input.to_string()))
}

/// Parse a path expression like @(1,name[0]) or @(name)
fn parse_path_expr(input: &str) -> Result<FunctionArg> {
    let input = input.trim();

    let rest = input.strip_prefix('@').ok_or_else(|| {
        Error::ParseError(format!(
            "Path expression must start with '@', got: {}",
            input
        ))
    })?;

    // Check for shorthand: @2 or @path (no parentheses)
    if !rest.starts_with('(') {
        // This is a shorthand, try to parse as number or treat as path
        if let Ok(level) = rest.parse::<usize>() {
            return Ok(FunctionArg::PathExpr(PathExpr {
                level,
                path: String::new(),
            }));
        } else {
            return Ok(FunctionArg::PathExpr(PathExpr {
                level: 0,
                path: rest.to_string(),
            }));
        }
    }

    // Full syntax: @(level,path) or @(path)
    if !rest.starts_with('(') || !rest.ends_with(')') {
        return Err(Error::ParseError(format!(
            "Malformed path expression: {}",
            input
        )));
    }

    let inner = &rest[1..rest.len() - 1];

    // Split by comma to get level and path
    // But we need to be careful about commas inside the path part
    let comma_pos = inner.find(',');

    let (level, path) = if let Some(pos) = comma_pos {
        let level_str = inner[..pos].trim();
        let path_str = inner[pos + 1..].trim();

        let level = level_str.parse::<usize>().map_err(|_| {
            Error::ParseError(format!("Invalid level in path expression: {}", level_str))
        })?;

        (level, path_str.to_string())
    } else {
        // No comma, so it's just @(path) which means level 0
        (0, inner.trim().to_string())
    };

    Ok(FunctionArg::PathExpr(PathExpr { level, path }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_concat() {
        let call = FunctionCall::parse("=concat(@(1,name[0]),' ',@(1,name[1]))").unwrap();
        assert_eq!(call.name, "concat");
        assert_eq!(call.args.len(), 3);

        match &call.args[0] {
            FunctionArg::PathExpr(p) => {
                assert_eq!(p.level, 1);
                assert_eq!(p.path, "name[0]");
            }
            _ => panic!("Expected PathExpr"),
        }

        match &call.args[1] {
            FunctionArg::Literal(s) => assert_eq!(s, " "),
            _ => panic!("Expected Literal"),
        }
    }

    #[test]
    fn test_parse_to_lower() {
        let call = FunctionCall::parse("=toLower(@(2,string))").unwrap();
        assert_eq!(call.name, "toLower");
        assert_eq!(call.args.len(), 1);
    }

    #[test]
    fn test_parse_shorthand() {
        let call = FunctionCall::parse("=trim").unwrap();
        assert_eq!(call.name, "trim");
        assert_eq!(call.args.len(), 0);
    }
}
