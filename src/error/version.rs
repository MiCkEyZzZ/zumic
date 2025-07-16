use thiserror::Error;

use crate::engine::FormatVersion;

#[derive(Debug, Error)]
pub enum VersionError {
    #[error("Unsupported ZDB dump version: {found}. Supported versions: {supported:?}")]
    UnsupportedVersion {
        found: u8,
        supported: Vec<FormatVersion>,
    },

    #[error("Version incompatibility: reader {reader} cannot read dump version {dump}")]
    IncompatibleVersion {
        reader: FormatVersion,
        dump: FormatVersion,
    },

    #[error("Deprecated version {version} detected. Please upgrade to {recommended}")]
    DeprecatedVersion {
        version: FormatVersion,
        recommended: FormatVersion,
    },

    #[error("Cannot write version {target} dump using {writer} writer")]
    WriteIncompatible {
        writer: FormatVersion,
        target: FormatVersion,
    },
}
