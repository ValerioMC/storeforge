pub mod model;
pub(super) mod repository;
mod service;

pub use service::RgwIdentityService;

#[cfg(test)]
#[path = "../../../tests/unit/administration/rgw_identities.rs"]
pub(crate) mod tests;
