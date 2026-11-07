use std::borrow::Cow;

use serde_json::Value;
use serde::Deserialize;

use crate::dsl::{Object, REntry, InfallibleLhs, Rhs, RhsEntry, IndexOp, RhsPart};
use crate::transform::Transform;
use crate::error::{json_kind, MissingKey};
use crate::modifier::budget::estimated_size;
use crate::{Error, Result};

const ROOT_KEY: &str = "root";

// Index expressions such as `[&0]` and `[@(1,idx)]` take their index from the input, and a write
// to index `i` pads the array with nulls up to `i`. Without a bound, one key like "10000000000"
// exhausts memory, and many moderate keys add up the same way. So each `shift` run gets a budget of
// gap slots: writing index `i` into an array of length `len` costs `i - len` when `i > len`, and
// nothing otherwise. A run may pad at most this many slots in total. Reaching the limit exactly is
// allowed; one slot more is an error. In-order writes (index == length) are free at any size.
const MAX_ARRAY_PADDING: usize = 100_000;

// A key built from the input through `&` or `@` is hashed, and usually copied, each time it is
// written or looked up. A value read through `@(n,...)` or `$n` with `n >= 1` comes from an
// ancestor, so it is copied once for every child under a wildcard. Either way one long string can
// cost children x length bytes of work or memory. So each `shift` run may spend at most this many
// estimated bytes on such keys and values. Reaching the limit exactly is allowed.
// Keys written in the spec, values matched in place, and `@`/`$` at level 0 are not charged:
// each input node is matched at most once per spec level, so their total is bounded by the input.
const MAX_COPIED_BYTES: usize = 64 * 1024 * 1024;

// Tracks what one `shift` run spends. See `MAX_ARRAY_PADDING` and `MAX_COPIED_BYTES`.
#[derive(Debug, Default)]
struct RunBudget {
    padded: usize,
    copied: usize,
}

impl RunBudget {
    // Reserve the gap slots needed to write `idx` into an array of length `len`.
    // Fails without reserving anything when the run would go over the limit.
    fn reserve_padding(&mut self, idx: usize, len: usize) -> Result<()> {
        let gap = idx.saturating_sub(len);
        let used = self.padded.saturating_add(gap);
        if used > MAX_ARRAY_PADDING {
            return Err(Error::ArrayPaddingLimit {
                idx,
                len,
                used: self.padded,
                max: MAX_ARRAY_PADDING,
            });
        }
        self.padded = used;
        Ok(())
    }

    // Charge `bytes` of copying or hashing. Call it before the work, so going over the limit
    // costs nothing. Fails without charging anything when the run would go over the limit.
    fn charge_copy(&mut self, bytes: usize) -> Result<()> {
        let used = self.copied.saturating_add(bytes);
        if used > MAX_COPIED_BYTES {
            return Err(Error::ShiftBudgetLimit {
                used,
                max: MAX_COPIED_BYTES,
            });
        }
        self.copied = used;
        Ok(())
    }

    // Charge using `key` as an object key. A key written wholly in the spec is free.
    fn charge_key(&mut self, key: &str, from_spec: bool) -> Result<()> {
        self.charge_key_len(key.len(), from_spec)
    }

