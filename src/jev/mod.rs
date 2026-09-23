//! TypeSafe Jev (System One) API layer.

/// Single HTTP choke point: retry policy + sha256-keyed response cache.
pub mod client;
/// Measured question builders: Stage 2 Nouls, Stage 3 listwise Choice.
pub mod questions;
/// Hand-rolled serde mirror of the System One wire format.
pub mod schemas;
