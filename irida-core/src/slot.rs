//! Pins (stub, replaced in Task 3).

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pins(pub(crate) BTreeMap<String, String>);
