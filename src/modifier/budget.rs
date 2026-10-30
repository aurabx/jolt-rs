use crate::{Error, Result};
use serde_json::Value;
use std::mem::size_of;

/// Upper bound on the estimated bytes one modifier run copies or writes. A wildcard repeats its
/// spec for every element, so a small body could otherwise multiply a large value many thousands
/// of times, either in memory or as repeated copying of function arguments.
pub(super) const MAX_RUN_BYTES: usize = 64 * 1024 * 1024;

/// Allocation for the first entry of an object. Measured at about 400 bytes for a one-entry
/// `serde_json` map, so this leans high.
const MAP_ALLOCATION_BYTES: usize = 512;

/// Cost of one object entry on top of its key text and value.
const ENTRY_OVERHEAD_BYTES: usize = size_of::<String>();

/// Tracks the bytes copied and written by one modifier run. See `MAX_RUN_BYTES`.
#[derive(Default)]
pub(super) struct RunBudget {
    pub(super) used: usize,
}

impl RunBudget {
    /// Charge a value about to be copied. Call this before cloning, so that the cost of the
    /// clone is paid for even when the function that receives it returns something small.
    pub(super) fn charge_value(&mut self, value: &Value) -> Result<()> {
        self.charge(estimated_size(value))
    }

    /// Charge writing `value` to `key` of the object `target`
    pub(super) fn charge_entry(&mut self, target: &Value, key: &str, value: &Value) -> Result<()> {
        let allocation = match target.as_object() {
            Some(map) if map.is_empty() => MAP_ALLOCATION_BYTES,
            _ => 0,
        };
        self.charge(
            allocation
                .saturating_add(ENTRY_OVERHEAD_BYTES)
                .saturating_add(key.len())
                .saturating_add(estimated_size(value)),
        )
    }

    fn charge(&mut self, bytes: usize) -> Result<()> {
        let used = self.used.saturating_add(bytes);
        if used > MAX_RUN_BYTES {
            return Err(Error::ModifierBudgetLimit {
                used,
                max: MAX_RUN_BYTES,
            });
        }
        self.used = used;
        Ok(())
    }
}

fn estimated_size(value: &Value) -> usize {
    let content = match value {
        Value::String(s) => s.len(),
        Value::Array(items) => items.iter().fold(0, |sum: usize, item| {
            sum.saturating_add(estimated_size(item))
        }),
        Value::Object(map) if !map.is_empty() => {
            map.iter()
                .fold(MAP_ALLOCATION_BYTES, |sum: usize, (key, item)| {
                    sum.saturating_add(ENTRY_OVERHEAD_BYTES)
                        .saturating_add(key.len())
                        .saturating_add(estimated_size(item))
                })
        }
        _ => 0,
    };
    content.saturating_add(size_of::<Value>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_first_entry_pays_for_the_map() {
        let mut first = RunBudget::default();
        let mut second = RunBudget::default();

        first.charge_entry(&json!({}), "k", &json!(1)).unwrap();
        second
            .charge_entry(&json!({ "a": 1 }), "k", &json!(1))
            .unwrap();

        assert_eq!(first.used - second.used, MAP_ALLOCATION_BYTES);
    }

    #[test]
    fn test_value_cost_includes_nested_content() {
        let mut budget = RunBudget::default();
        budget
            .charge_value(&json!({ "key": ["abcd", null] }))
            .unwrap();

        let values = 4 * size_of::<Value>();
        let expected = values + MAP_ALLOCATION_BYTES + ENTRY_OVERHEAD_BYTES + "key".len() + 4;
        assert_eq!(budget.used, expected);
    }

    #[test]
    fn test_charge_over_the_limit_is_an_error_and_keeps_the_total() {
        let mut budget = RunBudget {
            used: MAX_RUN_BYTES,
        };

        let result = budget.charge_value(&Value::Null);

        assert!(
            matches!(
                result,
                Err(Error::ModifierBudgetLimit {
                    max: MAX_RUN_BYTES,
                    ..
                })
            ),
            "got {result:?}"
        );
        assert_eq!(budget.used, MAX_RUN_BYTES);
    }
}
