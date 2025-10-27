//! Fuzzing support for harmony-jolt
//!
//! This module provides fuzzing targets for testing the DSL parser.

use crate::dsl::{Lhs, Rhs};

/// Fuzz target for parsing LHS and RHS expressions
pub fn fuzz_dsl_parser(data: &str) {
    Lhs::parse(data).ok();
    Rhs::parse(data).ok();
}
