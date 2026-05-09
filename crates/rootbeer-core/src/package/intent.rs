use std::fmt;

use serde::{Deserialize, Serialize};

use super::{LockedPackage, PackageRequest};
use crate::deterministic::DeterministicInput;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PackageIntent {
    Request(PackageRequest),
    Locked(LockedPackage),
}

impl PackageIntent {
    pub fn request(request: PackageRequest) -> Self {
        Self::Request(request)
    }

    pub fn locked(package: LockedPackage) -> Self {
        Self::Locked(package)
    }
}

impl DeterministicInput for PackageIntent {
    const KIND: &'static str = "package.intent";
}

impl fmt::Display for PackageIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageIntent::Request(request) => write!(f, "{request}"),
            PackageIntent::Locked(package) => write!(f, "{}", package.id()),
        }
    }
}
