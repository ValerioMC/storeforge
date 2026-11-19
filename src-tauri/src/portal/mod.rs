pub mod controller;
mod model;
mod service;

#[cfg(test)]
#[path = "../../tests/unit/portal/spaces.rs"]
mod portal_spaces;

#[cfg(test)]
#[path = "../../tests/unit/portal/sharing.rs"]
mod portal_sharing;

#[cfg(test)]
#[path = "../../tests/unit/portal/governance.rs"]
mod portal_governance;
