//! `lute check-project` integration tests, grouped by contract in `check_project/contracts.rs`.
//!
//! The support module is kept at the integration-test root so all CLI targets
//! can share the same temporary-project and process harness.

mod support;

#[path = "check_project/contracts.rs"]
mod contracts;
