pub mod controller;
mod model;
mod repository;
mod service;

#[cfg(test)]
#[path = "../../../tests/unit/audit/operational_events.rs"]
mod operational_events_tests;