    fn charge_key_len(&mut self, len: usize, from_spec: bool) -> Result<()> {
        if from_spec {
            return Ok(());
        }
        self.charge_copy(len)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Shift(Object);

impl Transform for Shift {
    fn apply(&self, val: &Value) -> Result<Value> {
        let mut path = vec![(vec![Cow::Borrowed(ROOT_KEY)], val)];
        let mut counts = vec![0];

        let mut budget = RunBudget::default();

        let mut out = Value::Null;
        apply(&self.0, &mut path, &mut counts, &mut budget, &mut out)?;

        path.pop().ok_or(Error::ShiftEmptyPath)?;
        // path should always be empty at this point
        // if not, the implementation is broken
        if !path.is_empty() {
            return Err(Error::ShiftPathNotEmpty);
        }

        Ok(out)
    }
}

// Apply an object from spec to the input
// input is passed using the path and the current input should be
// at the tip of the path.
// `counts` runs parallel to `path` and holds how many children each level has matched so far,
// which is what `[#n]` reads. As in Java JOLT, a level's count goes up after one of its children
// matches, and the special `#`, `$` and `@` keys do not count as matches.
fn apply<'ctx, 'input: 'ctx>(
    obj: &'input Object,
    path: &'ctx mut Vec<(Vec<Cow<'input, str>>, &'input Value)>,
    counts: &'ctx mut Vec<usize>,
    budget: &mut RunBudget,
    out: &'ctx mut Value,
) -> Result<()> {
    let tip = path.last().ok_or(Error::ShiftEmptyPath)?.clone();

    for (lhs, rhs) in obj.infallible.iter() {
        // The value is built once and moved into the last target, so it is copied once per target.
        // The lookup still runs for an empty target list, so a bad path is still an error.
        let writes = rhs.len();
        let v = match lhs {
            InfallibleLhs::DollarSign(idx0, idx1) => {
                let s = get_match((*idx0, *idx1), path)?;
                if writes == 0 {
                    continue;
                }
                if *idx0 > 0 {
                    budget.charge_copy(s.len().saturating_mul(writes))?;
                }
                Value::String(s.into())
            }
            InfallibleLhs::At(idx, at_rhs) => {
                let found = lookup_at((*idx, at_rhs), path, budget)?;
                if writes == 0 {
                    continue;
                }
                if *idx > 0 {
                    budget.charge_copy(estimated_size(found).saturating_mul(writes))?;
                }
                found.clone()
            }
            InfallibleLhs::Square(lit) => Value::String(lit.clone()),
        };
        let Some((last, rest)) = rhs.split_last() else {
            continue;
        };

        path.push(tip.clone());
        counts.push(0);
        for target in rest {
            insert_val_to_rhs(target, v.clone(), path, counts, budget, out)?;
        }
        insert_val_to_rhs(last, v, path, counts, budget, out)?;
        counts.pop();
        path.pop().ok_or(Error::ShiftEmptyPath)?;
    }

    match tip.1 {
        Value::Object(input) => {
            for (k, v) in input.iter() {
                match_obj_and_key(obj, path, counts, budget, Cow::Borrowed(k), v, out)?;
            }
        }
        Value::Bool(b) => {
            let k = if *b { "true" } else { "false" };

            match_obj_and_key(obj, path, counts, budget, Cow::Borrowed(k), tip.1, out)?;
        }
        Value::Array(arr) => {
            for (k, v) in arr.iter().enumerate() {
                let k = k.to_string();
                match_obj_and_key(
                    obj,
                    path,
                    counts,
                    budget,
                    // this makes the downstream functions to do some extra allocations.
                    // could avoid some of these allocations by mapping some small indexes to static str's
                    Cow::Owned(k),
                    v,
                    out,
                )?;
            }
        }
        Value::Number(n) => {
            let k = n.to_string();

            match_obj_and_key(obj, path, counts, budget, Cow::Owned(k), tip.1, out)?;
        }
        Value::String(k) => {
            match_obj_and_key(obj, path, counts, budget, Cow::Borrowed(k), tip.1, out)?;
        }
        Value::Null => {
            let k = "null";
            match_obj_and_key(obj, path, counts, budget, Cow::Borrowed(k), tip.1, out)?;
        }
    };

    Ok(())
}

// Match and object in the spec with a key/value pair from the input
// This function only runs the k/v pairs that have a fallible lhs in the spec
// The infallible ones should have ran beforehand
fn match_obj_and_key<'ctx, 'input: 'ctx>(
    obj: &'input Object,
    path: &'ctx mut Vec<(Vec<Cow<'input, str>>, &'input Value)>,
    counts: &'ctx mut Vec<usize>,
    budget: &mut RunBudget,
    k: Cow<'input, str>,
    v: &'input Value,
    out: &'ctx mut Value,
) -> Result<()> {
    for (lit, rhs) in obj.literal.iter() {
        let lit = Cow::Borrowed(lit.as_ref());
        if lit == k {
            path.push((vec![lit], v));
            counts.push(0);
            apply_match(v, rhs, path, counts, budget, out)?;
            pop_matched_level(path, counts)?;
            return Ok(());
        }
    }

    for (amp, rhs) in obj.amp.iter() {
        let m = get_match(*amp, path)?;
        if m == k {
            path.push((vec![m], v));
            counts.push(0);
            apply_match(v, rhs, path, counts, budget, out)?;
            pop_matched_level(path, counts)?;
            return Ok(());
        }
    }

    for (pipes, rhs) in obj.pipes.iter() {
        for stars in pipes.iter() {
            if let Some(m) = match_stars(&stars.0, Cow::clone(&k)) {
                path.push((m, v));
                counts.push(0);
                apply_match(v, rhs, path, counts, budget, out)?;
                pop_matched_level(path, counts)?;
                return Ok(());
            }
        }
    }

    Ok(())
}

// Leave a level that matched, and count the match against its parent.
fn pop_matched_level(
    path: &mut Vec<(Vec<Cow<'_, str>>, &Value)>,
    counts: &mut Vec<usize>,
) -> Result<()> {
    path.pop().ok_or(Error::ShiftEmptyPath)?;
    counts.pop().ok_or(Error::ShiftEmptyPath)?;
    *counts.last_mut().ok_or(Error::ShiftEmptyPath)? += 1;
    Ok(())
}

fn apply_match<'ctx, 'input: 'ctx>(
    v: &'input Value,
    rhs: &'input REntry,
    path: &'ctx mut Vec<(Vec<Cow<'input, str>>, &'input Value)>,
    counts: &'ctx mut Vec<usize>,
    budget: &mut RunBudget,
    out: &'ctx mut Value,
) -> Result<()> {
    match rhs {
        REntry::Obj(object) => apply(object, path, counts, budget, out),
        REntry::Rhs(rhs) => {
            for rhs in rhs.iter() {
                insert_val_to_rhs(rhs, v.clone(), path, counts, budget, out)?;
            }
            Ok(())
        }
        REntry::Thrash => Ok(()),
    }
}

// Find the value an @ expression points at, without copying it. Index expressions use this
// so they can check a string's length before anything copies it.
fn lookup_at<'v>(
    at: (usize, &Rhs),
    path: &[(Vec<Cow<'_, str>>, &'v Value)],
    budget: &mut RunBudget,
) -> Result<&'v Value> {
    if at.0 >= path.len() {
        return Err(Error::PathIndexOutOfRange {
            idx: at.0,
            len: path.len(),
        });
    }

    let v = path[path.len() - at.0 - 1].1;

    lookup_rhs(at.1, v, path, budget)
}

