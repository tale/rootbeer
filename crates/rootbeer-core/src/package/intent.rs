use std::fmt;

use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Deserialize, Serialize, Serializer};

use super::{
    LockedPackage, PackageRealizationInput, PackageRequest, PackageResolverInputs, ResolveContext,
};
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

    fn input(&self) -> PackageIntentInput<'_> {
        match self {
            PackageIntent::Request(request) => PackageIntentInput::Request { request },
            PackageIntent::Locked(package) => PackageIntentInput::Locked {
                package: package.realization_input(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageLockInput {
    pub context: ResolveContext,
    pub resolver_inputs: PackageResolverInputs,
    pub intents: Vec<PackageIntent>,
}

impl PackageLockInput {
    pub fn new(context: ResolveContext, intents: Vec<PackageIntent>) -> Self {
        Self::with_resolver_inputs(context, PackageResolverInputs::default(), intents)
    }

    pub fn with_resolver_inputs(
        context: ResolveContext,
        resolver_inputs: PackageResolverInputs,
        intents: Vec<PackageIntent>,
    ) -> Self {
        Self {
            context,
            resolver_inputs,
            intents,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.intents.is_empty()
    }
}

impl DeterministicInput for PackageLockInput {
    const KIND: &'static str = "package.lock";
}

impl Serialize for PackageLockInput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("PackageLockInput", 3)?;
        state.serialize_field("context", &self.context)?;
        state.serialize_field("resolver_inputs", &self.resolver_inputs)?;
        state.serialize_field("intents", &SerializablePackageIntents(&self.intents))?;
        state.end()
    }
}

struct SerializablePackageIntents<'a>(&'a [PackageIntent]);

impl Serialize for SerializablePackageIntents<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for intent in self.0 {
            seq.serialize_element(&intent.input())?;
        }
        seq.end()
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PackageIntentInput<'a> {
    Request {
        request: &'a PackageRequest,
    },
    Locked {
        package: PackageRealizationInput<'a>,
    },
}

impl fmt::Display for PackageIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageIntent::Request(request) => write!(f, "{request}"),
            PackageIntent::Locked(package) => write!(f, "{}", package.id()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::package::{
        GitHubRepositoryPin, LockedInstall, LockedSource, Provides, ResolverInput,
    };

    fn package(output_sha256: Option<&str>) -> LockedPackage {
        LockedPackage {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::Path {
                path: PathBuf::from("demo"),
                sha256: "source".to_string(),
            },
            install: LockedInstall::Directory { strip_prefix: None },
            provides: Provides {
                bins: BTreeMap::new(),
            },
            output_sha256: output_sha256.map(str::to_string),
        }
    }

    #[test]
    fn lock_input_fingerprint_ignores_locked_output_hashes() {
        let context = ResolveContext::new("test-system");
        let without_output =
            PackageLockInput::new(context.clone(), vec![PackageIntent::locked(package(None))]);
        let with_output = PackageLockInput::new(
            context,
            vec![PackageIntent::locked(package(Some("output")))],
        );

        assert_eq!(
            without_output.fingerprint().unwrap(),
            with_output.fingerprint().unwrap()
        );
    }

    #[test]
    fn lock_input_fingerprint_includes_resolver_inputs() {
        let context = ResolveContext::new("test-system");
        let base =
            PackageLockInput::new(context.clone(), vec![PackageIntent::locked(package(None))]);
        let pinned = PackageLockInput::with_resolver_inputs(
            context,
            PackageResolverInputs {
                resolvers: BTreeMap::from([(
                    "aqua".to_string(),
                    ResolverInput::AquaRegistry(GitHubRepositoryPin {
                        owner: "aquaproj".to_string(),
                        repo: "aqua-registry".to_string(),
                        rev: "abc123".to_string(),
                    }),
                )]),
            },
            vec![PackageIntent::locked(package(None))],
        );

        assert_ne!(base.fingerprint().unwrap(), pinned.fingerprint().unwrap());
    }
}
