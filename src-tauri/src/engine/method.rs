use serde::{Deserialize, Serialize};
use std::fmt;

/// The only identifiers that may cross the frontend/backend boundary.
///
/// `method1`, `method2`, and `method3` are accepted only as explicit legacy
/// aliases so existing settings can be migrated without silently selecting a
/// different installer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMethod {
    ResourceMount,
    Loader,
    SignatureBypass,
}

impl InstallMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResourceMount => "resource_mount",
            Self::Loader => "loader",
            Self::SignatureBypass => "signature_bypass",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "resource_mount" => Ok(Self::ResourceMount),
            "loader" => Ok(Self::Loader),
            "signature_bypass" => Ok(Self::SignatureBypass),
            // Explicit migration map for settings written by older builds.
            "method3" => Ok(Self::ResourceMount),
            "method2" => Ok(Self::Loader),
            "method1" => Ok(Self::SignatureBypass),
            other => Err(format!("Metode instalasi tidak dikenal: {other}")),
        }
    }
}

impl fmt::Display for InstallMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