// Find the value a rhs expression points at inside `v`, using the given path
fn lookup_rhs<'v>(
    rhs: &Rhs,
    v: &'v Value,
    path: &[(Vec<Cow<'_, str>>, &Value)],
    budget: &mut RunBudget,
) -> Result<&'v Value> {
    let mut v = v;

    for part in rhs.0.iter() {
        match part {
            RhsPart::Index(idx_op) => match v {
                Value::Array(a) => {
                    let idx = match idx_op {
                        IndexOp::Amp(idx0, idx1) => parse_index(&get_match((*idx0, *idx1), path)?)?,
                        IndexOp::Literal(idx) => *idx,
                        IndexOp::At(idx, rhs) => {
                            index_from_value(lookup_at((*idx, rhs), path, budget)?)?
                        }
                        // `#` only counts output matches, so it has no meaning in a lookup
                        IndexOp::Hash(_) | IndexOp::Empty => {
                            return Err(Error::UnexpectedRhsEntry);
                        }
                    };

                    v = a
                        .get(idx)
                        .ok_or(Error::ArrIndexOutOfRange { idx, len: a.len() })?;
                }
                _ => {
                    return Err(Error::UnexpectedRhsEntry);
                }
            },
            RhsPart::CompositeKey(entries) => {
                let (key, from_spec) = composite_key(entries, path, budget)?;
                v = key_into_object(v, &key, from_spec)?;
            }
            RhsPart::Key(entry) => {
                let cow = rhs_entry_to_cow(entry, path, budget)?;
                let from_spec = matches!(entry, RhsEntry::Key(_));
                budget.charge_key(cow.as_ref(), from_spec)?;
                v = key_into_object(v, cow.as_ref(), from_spec)?;
            }
        }
    }

    Ok(v)
}

// usize::MAX has 20 decimal digits. A longer index string can only be valid with leading zeros,
// and parsing it costs time in proportion to its length. An ancestor index such as `[&1]` is
// parsed again for every child, so a 1 MB key could cost many seconds of CPU.
const MAX_INDEX_LEN: usize = 20;

// Parse an array index taken from a match or an input value.
// Rejects over-length strings before parsing. No error holds the string itself, because it comes
// from the request body.
fn parse_index(raw: &str) -> Result<usize> {
    if raw.len() > MAX_INDEX_LEN {
        return Err(Error::IndexTooLong {
            len: raw.len(),
            max: MAX_INDEX_LEN,
        });
    }

    raw.parse().map_err(|e| Error::InvalidIndex {
        len: raw.len(),
        source: e,
    })
}

// Turn the value an `[@(...)]` expression points at into an array index.
// The error names only the kind of value, because the value comes from the request body.
fn index_from_value(v: &Value) -> Result<usize> {
    match v {
        Value::Number(n) => {
            n.as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or(Error::InvalidIndexVal {
                    kind: number_kind(n),
                })
        }
        Value::String(s) => parse_index(s),
        v => Err(Error::InvalidIndexVal { kind: json_kind(v) }),
    }
}

// Say why a number is not a valid array index, without printing it.
// Without serde_json's `arbitrary_precision` feature, an integer outside the u64 and i64 ranges is
// parsed as an f64, so the float value decides the label.
fn number_kind(n: &serde_json::Number) -> &'static str {
    // 2^64, the first value above u64::MAX.
    const U64_LIMIT: f64 = 18_446_744_073_709_551_616.0;

    if n.is_u64() {
        // Fits u64 but not usize, which only happens on targets narrower than 64 bits.
        return "number (too large)";
    }

    match n.as_f64() {
        Some(f) if f < 0.0 => "number (negative)",
        Some(f) if f >= U64_LIMIT => "number (too large)",
        _ => "number (not an integer)",
    }
}

// Evaluate a rhs expression into a string. A string read through `@` is borrowed from the input,
// because it may be long and this runs once per write.
fn rhs_entry_to_cow<'ctx, 'input: 'ctx>(
    entry: &'input RhsEntry,
    path: &'ctx [(Vec<Cow<'input, str>>, &'input Value)],
    budget: &mut RunBudget,
) -> Result<Cow<'input, str>> {
    let cow = match entry {
        RhsEntry::Amp(idx0, idx1) => get_match((*idx0, *idx1), path)?,
        RhsEntry::At(idx, rhs) => {
            let key = lookup_at((*idx, rhs), path, budget)?;
            match key {
                Value::String(s) => Cow::Borrowed(s.as_str()),
                Value::Number(n) => Cow::Owned(n.to_string()),
                Value::Bool(b) => {
                    if *b {
                        Cow::Borrowed("true")
                    } else {
                        Cow::Borrowed("false")
                    }
                }
                _ => return Err(Error::EvalString),
            }
        }
        RhsEntry::Key(key) => Cow::Borrowed(key.as_str()),
    };

    Ok(cow)
}

// Build a composite key such as `pre&1` and charge it to the budget before joining its parts.
// Returns the key and whether every part was written in the spec.
fn composite_key<'ctx, 'input: 'ctx>(
    entries: &'input [RhsEntry],
    path: &'ctx [(Vec<Cow<'input, str>>, &'input Value)],
    budget: &mut RunBudget,
) -> Result<(String, bool)> {
    let parts = entries
        .iter()
        .map(|entry| rhs_entry_to_cow(entry, path, budget))
        .collect::<Result<Vec<_>>>()?;
    let len = parts
        .iter()
        .fold(0, |sum: usize, part| sum.saturating_add(part.len()));

    let from_spec = entries.iter().all(|e| matches!(e, RhsEntry::Key(_)));
    // The joined key is hashed and copied once per write, whatever its parts cost to find.
    budget.charge_key_len(len, from_spec)?;

    Ok((parts.concat(), from_spec))
}

