use std::ops::Deref;

use nsstring::nsCString;

/// We use firefox's nsCString instead of Rust's String, as it brings benefits
/// such as reference counting. However, pdslib uses Uri as HashMap keys.
/// This is a newtype wrapper around nsCString to implement Hash and Eq.
#[derive(Debug, Clone, PartialEq)]
pub struct MozUri(pub nsCString);

impl MozUri {
    pub fn new(uri: &str) -> Self {
        Self(nsCString::from(uri))
    }
}

impl From<nsCString> for MozUri {
    fn from(uri: nsCString) -> Self {
        Self(uri)
    }
}

impl Deref for MozUri {
    type Target = nsCString;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::hash::Hash for MozUri {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl std::cmp::Eq for MozUri {}
