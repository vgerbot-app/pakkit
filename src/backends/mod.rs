//! Package-manager backends.
//!
//! The app talks to one backend per platform, detected at startup. Each
//! backend module owns its CLI discovery, argument builders and output
//! parsers; [`types`] holds the data shapes they all produce and [`exec`]
//! the shared process plumbing.

pub mod exec;
pub mod types;
pub mod winget;