// Index into an object using a given key, and fail if the key is absent.
// `from_spec` says whether every part of the key was written in the spec. A key built from the
// request body is left out of the error, because callers log it.
fn key_into_object<'input>(v: &'input Value, key: &str, from_spec: bool) -> Result<&'input Value> {
    let obj = v.as_object().ok_or(Error::UnexpectedRhsEntry)?;

    match obj.get(key) {
        Some(v) => Ok(v),
        None if from_spec => Err(Error::KeyNotFound(MissingKey::Spec(key.to_owned()))),
        None => Err(Error::KeyNotFound(MissingKey::FromInput { len: key.len() })),
    }
}

fn insert_val_to_rhs<'ctx, 'input: 'ctx>(
    rhs: &Rhs,
    v: Value,
    path: &'ctx [(Vec<Cow<'input, str>>, &'input Value)],
    counts: &[usize],
    budget: &mut RunBudget,
    out: &mut Value,
) -> Result<()> {
    let mut out = out;

    for part in rhs.0.iter() {
        match part {
            RhsPart::Index(idx_op) => {
                let arr = if out.is_array() {
                    out.as_array_mut().unwrap()
                } else if out.is_null() {
                    *out = Value::Array(Vec::new());
                    out.as_array_mut().unwrap()
                } else {
                    *out = Value::Array(vec![std::mem::take(out)]);
                    out.as_array_mut().unwrap()
                };

                let idx = match idx_op {
                    IndexOp::Amp(idx0, idx1) => parse_index(&get_match((*idx0, *idx1), path)?)?,
                    IndexOp::Literal(idx) => *idx,
                    IndexOp::At(idx, rhs) => {
                        index_from_value(lookup_at((*idx, rhs), path, budget)?)?
                    }
                    IndexOp::Hash(level) => get_match_count(*level, counts)?,
                    IndexOp::Empty => {
                        arr.push(Value::Null);
                        out = arr.last_mut().unwrap();
                        continue;
                    }
                };

                // Charge before pushing, so an index over the budget allocates nothing.
                budget.reserve_padding(idx, arr.len())?;
                while arr.len() <= idx {
                    arr.push(Value::Null);
                }

                out = arr.get_mut(idx).unwrap();
            }
            RhsPart::CompositeKey(entries) => {
                let (key, _) = composite_key(entries, path, budget)?;

                let obj = if out.is_object() {
                    out.as_object_mut().unwrap()
                } else {
                    *out = Value::Object(Default::default());
                    out.as_object_mut().unwrap()
                };

                out = obj.entry(key).or_insert(Value::Null);
            }
            RhsPart::Key(entry) => {
                let cow = rhs_entry_to_cow(entry, path, budget)?;
                budget.charge_key(cow.as_ref(), matches!(entry, RhsEntry::Key(_)))?;
                let obj = if out.is_object() {
                    out.as_object_mut().unwrap()
                } else {
                    *out = Value::Object(Default::default());
                    out.as_object_mut().unwrap()
                };

                out = obj.entry(cow.as_ref()).or_insert(Value::Null);
            }
        }
    }

    match out {
        Value::Null => {
            *out = v;
        }
        Value::Array(arr) => {
            arr.push(v);
        }
        val => {
            let v = Value::Array(vec![std::mem::take(val), v]);
            *val = v;
        }
    }

    Ok(())
}

fn match_stars<'ctx, 'input: 'ctx>(
    stars: &'input [String],
    k: Cow<'input, str>,
) -> Option<Vec<Cow<'input, str>>> {
    match stars.len() {
        0 => {
            return if k.is_empty() {
                Some(vec!["".into()])
            } else {
                None
            };
        }
        1 => {
            return if k == stars[0].as_str() {
                Some(vec![k])
            } else {
                None
            };
        }
        _ => (),
    }

    let mut m = vec![k.clone()];

    let prefix = stars[0].as_str();

    let mut k = if prefix.is_empty() {
        k
    } else {
        match k {
            Cow::Borrowed(s) => {
                let s = s.strip_prefix(prefix)?;
                Cow::Borrowed(s)
            }
            Cow::Owned(s) => {
                let s = s.strip_prefix(prefix)?;
                Cow::Owned(s.to_owned())
            }
        }
    };

    for pattern in stars.iter().skip(1) {
        if !pattern.is_empty() {
            {
                let idx = k.find(pattern.as_str())?;
                match &k {
                    Cow::Borrowed(s) => {
                        m.push(Cow::Borrowed(&s[..idx]));
                        k = Cow::Borrowed(&s[idx..]);
                    }
                    Cow::Owned(s) => {
                        m.push(Cow::Owned(s[..idx].to_owned()));
                        k = Cow::Owned(s[idx..].to_owned());
                    }
                }
            }
        } else {
            m.push(k.clone());
        }
    }

    Some(m)
}

fn get_match<'ctx, 'input: 'ctx>(
    idx: (usize, usize),
    path: &'ctx [(Vec<Cow<'input, str>>, &'input Value)],
) -> Result<Cow<'input, str>> {
    if idx.0 >= path.len() {
        return Err(Error::PathIndexOutOfRange {
            idx: idx.0,
            len: path.len(),
        });
    }

    let (matches, _) = &path[path.len() - idx.0 - 1];

    let m = matches.get(idx.1).ok_or(Error::MatchIndexOutOfRange {
        idx: idx.1,
        len: matches.len(),
    })?;

    Ok(m.clone())
}

// Read the match count of the spec level `level` steps up from the current leaf
fn get_match_count(level: usize, counts: &[usize]) -> Result<usize> {
    if level >= counts.len() {
        return Err(Error::PathIndexOutOfRange {
            idx: level,
            len: counts.len(),
        });
    }

    Ok(counts[counts.len() - level - 1])
}

