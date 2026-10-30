use thiserror::Error as ThisError;
use std::{result::Result as StdResult, num::ParseIntError};

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
    #[error("Invalid index in expression: {raw}\n{source}")]
    InvalidIndex {
        raw: String,
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
    #[error("Array index string is too long. Length={len};Max={max};")]
    IndexTooLong { len: usize, max: usize },
    #[error("Json value can't be used as an index: {0:?}")]
    InvalidIndexVal(serde_json::Value),
    #[error("Key not found in object:{0}")]
    KeyNotFound(String),
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
