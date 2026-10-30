mod budget;
mod function_parser;
mod functions;
mod evaluator;
mod operation;

pub use operation::ModifierSpec;

use crate::Result;
use serde_json::Value;

/// Apply modifier transformation to input
pub fn modify(input: Value, spec: &ModifierSpec) -> Result<Value> {
    spec.apply(input)
}
