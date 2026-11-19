pub mod model;
pub(super) mod repository;
mod service;

pub use service::RgwIdentityService;

#[cfg(test)]
#[path = "../../../tests/unit/administration/rgw_identities.rs"]
mod tests;
