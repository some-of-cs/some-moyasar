//! Integration test suite entry point for the Rocket.rs example server.
#![forbid(unsafe_code)]

#[path = "rocket/mod.rs"]
mod rocket_suite;

pub use rocket_suite::{responder, state};
