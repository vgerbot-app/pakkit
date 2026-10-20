---
name: rust-serde
description: Serialize/deserialize data with serde — derive attributes, renames, defaults, enum representations, skip/borrow patterns, and choosing between serde_json/toml/bincode. Use when adding serde derives or designing a wire/on-disk format.
---

# Rust serde

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"          # JSON
# toml = "0.8"            # config files
# bincode = "1"           # compact binary
```

## Core derive

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]     // JSON field naming
pub struct Config {
    pub endpoint: String,
    #[serde(default)]                  // tolerate missing key
    pub retries: u32,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    #[serde(rename = "v")]             // per-field override
    pub version: u8,
}

fn default_timeout() -> u64 { 30_000 }
```

- `deny_unknown_fields` when extra keys should be an error (config validation); omit it for
  forward-compatible wire formats.
- `flatten` merges a nested struct into the parent object; combine with `deny_unknown_fields`
  carefully — they interact poorly.

## Enums

```rust
// externally tagged (default): {"Variant": {...}} or "Unit"
#[derive(Serialize, Deserialize)]
enum Shape { Unit, Tuple(u32), Struct { w: u32 } }

// internally tagged — must be struct/unit variants only:
#[serde(tag = "type", rename_all = "snake_case")]
enum Message { Ping, Data { id: u64 } }   // {"type":"data","id":1}

// adjacently tagged — works with tuple variants:
#[serde(tag = "t", content = "c")]
enum Part { Raw(Vec<u8>), Text(String) }  // {"t":"raw","c":[...]}
```

Prefer internally/adjacently tagged for JSON consumed by other languages; externally tagged is
fine for Rust-to-Rust.

## Borrowing and zero-copy

```rust
#[derive(Deserialize)]
struct Header<'a> {
    #[serde(borrow)]
    name: &'a str,          // borrows from the input buffer; no allocation
}
```

- Borrowed deserialization only works when the input outlives the value and the format supports it
  (`serde_json::from_str`, `from_slice`). Not usable with streaming/`from_reader` generically.
- Use `Cow<'a, str>` when a field is usually borrowed but sometimes escaped/owned.
- For fast JSON parsing with owned data, consider `simd-json`; for very large documents,
  `serde_json::from_reader` plus `StreamDeserializer`.

## Skipping and conditional fields

| Attribute | Effect |
|---|---|
| `skip` | never (de)serialized, requires `Default` on deserialize |
| `skip_serializing` / `skip_deserializing` | one direction only |
| `skip_serializing_if = "Option::is_none"` | omit `None` fields |
| `default` / `default = "fn"` | fill missing on deserialize |
| `alias = "old"` | accept legacy key names |

## Common pitfalls

- `#[serde(default)]` requires the field type to implement `Default` (or use `default = "path"`).
- `u64`/`i64` in JSON: JS numbers lose precision above 2^53. Use strings (`serialize_with`) for
  large IDs crossing into JavaScript.
- Date/time: add `time` or `chrono` with the `serde` feature; serde has no built-in datetime.
- `HashMap<K, V>` with non-string keys fails in JSON (keys are strings) unless the key is a
  unit-variant enum or you serialize as a sequence.
- Round-tripping through `serde_json::Value` is lossy for map ordering and numeric types.

## Runtime patterns

```rust
let json = serde_json::to_string(&cfg)?;              // compact
let pretty = serde_json::to_string_pretty(&cfg)?;     // indented
let cfg: Config = serde_json::from_str(&json)?;

// Merge defaults into a partial input:
let mut base = Config::default();
let partial: serde_json::Value = serde_json::from_str(input)?;
// ... or use #[serde(default)] on the struct itself.
```

## Verify

```bash
cargo test --all-features
cargo expand   # inspect generated impls if an attribute misbehaves
```
