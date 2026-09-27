//! rmk's only test target, named `integration` after this directory.
//!
//! [`simulator`] is the harness every case runs on. `run_tests!` expands each
//! `scenarios/*.toml` into a `mod` of keyboard-behavior tests;
//! `scenarios/README.md` documents their syntax. [`rynk`], [`vial`], and
//! [`ble_profile`] hold what a scenario file cannot express: wire-protocol
//! writes interleaved with matrix input, and what the BLE profile task received.

// The harness offers the whole step vocabulary, and each feature row plays a
// subset of it — so per-row dead code is expected, not a finding.
#![allow(dead_code)]

mod simulator;

#[cfg(feature = "_ble")]
mod ble_profile;
// The case drives VIA packets, so it needs the host session that answers them.
#[cfg(all(feature = "rgb_matrix", feature = "vial"))]
mod lighting;
#[cfg(feature = "rynk")]
mod rynk;
#[cfg(feature = "vial")]
mod vial;

rmk_macro::run_tests!("tests/scenarios");
