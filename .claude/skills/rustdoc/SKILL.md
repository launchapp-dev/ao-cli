---
name: rustdoc
description: Rust documentation conventions (RFC 1574). Apply when writing doc comments on public Rust items. Covers summary sentences, section headings, type references, and examples.
---

# Rust Documentation Conventions (RFC 1574)

Apply these rules when writing doc comments (`///`) on public Rust items.

## Summary Sentence

Every doc comment starts with a single-line summary sentence.

```rust
// DO: third person singular present indicative, ends with period
/// Returns the length of the string.
/// Creates a new instance with default settings.
/// Parses the input and returns the result.

// DON'T: imperative, missing period, or verbose
/// Return the length of the string
/// This function creates a new instance with default settings.
/// Use this to parse the input and get the result back.
```

## Comment Style

Use line comments, not block comments.

```rust
// DO
/// Summary sentence here.
///
/// More details if needed.

// DON'T
/**
 * Summary sentence here.
 *
 * More details if needed.
 */
```

Use `//!` only for crate-level and module-level docs at the top of the file.

## Section Headings

Use these exact headings (always plural):

```rust
/// Summary sentence.
///
/// # Examples
///
/// # Panics
///
/// # Errors
///
/// # Safety
///
/// # Aborts
///
/// # Undefined Behavior
```

```rust
// DO
/// # Examples

// DON'T
/// # Example
/// ## Examples
/// **Examples:**
```

## Type References

Use full generic forms and link with reference-style markdown.

```rust
// DO
/// Returns [`Option<T>`] if the value exists.
///
/// [`Option<T>`]: std::option::Option

// DON'T
/// Returns `Option` if the value exists.
/// Returns an optional value.
```

## Examples

Every public item should have examples showing usage.

```rust
/// Adds two numbers together.
///
/// # Examples
///
/// ```
/// let result = my_crate::add(2, 3);
/// assert_eq!(result, 5);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

## Errors Section

Document all error conditions for functions returning `Result`.

```rust
/// # Errors
///
/// Returns [`io::Error`] if the file cannot be opened.
/// Returns [`ParseError`] if the file content is malformed.
```

## Panics Section

Document all conditions that cause panics.

```rust
/// # Panics
///
/// Panics if `index` is out of bounds.
```

## Safety Section

Required for all `unsafe` functions.

```rust
/// # Safety
///
/// The caller must ensure that `ptr` is valid and properly aligned.
/// The pointed-to value must be initialized.
```