#[cfg(test)]
mod test {
    use serde_json::{json, Value};
    use super::*;

    fn run(spec: Value, input: Value) -> Result<Value> {
        let shift: Shift = serde_json::from_value(spec).expect("parsed spec");
        shift.apply(&input)
    }

    // Errors from these lookups are logged by callers, so they must not repeat body data.
    fn assert_no_secret(err: &Error) {
        assert!(
            !format!("{err}").contains("PHI-SECRET"),
            "Display leaked: {err}"
        );
        assert!(
            !format!("{err:?}").contains("PHI-SECRET"),
            "Debug leaked: {err:?}"
        );
    }

    #[test]
    fn test_invalid_index_value_names_only_the_object_kind() {
        let err = run(
            json!({ "a": "out[@(1,x)]" }),
            json!({ "a": 1, "x": { "secret": "PHI-SECRET" } }),
        )
        .unwrap_err();

        assert!(
            matches!(err, Error::InvalidIndexVal { kind: "object" }),
            "got {:?}",
            err
        );
        assert_no_secret(&err);
    }

    #[test]
    fn test_invalid_index_value_names_each_kind() {
        let cases = [
            (json!(["PHI-SECRET"]), "array"),
            (json!(true), "boolean"),
            (json!(null), "null"),
            (json!(-3), "number (negative)"),
            (json!(1.5), "number (not an integer)"),
            (json!(1e20), "number (too large)"),
            (json!(-1e20), "number (negative)"),
        ];

        for (x, expected) in cases {
            let err = run(json!({ "a": "out[@(1,x)]" }), json!({ "a": 1, "x": x })).unwrap_err();

            assert!(
                matches!(err, Error::InvalidIndexVal { kind } if kind == expected),
                "x={x}: got {err:?}"
            );
            assert_no_secret(&err);
        }
    }

    #[test]
    fn test_missing_key_built_from_input_reports_only_its_length() {
        let err = run(
            json!({ "*": { "v": "out[@(2,lookup.&1)]" } }),
            json!({ "PHI-SECRET": { "v": 1 }, "lookup": {} }),
        )
        .unwrap_err();

        assert!(
            matches!(err, Error::KeyNotFound(MissingKey::FromInput { len: 10 })),
            "got {:?}",
            err
        );
        assert_no_secret(&err);
    }

    #[test]
    fn test_missing_key_with_any_input_part_reports_only_its_length() {
        let cases = [
            // A composite key that mixes spec text with a matched input key.
            (
                json!({ "*": { "v": "out[@(2,lookup.pre&1)]" } }),
                json!({ "PHI-SECRET": { "v": 1 }, "lookup": {} }),
                13,
            ),
            // A key read from an input value.
            (
                json!({ "a": "out[@(1,lookup.@(1,k))]" }),
                json!({ "a": 1, "k": "PHI-SECRET", "lookup": {} }),
                10,
            ),
        ];

        for (spec, input, expected) in cases {
            let err = run(spec.clone(), input).unwrap_err();

            assert!(
                matches!(err, Error::KeyNotFound(MissingKey::FromInput { len }) if len == expected),
                "spec={spec}: got {err:?}"
            );
            assert_no_secret(&err);
        }
    }

    #[test]
    fn test_invalid_index_string_reports_only_its_length() {
        let cases = [
            (
                json!({ "a": "out[@(1,x)]" }),
                json!({ "a": 1, "x": "PHI-SECRET" }),
            ),
            (json!({ "*": "out[&0]" }), json!({ "PHI-SECRET": 1 })),
        ];

        for (spec, input) in cases {
            let err = run(spec.clone(), input).unwrap_err();

            assert!(
                matches!(err, Error::InvalidIndex { len: 10, .. }),
                "spec={spec}: got {err:?}"
            );
            assert_no_secret(&err);
        }
    }

    #[test]
    fn test_missing_key_written_in_spec_is_named() {
        let err = run(
            json!({ "a": "out[@(1,lookup.missing)]" }),
            json!({ "a": 1, "lookup": {} }),
        )
        .unwrap_err();

        assert!(
            matches!(&err, Error::KeyNotFound(MissingKey::Spec(key)) if key == "missing"),
            "got {:?}",
            err
        );
        assert_eq!(err.to_string(), "Key not found in object:missing");
    }

    #[test]
    fn test_hash_index_on_leaf_counts_matched_siblings() {
        let output = run(json!({ "*": "out[#1]" }), json!({ "a": 1, "b": 2 })).unwrap();

        assert_eq!(output, json!({ "out": [1, 2] }));
    }

    #[test]
    fn test_hash_index_ignores_special_keys() {
        let output = run(
            json!({
                "a": {
                    "#lit": "o.l",
                    "@(1,z)": "o.z",
                    "$": "o.d",
                    "b": "out[#1]",
                    "c": "out[#1]"
                }
            }),
            json!({ "a": { "b": 1, "c": 2 }, "z": "zz" }),
        )
        .unwrap();

        assert_eq!(output["out"], json!([1, 2]));
        assert_eq!(output["o"], json!({ "l": "lit", "z": "zz", "d": "a" }));
    }

    #[test]
    fn test_hash_index_follows_input_order_and_skips_unmatched() {
        let output = run(
            json!({ "x*": { "v": "out[#2]" } }),
            json!({ "x2": { "v": "two" }, "y": { "v": "skip" }, "x1": { "v": "one" } }),
        )
        .unwrap();

        assert_eq!(output, json!({ "out": ["two", "one"] }));
    }

