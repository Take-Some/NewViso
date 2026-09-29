#![forbid(unsafe_op_in_unsafe_fn)]

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContractKind {
    Wire,
    Schema,
    Abi,
    Protocol,
    Manifest,
}

impl ContractKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wire => "wire",
            Self::Schema => "schema",
            Self::Abi => "abi",
            Self::Protocol => "protocol",
            Self::Manifest => "manifest",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl ContractVersion {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub const fn major(major: u16) -> Self {
        Self::new(major, 0, 0)
    }
}

impl fmt::Display for ContractVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContractCompatibility {
    /// Producer/consumer must use exactly the same version.
    Exact,
    /// Any version in the same major family is accepted.
    SameMajor,
    /// Offered version must be greater than or equal to the registered minimum.
    AtLeast,
}

impl ContractCompatibility {
    pub fn accepts(self, expected: ContractVersion, offered: ContractVersion) -> bool {
        match self {
            Self::Exact => offered == expected,
            Self::SameMajor => offered.major == expected.major,
            Self::AtLeast => offered >= expected,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContractSpec {
    /// Stable registry key. It does not contain the version number.
    pub key: &'static str,
    pub kind: ContractKind,
    pub version: ContractVersion,
    pub compatibility: ContractCompatibility,
    /// Crate that owns the contract definition.
    pub owner: &'static str,
    /// Optional token already carried on the wire/service boundary.
    pub advertised_id: Option<&'static str>,
}

impl ContractSpec {
    pub const fn new(
        key: &'static str,
        kind: ContractKind,
        version: ContractVersion,
        compatibility: ContractCompatibility,
        owner: &'static str,
        advertised_id: Option<&'static str>,
    ) -> Self {
        Self {
            key,
            kind,
            version,
            compatibility,
            owner,
            advertised_id,
        }
    }

    pub fn accepts_version(self, offered: ContractVersion) -> bool {
        self.compatibility.accepts(self.version, offered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_requires_identical_version() {
        assert!(ContractCompatibility::Exact
            .accepts(ContractVersion::major(2), ContractVersion::major(2)));
        assert!(!ContractCompatibility::Exact
            .accepts(ContractVersion::major(2), ContractVersion::new(2, 1, 0)));
    }

    #[test]
    fn same_major_accepts_minor_evolution() {
        assert!(ContractCompatibility::SameMajor
            .accepts(ContractVersion::new(3, 1, 0), ContractVersion::new(3, 9, 7)));
        assert!(!ContractCompatibility::SameMajor
            .accepts(ContractVersion::major(3), ContractVersion::major(4)));
    }

    #[test]
    fn at_least_is_monotonic() {
        assert!(ContractCompatibility::AtLeast
            .accepts(ContractVersion::new(1, 2, 0), ContractVersion::new(1, 3, 0)));
        assert!(!ContractCompatibility::AtLeast
            .accepts(ContractVersion::new(1, 2, 0), ContractVersion::new(1, 1, 9)));
    }
}
