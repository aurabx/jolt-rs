use thiserror::Error as ThisError;
use std::{fmt, result::Result as StdResult, num::ParseIntError};

#[derive(Debug, ThisError)]
pub enum Error {
    #[error("Path index out of range when using wildcard. Index={idx};Length={len};")]
    PathIndexOutOfRange { idx: usize, len: usize },
    #[error("Match index out of range when using wildcard. Index={idx};Length={len};")]
    MatchIndexOutOfRange { idx: usize, len: usize },
    #[error("Unexpected end of right hand side expression.")]
    UnexpectedEndOfRhs,
    #[error("Unexpected right hand side expression.")]
    UnexpectedRhsEntry,
    #[error("Unexpected object in right hand side.")]
    UnexpectedObjectInRhs,
    #[error("Not implemented yet.")]
    Todo,
    /// Holds only the length of the index string. The string is a matched key or a value from
    /// the request body, and callers log these errors.
    #[error("Invalid index in expression. Length={len};\n{source}")]
    InvalidIndex {
        len: usize,
        #[source]
        source: ParseIntError,
    },
    #[error("Array index out of range. Index={idx};Length={len};")]
    ArrIndexOutOfRange { idx: usize, len: usize },
    #[error("Array index would pad the output with too many null slots. Index={idx};Length={len};Used={used};Limit={max} padded slots per shift;")]
    ArrayPaddingLimit {
        idx: usize,
        len: usize,
        used: usize,
        max: usize,
    },
    #[error(
        "Modifier run copied or wrote too much data. Used={used};Limit={max} estimated bytes per run;"
    )]
    ModifierBudgetLimit { used: usize, max: usize },
    /// `used` is the total the run would have reached with the failed charge, as in
    /// `ModifierBudgetLimit`. It is not what was spent before it.
    #[error(
        "Shift run copied too much data from the input. Used={used};Limit={max} estimated bytes per run;"
    )]
    ShiftBudgetLimit { used: usize, max: usize },
    #[error("Array index string is too long. Length={len};Max={max};")]
    IndexTooLong { len: usize, max: usize },
    /// Holds only the kind of the value, such as "object" or "number (negative)". The value
    /// comes from the request body, and callers log these errors.
    #[error("Json value can't be used as an index: {kind}")]
    InvalidIndexVal { kind: &'static str },
    #[error("Key not found in object:{0}")]
    KeyNotFound(MissingKey),
    #[error("Expression didn't evaluate to a string.")]
    EvalString,
    #[error("Empty path while executing shift. THIS SHOULD NEVER HAPPEN.")]
    ShiftEmptyPath,
    #[error("Path is not empty after executing shift. THIS SHOULD NEVER HAPPEN.")]
    ShiftPathNotEmpty,
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Invalid function: {0}")]
    InvalidFunction(String),
    #[error("Function argument error: {0}")]
    FunctionArgumentError(String),
    #[error("Path evaluation error: {0}")]
    PathEvaluationError(String),
}

pub type Result<T> = StdResult<T, Error>;

/// The key named in an [`Error::KeyNotFound`].
///
/// A key written in the spec is kept so spec mistakes are easy to find. A key built from the
/// request body, through `&` or `@`, is reduced to its length. Callers log these errors, and the
/// body may hold personal data of any size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingKey {
    Spec(String),
    FromInput { len: usize },
}

impl fmt::Display for MissingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MissingKey::Spec(key) => f.write_str(key),
            MissingKey::FromInput { len } => write!(f, "<key from input, {len} bytes>"),
        }
    }
}

/// Name the JSON type of a value, for error messages that must not contain the value.
pub(crate) fn json_kind(v: &serde_json::Value) -> &'static str {
    use serde_json::Value;

    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
