<h1 align="center">Harmony Jolt JSON library</h1>
<div align="center">
 <strong>
    JSON to JSON transformation Rust library
 </strong>
</div>

<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/harmony-jolt">
    <img src="https://img.shields.io/crates/v/harmony-jolt?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/harmony-jolt">
    <img src="https://img.shields.io/crates/d/harmony-jolt.svg?style=flat-square"
      alt="Download" />
  </a>
  <!-- docs.rs docs -->

[//]: # (  <a href="https://docs.rs/harmony-jolt">)

[//]: # (    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square")

[//]: # (      alt="docs.rs docs" />)

[//]: # (  </a>)
</div>

## Overview
JSON to JSON transformation where the "specification" for the transform is itself a JSON document.

Port of Java [Jolt](https://github.com/bazaarvoice/jolt) library written in Rust.

This is a fork of [Fluvio Jolt](https://github.com/infinyon/fluvio-jolt), created by the Fluvio team. That project was archived on March 2nd 2025.


## Usage Example
Add `harmony-jolt` crate to your `Cargo.toml` file:
```toml
[dependencies]
harmony-jolt = "0.8"
```

Then, for example, if you want to repack your JSON record, you can do the following:
```rust
use serde_json::{json, Value};
use harmony_jolt::{transform, TransformSpec};

let input: Value = serde_json::from_str(r#"
    {
        "id": 1,
        "name": "John Smith",
        "account": {
            "id": 1000,
            "type": "Checking"
        }
    }
"#).unwrap();

let spec: TransformSpec =
serde_json::from_str(r#"[
    {
      "operation": "shift",
      "spec": {
        "name": "data.name",
        "account": "data.account"
      }
    }
  ]"#).unwrap();

let output = transform(input, &spec).unwrap();

assert_eq!(output, json!({
    "data" : {
      "name": "John Smith",
      "account": {
        "id": 1000,
        "type": "Checking"
      }
    }
}));
```
## Supported Operations
1. `shift`: copy data from the input tree and put it in the output tree
2. `default`: apply default values to the tree
3. `modify-overwrite-beta`: modify data using functions (concat, toLower, toUpper, substring, trim, join, split)
4. `remove`: remove data from the tree

Operations run in the order they appear in the spec.

See `SPEC.md` for more info on specifics of execution order and DSL grammar.

## Specification

Composes a list of operation specifications. Each operation has its own DSL (Domain Specific
Language) in order to facilitate its narrow job.

```
use harmony_jolt::TransformSpec;

let spec: TransformSpec =
serde_json::from_str(r#"[
    {
      "operation": "shift",
      "spec": {
        "name": "data.name",
        "account": "data.account"
      }
    }
  ]"#).unwrap();
```

### <a name="shift"></a>`Shift` operation
Specifies where the data from the input JSON should be placed in the output JSON, or in other
words, how the input JSON/data should be shifted around to make the output JSON/data.

At a base level, a single `shift` operation is a mapping from an input path to an output path,
similar to the `mv` command in Unix, `mv /var/data /var/backup/data`.

The input path is a JSON tree structure, and the output path is flattened "dot notation" path
notation.

 For example, given this simple input JSON:
 <pre>
{
    "id": 1,
    "name": "John Smith",
    "account": {
        "id": 1000,
        "type": "Checking"
    }
}
</pre>
A simple spec could be constructed by copying that input, and modifying it to supply an output
path for each piece of data:
<pre>
{
    "id": "data.id",
    "name": "data.name",
    "account": "data.account"
}
</pre>
would produce the following output JSON:
<pre>
{
    "data" : {
        "id": 1,
        "name": "John Smith",
        "account": {
            "id": 1000,
            "type": "Checking"
        }
    }
}
</pre>

### Wildcards
The `shift` specification on the keys level supports wildcards and conditions:  
    1. `*` - match everything  
    2. `name1|name2|nameN` - match any of the specified names

#### `&` Wildcard
`&` lookup allows referencing the values captured by the `*` or `|`.

`&(x,y)` means go up the path x levels and get the yth match from that level.

The 0th match is always the entire input key. The rest are the specific things the `*`s matched.

`&` == `&(0)` == `&(0,0)` and `&(x)` == `&(x,0)` 

It allows for specs to be more compact. For example, for this input:
 <pre>
{
    "id": 1,
    "name": "John Smith",
    "account": {
        "id": 1000,
        "type": "Checking"
    }
}
</pre>
to get the output:
<pre>
{
    "data" : {
        "id": 1,
        "name": "John Smith",
        "account": {
            "id": 1000,
            "type": "Checking"
        }
    }
}
</pre>
the spec with wildcards would be:
<pre>
{
    "*": "data.&0"
}
</pre>
If you want only `id` and `name` in the output, the spec is:
<pre>
{
    "id|name": "data.&(0)"
}
</pre>


`&` wildcard also allows to dereference any level of the path of given node:
<pre>
{
    "foo": {
        "bar" : {
            "baz": "new_location.&(0).&(1).&(2)" // &(0) = baz, &(1) = bar, &(2) = foo
            }
        }
    }
</pre>
for the input:
<pre>
{
    "foo": {
      "bar": {
        "baz": "value"
      }
    }
  }
</pre>
will produce:
<pre>
{
    "new_location": {
      "baz": {
        "bar": {
          "foo": "value"
        }
      }
    }
}
</pre>

#### `$` Wildcard

`$` on the left hand side takes a matched key from the path and writes it as a value. It is not valid on the right hand side.

See tests in `tests/java/resources/shift` for usage examples.

See [java library docs here](https://github.com/bazaarvoice/jolt/blob/master/jolt-core/src/main/java/com/bazaarvoice/jolt/Shiftr.java).

#### `@` Wildcard

`@` on the left hand side takes the value at a path and writes it to the output. On the right hand side, `@(n,key)` uses an input value as part of the output path.

See tests in `tests/java/resources/shift` for usage examples.

#### `#` Wildcard

On the left hand side, `#` writes a literal string. Everything after the `#` is the literal, so it
can hold dots, for example `"#http://hl7.org/fhir/sid/us-ssn": "identifier.system"`.

On the right hand side, `[#n]` uses a match count as an array index. It counts how many matches
the spec level `n` steps up has made so far, where the current leaf is level 0. This turns a map
into a dense array:
<pre>
{
    "ratings": {
        "primary|quality": {
            "$": "out[#2].name",
            "value": "out[#2].value"
        }
    }
}
</pre>
writes the matched entries to `out[0]`, `out[1]` and so on, skipping any other keys `ratings` holds.
The order follows the input, not the spec.

#### Array index limit

An array index on the right hand side, such as `[&0]`, `[@(1,idx)]`, `[#n]` or `[3]`, can point
past the end of the array. The gap is filled with nulls. Each `shift` run may fill at most 100,000
null slots in total, across all arrays. Going over fails with `Error::ArrayPaddingLimit`. Writing to
the next free index costs nothing, so reshaping a large input array in order is not limited.
A gap is charged when it is created, even if later writes fill it. So a large array written out of
order can hit the limit.

An index taken from a string, such as a key for `[&0]` or a string value for `[@(1,idx)]`, may be
at most 20 characters long. That is the number of digits in the largest index. A longer string
fails with `Error::IndexTooLong`, even if it is a valid number with leading zeros.

See [java library docs here](https://github.com/bazaarvoice/jolt/blob/master/jolt-core/src/main/java/com/bazaarvoice/jolt/Shiftr.java).

### `Default` operation
Applies default values if the value is not present in the input JSON.

 For example, given this simple input JSON:
 <pre>
{
    "phones": {
        "mobile": "01234567",
        "country": "US"
    }
}
</pre>
with the following specification for `default` operation:
<pre>
{
    "phones": {
        "mobile": "0000000",
        "code": "+1"
    }
}
</pre>
the output JSON will be:
<pre>
{
    "phones": {
        "mobile": "01234567",
        "country": "US",
        "code": "+1"
    }
}
</pre>
As you can see, the field `mobile` remains not affected while the `code` has a default '+1' value.

A default is also written where the input value is `null`, as in Java JOLT. Array values in the
spec are literals: `{"meta": {"profile": ["http://example.org/x"]}}` writes the array as is.
An existing array in the input is never merged with the default.

### `Modifier` operations
The `modify-overwrite-beta` operation allows you to modify values using functions. This is useful for string manipulation, data transformation, and combining values.

#### Supported Functions
- **`concat`**: Concatenate multiple strings
- **`toLower`**: Convert string to lowercase
- **`toUpper`**: Convert string to uppercase
- **`substring`**: Extract a substring (start, end)
- **`trim`**: Remove leading/trailing whitespace
- **`join`**: Join array elements with a separator
- **`split`**: Split a string into an array, for example `=split(',', @(1,tags))`. The separator is a plain string. Java JOLT treats it as a regular expression. Trailing empty parts are dropped, as in Java. A split that would produce more than 100,000 parts fails.

There is no date or time function, and `${...}` substitution is not supported. A string such as `${now}` is copied as is. Pass values like the current time in through the input instead.

#### Example: Concatenating Names
For example, given this input JSON:
<pre>
{
    "customer": {
        "firstName": "John",
        "lastName": "Doe"
    }
}
</pre>

You can use `shift` to restructure and `modify-overwrite-beta` with `concat` to combine the names:
<pre>
[
    {
        "operation": "shift",
        "spec": {
            "customer": {
                "firstName": "name[0]",
                "lastName": "name[1]"
            }
        }
    },
    {
        "operation": "modify-overwrite-beta",
        "spec": {
            "name": "=concat(@(1,name[0]),' ',@(1,name[1]))"
        }
    }
]
</pre>

This produces:
<pre>
{
    "name": "John Doe"
}
</pre>

The `@(n,path)` syntax navigates the context stack:
- `@(1,path)` - reference from current object (the one being modified)
- `@(2,path)` - go up 1 level (parent), then navigate path
- `@(n,path)` - go up n-1 levels from current, then navigate path

In the example above, `@(1,name[0])` references `name[0]` from the current object (root level).

#### Function Syntax
Functions are specified using the `=functionName(args...)` syntax:
- **Literals**: Single-quoted strings (required) like `' '`, `'_'`, or `'-'`. Double quotes are not supported.
- **Numbers**: Unquoted numeric literals like `0`, `4`, `42`, or `100`. These are passed as actual numbers to functions.
- **Path expressions**: `@(n,path)` to reference values (see above)
- **Identifiers**: Unquoted values that will be looked up
- **Nested functions**: Functions can be nested (see example below)

#### Nested Functions
Functions can be nested, allowing you to compose complex transformations. Inner functions are evaluated first, and their results become arguments to outer functions.

Example: Format a date string by extracting and joining parts. Given input `{"date": "20250605"}` at root level:
<pre>
{
    "formatted": "=concat(=substring(@(1,date),0,4),'-',=substring(@(1,date),4,6),'-',=substring(@(1,date),6,8))"
}
</pre>

This produces `{"date": "20250605", "formatted": "2025-06-05"}` by:
1. Extracting characters 0-4 ("2025")
2. Extracting characters 4-6 ("06")
3. Extracting characters 6-8 ("05")
4. Concatenating them with "-" as separator

#### Modifier Variants
- **`modify-overwrite-beta`**: Always writes the result, overwriting existing values
- **`modify-default-beta`**: Only writes if the key doesn't exist or its value is null
- **`modify-define-beta`**: Only writes if the key doesn't exist

`modify` is another name for `modify-overwrite-beta`. These rules match Java JOLT.

A nested spec needs an object, or an array when it uses the `*` wildcard. When an object member has another type, overwrite replaces it with a new object. Default replaces only a `null`. Define keeps the value. A `*` walk over an array skips scalar elements under every variant.

A function does not run for a value that is kept, so a missing path there is not an error. A malformed function string can still be rejected.

Unlike Java JOLT, a spec key other than `*` does not address an array index, and `[0]` is not supported. Next to a `*`, such a key is ignored on an array. Java skips the whole block for a literal key, and honours `[n]`.

A spec that holds only `*` walks what exists. It never creates a parent or replaces a value. Literal keys run before `*`, so spec order does not matter. Under overwrite, `*` replaces a leaf it shares with a literal object key. `*` does not walk a key that the same block gives a leaf value.

A path that cannot be resolved is an error under every variant. That includes `@(n,...)` when the level `n` does not exist in the input.

One modifier run may copy or write about 64 MiB, estimated across function arguments, function results, written values and path lookups. Going over is a `ModifierBudgetLimit` error. The limit applies to each modifier operation in a spec separately.

### `Remove` operation
Removes content from the input JSON.
The spec structure matches the input JSON structure. The value of fields is ignored.

 For example, given this simple input JSON:
 <pre>
{
    "phones": {
        "mobile": "01234567",
        "country": "US"
    }
}
</pre>
you can remove the `country` by the following specification for `remove` operation:
<pre>
{
    "phones": {
        "country": ""
    }
}
</pre>
the output JSON will be:
<pre>
{
    "phones": {
        "mobile": "01234567"
    }
}
</pre>

Spec keys can use `*` to match any run of characters. `"*"` matches every key, and `"tmp_*"`
matches every key that starts with `tmp_`. Write `\*` to match a literal `*`. On arrays, keys match
element indexes, so `"0"` removes the first element and `"*"` walks every element:
<pre>
{
    "patients": {
        "*": {
            "ssn": ""
        }
    }
}
</pre>
removes `ssn` from every element of the `patients` array.

## Differences from Java JOLT

- `default` does not support `*`, `|` or `key[]` keys. A `*` key is written as a literal key.
- `remove` accepts any leaf value. Java accepts only `""` or an object.
- `split` takes a plain separator, not a regular expression.
- Neither library supports `${...}` substitution. A string such as `${now}` is copied as is.

## Contributing

If you'd like to contribute to the project, please read our [Contributing guide](CONTRIBUTING.md).

## License

This project is licensed under the [Apache license](LICENSE).
