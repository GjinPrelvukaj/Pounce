//! Deterministic fixture site and benchmark harness for Pounce.
//!
//! The fixture site is generated from a seed, so the same `GraphSpec`
//! always produces a byte-identical website. That reproducibility is what
//! makes benchmark numbers comparable across machines and over time.

pub mod rng;