    #[test]
    fn test_hash_index_over_input_array() {
        let output = run(
            json!({ "items": { "*": { "id": "ids[#2]" } } }),
            json!({ "items": [{ "id": 10 }, { "id": 20 }, { "id": 30 }] }),
        )
        .unwrap();

        assert_eq!(output, json!({ "ids": [10, 20, 30] }));
    }

    #[test]
    fn test_hash_index_out_of_range_is_an_error() {
        let result = run(json!({ "a": "out[#5]" }), json!({ "a": 1 }));

        assert!(
            matches!(result, Err(Error::PathIndexOutOfRange { idx: 5, .. })),
            "got {:?}",
            result
        );
    }

    #[test]
    fn test_hash_index_in_lookup_is_rejected() {
        let result = run(
            json!({ "a": { "@(1,arr[#1])": "out" } }),
            json!({ "a": 1, "arr": [1] }),
        );

        assert!(result.is_err(), "got {:?}", result);
    }

    fn sparse_objects(count: usize, key: &str) -> Value {
        let mut input = serde_json::Map::new();
        for i in 0..count {
            input.insert(format!("o{}", i), json!({ key: i }));
        }
        Value::Object(input)
    }

    #[test]
    fn test_amp_index_padding_limit_boundary() {
        let at_limit = MAX_ARRAY_PADDING.to_string();
        let output = run(json!({ "*": "a[&0]" }), json!({ at_limit: "x" })).unwrap();
        let arr = output["a"].as_array().unwrap();
        assert_eq!(arr.len(), MAX_ARRAY_PADDING + 1);
        assert_eq!(arr[MAX_ARRAY_PADDING], json!("x"));

        let over_limit = (MAX_ARRAY_PADDING + 1).to_string();
        let result = run(json!({ "*": "a[&0]" }), json!({ over_limit: "x" }));
        assert!(
            matches!(result, Err(Error::ArrayPaddingLimit { idx, len: 0, used: 0, max: MAX_ARRAY_PADDING }) if idx == MAX_ARRAY_PADDING + 1),
            "got {:?}",
            result
        );
    }

    #[test]
    fn test_at_index_over_padding_limit_is_an_error() {
        let spec = json!({ "v": "out[@(1,idx)]" });

        let from_number = run(
            spec.clone(),
            json!({ "v": "x", "idx": MAX_ARRAY_PADDING + 1 }),
        );
        assert!(
            matches!(from_number, Err(Error::ArrayPaddingLimit { .. })),
            "got {:?}",
            from_number
        );

        let from_string = run(spec.clone(), json!({ "v": "x", "idx": "200000" }));
        assert!(
            matches!(from_string, Err(Error::ArrayPaddingLimit { .. })),
            "got {:?}",
            from_string
        );

        let output = run(spec, json!({ "v": "x", "idx": 3 })).unwrap();
        assert_eq!(output, json!({ "out": [null, null, null, "x"] }));
    }

    #[test]
    fn test_huge_amp_index_fails_without_allocating() {
        let result = run(json!({ "*": "a[&0]" }), json!({ "10000000000": "x" }));

        assert!(
            matches!(
                result,
                Err(Error::ArrayPaddingLimit {
                    idx: 10_000_000_000,
                    len: 0,
                    ..
                })
            ),
            "got {:?}",
            result
        );
    }

    #[test]
    fn test_padding_across_many_arrays_is_bounded() {
        let result = run(
            json!({ "*": { "*": "&1[&0]" } }),
            sparse_objects(1_000, "99999"),
        );

        assert!(
            matches!(result, Err(Error::ArrayPaddingLimit { .. })),
            "got {:?}",
            result
        );
    }

    #[test]
    fn test_positional_reshaping_costs_no_padding() {
        let items: Vec<Value> = (0..200_000).map(|i| json!({ "qty": i })).collect();
        let output = run(
            json!({ "items": { "*": { "qty": "data.items[&1].qty" } } }),
            json!({ "items": items }),
        )
        .unwrap();

        let out_items = output["data"]["items"].as_array().unwrap();
        assert_eq!(out_items.len(), 200_000);
        assert_eq!(out_items[199_999], json!({ "qty": 199_999 }));
    }

    #[test]
    fn test_small_sparse_writes_share_one_budget() {
        let spec = json!({ "*": { "*": "&1[&0]" } });

        // Each object pads 1,000 slots, so 100 objects use the whole budget.
        let output = run(spec.clone(), sparse_objects(100, "1000")).unwrap();
        assert_eq!(output["o99"].as_array().unwrap().len(), 1_001);

        let result = run(spec, sparse_objects(101, "1000"));
        assert!(
            matches!(result, Err(Error::ArrayPaddingLimit { .. })),
            "got {:?}",
            result
        );
    }

    #[test]
    fn test_padding_budget_resets_for_each_run() {
        let shift: Shift = serde_json::from_value(json!({ "*": "a[&0]" })).expect("parsed spec");
        let input = json!({ MAX_ARRAY_PADDING.to_string(): "x" });

        assert!(shift.apply(&input).is_ok());
        assert!(shift.apply(&input).is_ok());
    }

    #[test]
    fn test_padding_error_reports_budget_already_used() {
        let result = run(
            json!({ "*": { "*": "&1[&0]" } }),
            json!({ "a": { "60000": 1 }, "b": { "60000": 1 } }),
        );

        assert!(
            matches!(
                result,
                Err(Error::ArrayPaddingLimit {
                    idx: 60_000,
                    len: 0,
                    used: 60_000,
                    ..
                })
            ),
            "got {:?}",
            result
        );
    }

