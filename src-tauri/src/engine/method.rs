use serde::{Deserialize, Serialize};
use std::fmt;

/// The only identifiers that may cross the frontend/backend boundary.
///
/// `method2` and `method3` remain accepted as legacy aliases for the two
/// supported installers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMethod {
    ResourceMount,
    Loader,
}

impl InstallMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResourceMount => "resource_mount",
            Self::Loader => "loader",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "resource_mount" => Ok(Self::ResourceMount),
            "loader" => Ok(Self::Loader),
            // Explicit migration map for settings written by older builds.
            "method3" => Ok(Self::ResourceMount),
            "method2" => Ok(Self::Loader),
            other => Err(format!("Metode instalasi tidak dikenal: {other}")),
        }
    }
}

impl fmt::Display for InstallMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
