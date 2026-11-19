pub mod controller;
pub mod model;
pub(crate) mod repository;
pub mod service;

#[cfg(test)]
#[path = "../../../tests/unit/connection/shared_service.rs"]
mod shared_service_tests;