    #[test]
    fn test_literal_index_is_charged_against_the_budget() {
        let result = run(json!({ "a": "out[200000]" }), json!({ "a": 1 }));

        assert!(
            matches!(result, Err(Error::ArrayPaddingLimit { idx: 200_000, .. })),
            "got {:?}",
            result
        );
    }

    #[test]
    fn test_nested_arrays_share_one_budget() {
        let spec = json!({ "*": { "*": "a[&1][&0]" } });

        // 50,000 + 49,999 gap slots fits; 50,000 + 50,001 does not.
        let output = run(spec.clone(), json!({ "50000": { "49999": 1 } })).unwrap();
        assert_eq!(output["a"][50_000][49_999], json!(1));

        let result = run(spec, json!({ "50000": { "50001": 1 } }));
        assert!(
            matches!(result, Err(Error::ArrayPaddingLimit { .. })),
            "got {:?}",
            result
        );
    }

    fn long_index(len: usize) -> String {
        "0".repeat(len - 1) + "1"
    }

    #[test]
    fn test_amp_index_string_over_max_len_is_an_error() {
        let key = long_index(MAX_INDEX_LEN + 1);
        let result = run(json!({ "*": "a[&0]" }), json!({ key: "x" }));

        assert!(
            matches!(result, Err(Error::IndexTooLong { len, max: MAX_INDEX_LEN }) if len == MAX_INDEX_LEN + 1),
            "got {:?}",
            result
        );
    }

    #[test]
    fn test_at_index_string_over_max_len_is_an_error() {
        let result = run(
            json!({ "v": "out[@(1,idx)]" }),
            json!({ "v": "x", "idx": long_index(MAX_INDEX_LEN + 1) }),
        );

        assert!(
            matches!(result, Err(Error::IndexTooLong { .. })),
            "got {:?}",
            result
        );
    }

    #[test]
    fn test_index_string_at_max_len_still_parses() {
        let idx = "00000000000000000005";
        assert_eq!(idx.len(), MAX_INDEX_LEN);

        let from_amp = run(json!({ "*": "a[&0]" }), json!({ idx: "x" })).unwrap();
        assert_eq!(
            from_amp,
            json!({ "a": [null, null, null, null, null, "x"] })
        );

        let from_at = run(
            json!({ "v": "out[@(1,idx)]" }),
            json!({ "v": "x", "idx": idx }),
        )
        .unwrap();
        assert_eq!(
            from_at,
            json!({ "out": [null, null, null, null, null, "x"] })
        );
    }

    #[test]
    fn test_lookup_index_string_over_max_len_is_an_error() {
        let at_spec = json!({ "a": { "@(1,arr[@(1,i)])": "out" } });
        let output = run(
            at_spec.clone(),
            json!({ "a": {}, "arr": [10, 20], "i": "1" }),
        )
        .unwrap();
        assert_eq!(output, json!({ "out": 20 }));

        let from_at = run(
            at_spec,
            json!({ "a": {}, "arr": [10, 20], "i": long_index(MAX_INDEX_LEN + 1) }),
        );
        assert!(
            matches!(from_at, Err(Error::IndexTooLong { .. })),
            "got {:?}",
            from_at
        );

        let amp_spec = json!({ "k*": { "@(1,arr[&(0,1)])": "out" } });
        let output = run(amp_spec.clone(), json!({ "k1": {}, "arr": [10, 20] })).unwrap();
        assert_eq!(output, json!({ "out": 20 }));

        let key = format!("k{}", long_index(MAX_INDEX_LEN + 1));
        let from_amp = run(amp_spec, json!({ key: {}, "arr": [10, 20] }));
        assert!(
            matches!(from_amp, Err(Error::IndexTooLong { .. })),
            "got {:?}",
            from_amp
        );
    }

    #[test]
    fn test_long_ancestor_index_string_fails_fast() {
        let children: serde_json::Map<String, Value> =
            (0..10_000).map(|i| (i.to_string(), json!(i))).collect();
        let result = run(
            json!({ "*": { "*": "out[&1]" } }),
            json!({ long_index(100_000): children }),
        );
        assert!(
            matches!(result, Err(Error::IndexTooLong { len: 100_000, .. })),
            "got {:?}",
            result
        );

        let items: Vec<Value> = (0..10_000).map(|i| json!({ "v": i })).collect();
        let result = run(
            json!({ "items": { "*": { "v": "out[@(3,idx)]" } } }),
            json!({ "items": items, "idx": long_index(100_000) }),
        );
        assert!(
            matches!(result, Err(Error::IndexTooLong { len: 100_000, .. })),
            "got {:?}",
            result
        );
    }

    const MIB: usize = 1024 * 1024;

    // How many 1 MiB copies fit in the budget exactly.
    const MIB_COPIES: usize = MAX_COPIED_BYTES / MIB;

    fn numbered_children(count: usize) -> Value {
        Value::Object((0..count).map(|i| (i.to_string(), json!(i))).collect())
    }

    fn numbered_items(count: usize) -> Value {
        Value::Array((0..count).map(|i| json!({ "v": i })).collect())
    }

    fn assert_copy_limit(result: &Result<Value>) {
        assert!(
            matches!(
                result,
                Err(Error::ShiftBudgetLimit {
                    max: MAX_COPIED_BYTES,
                    ..
                })
            ),
            "got {:?}",
            result.as_ref().map(|_| "ok")
        );
    }

    #[test]
    fn test_ancestor_key_copies_are_bounded() {
        let spec = json!({ "*": { "*": "&1[&0]" } });
        let key = "k".repeat(MIB);

        let output = run(
            spec.clone(),
            json!({ key.clone(): numbered_children(MIB_COPIES) }),
        )
        .unwrap();
        assert_eq!(output[&key].as_array().unwrap().len(), MIB_COPIES);

        let result = run(spec, json!({ key: numbered_children(MIB_COPIES + 1) }));
        assert_copy_limit(&result);
    }

    #[test]
    fn test_composite_ancestor_key_copies_are_bounded() {
        let key = "k".repeat(MIB);
        let result = run(
            json!({ "*": { "*": "pre&1" } }),
            json!({ key: numbered_children(MIB_COPIES + 1) }),
        );

        assert_copy_limit(&result);
    }

    #[test]
    fn test_at_key_copies_are_bounded() {
        let spec = json!({ "items": { "*": { "v": "out.@(3,idx)" } } });
        let idx = "k".repeat(MIB);

        let output = run(
            spec.clone(),
            json!({ "items": numbered_items(MIB_COPIES), "idx": idx.clone() }),
        )
        .unwrap();
        assert_eq!(output["out"][&idx].as_array().unwrap().len(), MIB_COPIES);

        let result = run(
            spec,
            json!({ "items": numbered_items(MIB_COPIES + 1), "idx": idx }),
        );
        assert_copy_limit(&result);
    }

    #[test]
    fn test_lookup_keys_from_input_are_charged() {
        let key = "k".repeat(MIB);
        let result = run(
            json!({ "data": { "*": { "*": { "@(3,lookup.&1)": "out[]" } } } }),
            json!({
                "data": { key.clone(): numbered_children(MIB_COPIES + 1) },
                "lookup": { key: 1 }
            }),
        );

        assert_copy_limit(&result);
    }

    #[test]
    fn test_ancestor_at_value_copies_are_bounded() {
        let spec = json!({ "items": { "*": { "@(2,big)": "out[]" } } });
        let big = "v".repeat(MIB);

        // Each copy is estimated a little over 1 MiB, so one fewer copy fits.
        let output = run(
            spec.clone(),
            json!({ "items": numbered_items(MIB_COPIES - 1), "big": big.clone() }),
        )
        .unwrap();
        assert_eq!(output["out"].as_array().unwrap().len(), MIB_COPIES - 1);

        let result = run(
            spec,
            json!({ "items": numbered_items(MIB_COPIES), "big": big }),
        );
        assert_copy_limit(&result);
    }

    #[test]
    fn test_ancestor_dollar_value_copies_are_bounded() {
        let spec = json!({ "*": { "*": { "$1": "out[]" } } });
        let key = "k".repeat(MIB);

        let output = run(
            spec.clone(),
            json!({ key.clone(): numbered_children(MIB_COPIES) }),
        )
        .unwrap();
        assert_eq!(output["out"].as_array().unwrap().len(), MIB_COPIES);

        let result = run(spec, json!({ key: numbered_children(MIB_COPIES + 1) }));
        assert_copy_limit(&result);
    }

    #[test]
    fn test_level_zero_at_and_dollar_values_are_free() {
        let big = "v".repeat(MAX_COPIED_BYTES + 1);

        let output = run(json!({ "@": "data" }), json!({ "a": big.clone() })).unwrap();
        assert_eq!(output["data"]["a"].as_str().map(str::len), Some(big.len()));

        let output = run(json!({ "*": { "$": "keys[]" } }), json!({ big: 1 })).unwrap();
        assert_eq!(
            output["keys"][0].as_str().map(str::len),
            Some(MAX_COPIED_BYTES + 1)
        );
    }

    #[test]
    fn test_copy_budget_resets_for_each_run() {
        let shift: Shift =
            serde_json::from_value(json!({ "*": { "*": "&1[&0]" } })).expect("parsed spec");
        let input = json!({ "k".repeat(MIB): numbered_children(MIB_COPIES) });

        assert!(shift.apply(&input).is_ok());
        assert!(shift.apply(&input).is_ok());
    }

    #[test]
    fn test_copy_limit_error_holds_no_body_data() {
        let key = format!("PHI-SECRET{}", "k".repeat(MIB));
        let err = run(
            json!({ "*": { "*": "&1[&0]" } }),
            json!({ key: numbered_children(MIB_COPIES + 1) }),
        )
        .unwrap_err();

        assert_no_secret(&err);
    }

    #[test]
    fn test_empty_target_list_writes_nothing_and_still_checks_the_path() {
        let big = "v".repeat(MIB);
        let output = run(
            json!({ "items": { "*": { "@(2,big)": [], "$1": [] } } }),
            json!({ "items": numbered_items(MIB_COPIES + 1), "big": big }),
        )
        .unwrap();
        assert_eq!(output, Value::Null);

        let result = run(json!({ "a": { "@(1,missing)": [] } }), json!({ "a": {} }));
        assert!(
            matches!(&result, Err(Error::KeyNotFound(MissingKey::Spec(key))) if key == "missing"),
            "got {:?}",
            result
        );
    }

    #[test]
    fn test_ancestor_value_with_several_targets_is_charged_per_target() {
        let spec = json!({ "items": { "*": { "@(2,big)": ["a[]", "b[]"] } } });
        let big = "v".repeat(MIB);
        let pairs = (MIB_COPIES - 1) / 2;

        let output = run(
            spec.clone(),
            json!({ "items": numbered_items(pairs), "big": big.clone() }),
        )
        .unwrap();
        assert_eq!(output["a"].as_array().unwrap().len(), pairs);
        assert_eq!(output["b"].as_array().unwrap().len(), pairs);

        let result = run(
            spec,
            json!({ "items": numbered_items(pairs + 1), "big": big }),
        );
        assert_copy_limit(&result);
    }
}
