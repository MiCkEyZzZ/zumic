use zumic_error::ZdbVersionError;

/// «Магическое» начало файла: ASCII-буквы «ZDB».
pub const FILE_MAGIC: &[u8; 3] = b"ZDB";

/// Поддерживаемые версии формата дампа ZDB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum FormatVersion {
    /// Legacy данные без явной версии (до введения версионирования)
    Legacy = 0,
    /// Версия 1 - базовая реализация с версионированием
    V1 = 1,
    /// Версия 2 - с улучшенным сжатием и новыми типами данных
    V2 = 2,
    /// Версия 3 - с varint encoding для размеров (экономия 20-30%)
    V3,
}

#[derive(Debug, Clone)]
pub struct CompatibilityInfo {
    pub reader_version: FormatVersion,
    pub dump_version: FormatVersion,
    pub can_read: bool,
    pub can_write: bool,
    pub requires_migration: bool,
    pub warnings: Vec<String>,
}

pub struct VersionUtils;

impl FormatVersion {
    /// Возвращает текущую версию формата по умолчанию.
    pub const fn current() -> Self {
        FormatVersion::V3
    }

    /// Проверяем, использует ли версия varint encoding для размеров.
    pub const fn uses_varint(&self) -> bool {
        matches!(self, FormatVersion::V3)
    }

    /// Проверяет, может ли данная версия читать указанную версию.
    pub fn can_read(
        &self,
        target: FormatVersion,
    ) -> bool {
        use FormatVersion::*;
        match (self, target) {
            (Legacy, Legacy) => true,
            (Legacy, _) => false,
            (V1, Legacy) => true,
            (V1, V1) => true,
            (V1, V2 | V3) => false,
            (V2, Legacy) => true,
            (V2, V1) => true,
            (V2, V2) => true,
            (V2, V3) => false,
            (V3, _) => true,
        }
    }

    /// Проверяет, может ли данная версия писать в указанную версию.
    pub fn can_write(
        &self,
        target: FormatVersion,
    ) -> bool {
        *self <= target
    }

    /// Возвращает список всех поддерживаемых версий.
    pub fn supported_versions() -> Vec<FormatVersion> {
        vec![
            FormatVersion::Legacy,
            FormatVersion::V1,
            FormatVersion::V2,
            FormatVersion::V3,
        ]
    }

    /// Возвращает человекочитаемое описание версии.
    pub fn description(&self) -> &'static str {
        match self {
            FormatVersion::Legacy => "Legacy format (before versioning)",
            FormatVersion::V1 => "Version 1 (basic versioning)",
            FormatVersion::V2 => "Version 2 (enhanced compression)",
            FormatVersion::V3 => "Version 3 (varint encoding, 20-30% smaller)",
        }
    }

    /// Проверяет, является ли версия устаревшей.
    pub fn is_deprecated(&self) -> bool {
        matches!(self, FormatVersion::Legacy | FormatVersion::V1)
    }

    /// Возвращает рекомендуемую версию для миграции.
    pub fn recommended_upgrade(&self) -> Option<FormatVersion> {
        match self {
            FormatVersion::Legacy => Some(FormatVersion::V1),
            FormatVersion::V1 => Some(FormatVersion::V2),
            FormatVersion::V2 => Some(FormatVersion::V3),
            FormatVersion::V3 => None,
        }
    }
}

impl std::fmt::Display for FormatVersion {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            FormatVersion::Legacy => write!(f, "Legacy"),
            FormatVersion::V1 => write!(f, "V1"),
            FormatVersion::V2 => write!(f, "V2"),
            FormatVersion::V3 => write!(f, "V3"),
        }
    }
}

impl CompatibilityInfo {
    /// Проверяет совместимость между версией читателя и версией дампа
    pub fn check(
        reader_version: FormatVersion,
        dump_version: FormatVersion,
    ) -> Self {
        let can_read = reader_version.can_read(dump_version);
        let can_write = reader_version.can_write(dump_version);
        let requires_migration = dump_version.is_deprecated();

        let mut warnings = Vec::new();

        if requires_migration {
            warnings.push(format!(
                "Dump version {} is deprecated. Consider upgrading to {}",
                dump_version,
                dump_version
                    .recommended_upgrade()
                    .unwrap_or(FormatVersion::current())
            ));
        }

        if dump_version > reader_version {
            warnings.push(format!(
                "Dump version {dump_version} is newer than reader version {reader_version}. Some features may not be supported"
            ));
        }

        if !can_read {
            warnings.push(format!(
                "Reader version {reader_version} cannot read dump version {dump_version}"
            ));
        }

        if dump_version == FormatVersion::V3 && reader_version < FormatVersion::V3 {
            warnings.push(
                "V3 format uses varint encoding. Older reader cannot parse this format."
                    .to_string(),
            );
        }

        CompatibilityInfo {
            reader_version,
            dump_version,
            can_read,
            can_write,
            requires_migration,
            warnings,
        }
    }
}

impl VersionUtils {
    /// Определяет версию по содержимому дампа.
    pub fn detect_version(data: &[u8]) -> Result<FormatVersion, ZdbVersionError> {
        if data.len() < 4 {
            return Err(ZdbVersionError::UnsupportedVersion {
                found: 0,
                supported: FormatVersion::supported_versions()
                    .into_iter()
                    .map(|v| v as u8)
                    .collect(),
                offset: None,
                key: None,
            });
        }

        // Проверяем магическое число.
        if &data[0..3] != FILE_MAGIC {
            return Err(ZdbVersionError::UnsupportedVersion {
                found: 0,
                supported: FormatVersion::supported_versions()
                    .into_iter()
                    .map(|v| v as u8)
                    .collect(),
                offset: Some(0),
                key: None,
            });
        }

        // Четвёртый байт - версия.
        let version_byte = data[3];
        FormatVersion::try_from(version_byte)
    }

    /// Валидирует совместимость версий с подробной диагностикой.
    pub fn validate_compatibility(
        reader_version: FormatVersion,
        dump_version: FormatVersion,
    ) -> Result<CompatibilityInfo, ZdbVersionError> {
        let info = CompatibilityInfo::check(reader_version, dump_version);

        if !info.can_read {
            return Err(ZdbVersionError::IncompatibleVersion {
                reader: reader_version as u8,
                dump: dump_version as u8,
                offset: None,
                key: None,
            });
        }

        Ok(info)
    }

    /// Возвращает список изменений между версиями.
    pub fn version_changes(
        from: FormatVersion,
        to: FormatVersion,
    ) -> Vec<String> {
        let mut changes = Vec::new();

        use FormatVersion::*;

        if from < V1 && to >= V1 {
            changes.push("Added explicit version header".to_string());
            changes.push("Improved error handling".to_string());
        }

        if from < V2 && to >= V2 {
            changes.push("Enhanced compression algorithm".to_string());
            changes.push("Added new data types support".to_string());
            changes.push("Improved streaming performance".to_string());
        }

        if from < V3 && to >= V3 {
            changes.push("Varint encoding for all size (LEB128)".to_string());
            changes.push("20-30% space saving on typical data".to_string());
            changes.push("1 byte for sizes <128 (vs 4 bytes)".to_string());
            changes.push("2 bytes for sizes <16384 (vs 4 bytes)".to_string());
            changes.push("Backward compatible reader".to_string());
        }

        changes
    }
}

impl TryFrom<u8> for FormatVersion {
    type Error = ZdbVersionError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(FormatVersion::Legacy),
            1 => Ok(FormatVersion::V1),
            2 => Ok(FormatVersion::V2),
            3 => Ok(FormatVersion::V3),
            other => Err(ZdbVersionError::UnsupportedVersion {
                found: other,
                supported: FormatVersion::supported_versions()
                    .into_iter()
                    .map(|v| v as u8)
                    .collect(),
                offset: None,
                key: None,
            }),
        }
    }
}

/// Текущая версия формата дампа, как число (для совместимости).
pub const DUMP_VERSION: u8 = FormatVersion::V3 as u8;

#[cfg(test)]
mod tests {
    use zumic_error::{ZdbError, ZdbVersionError};

    use super::*;

    #[test]
    fn test_version_ordering() {
        assert!(FormatVersion::Legacy < FormatVersion::V1);
        assert!(FormatVersion::V1 < FormatVersion::V2);
        assert!(FormatVersion::V2 < FormatVersion::V3);
    }

    #[test]
    fn test_current_version() {
        assert_eq!(FormatVersion::current(), FormatVersion::V3);
    }

    #[test]
    fn test_uses_varint() {
        assert!(!FormatVersion::Legacy.uses_varint());
        assert!(!FormatVersion::V1.uses_varint());
        assert!(!FormatVersion::V2.uses_varint());
        assert!(FormatVersion::V3.uses_varint());
    }

    #[test]
    fn test_try_from_u8() {
        assert_eq!(FormatVersion::try_from(0).unwrap(), FormatVersion::Legacy);
        assert_eq!(FormatVersion::try_from(1).unwrap(), FormatVersion::V1);
        assert_eq!(FormatVersion::try_from(2).unwrap(), FormatVersion::V2);
        assert_eq!(FormatVersion::try_from(3).unwrap(), FormatVersion::V3);

        let err = FormatVersion::try_from(99).unwrap_err();
        assert!(matches!(
            err,
            ZdbVersionError::UnsupportedVersion { found: 99, .. }
        ));
    }

    #[test]
    fn test_deprecated_and_recommended() {
        assert!(FormatVersion::Legacy.is_deprecated());
        assert!(FormatVersion::V1.is_deprecated());
        assert!(!FormatVersion::V2.is_deprecated());
        assert!(!FormatVersion::V3.is_deprecated());

        assert_eq!(
            FormatVersion::Legacy.recommended_upgrade(),
            Some(FormatVersion::V1)
        );
        assert_eq!(
            FormatVersion::V1.recommended_upgrade(),
            Some(FormatVersion::V2)
        );
        assert_eq!(
            FormatVersion::V2.recommended_upgrade(),
            Some(FormatVersion::V3)
        );
        assert_eq!(FormatVersion::V3.recommended_upgrade(), None);
    }

    #[test]
    fn test_backward_compatibility() {
        assert!(FormatVersion::V3.can_read(FormatVersion::Legacy));
        assert!(FormatVersion::V3.can_read(FormatVersion::V1));
        assert!(FormatVersion::V3.can_read(FormatVersion::V2));
        assert!(FormatVersion::V3.can_read(FormatVersion::V3));

        assert!(!FormatVersion::V1.can_read(FormatVersion::V3));
        assert!(!FormatVersion::V2.can_read(FormatVersion::V3));

        assert!(FormatVersion::V1.can_write(FormatVersion::V1));
        assert!(FormatVersion::V1.can_write(FormatVersion::V2));
        assert!(!FormatVersion::V2.can_write(FormatVersion::V1));
    }

    #[test]
    fn test_compatibility_info_warnings() {
        let info = CompatibilityInfo::check(FormatVersion::V2, FormatVersion::V3);
        assert!(!info.can_read);
        assert!(info
            .warnings
            .iter()
            .any(|w| w.contains("varint") || w.contains("V3")));
    }

    #[test]
    fn test_version_description_and_display() {
        assert_eq!(
            FormatVersion::Legacy.description(),
            "Legacy format (before versioning)"
        );
        assert_eq!(
            FormatVersion::V1.description(),
            "Version 1 (basic versioning)"
        );
        assert_eq!(
            FormatVersion::V2.description(),
            "Version 2 (enhanced compression)"
        );
        assert_eq!(
            FormatVersion::V3.description(),
            "Version 3 (varint encoding, 20-30% smaller)"
        );

        assert_eq!(format!("{}", FormatVersion::Legacy), "Legacy");
        assert_eq!(format!("{}", FormatVersion::V1), "V1");
        assert_eq!(format!("{}", FormatVersion::V2), "V2");
        assert_eq!(format!("{}", FormatVersion::V3), "V3");
    }

    #[test]
    fn test_version_changes() {
        let changes = VersionUtils::version_changes(FormatVersion::Legacy, FormatVersion::V1);
        assert!(changes.contains(&"Added explicit version header".to_string()));

        let changes = VersionUtils::version_changes(FormatVersion::V1, FormatVersion::V2);
        assert!(changes.contains(&"Enhanced compression algorithm".to_string()));

        let changes = VersionUtils::version_changes(FormatVersion::V2, FormatVersion::V3);
        assert!(changes.iter().any(|c| c.contains("Varint encoding")));
        assert!(changes.iter().any(|c| c.contains("20-30%")));
    }

    #[test]
    fn test_detect_version() {
        let data = b"ZDB\x01some data";
        let version = VersionUtils::detect_version(data).unwrap();
        assert_eq!(version, FormatVersion::V1);

        let bad_data = b"BAD\x01";
        assert!(VersionUtils::detect_version(bad_data).is_err());
    }

    #[test]
    fn test_validate_compatibility() {
        let result = VersionUtils::validate_compatibility(FormatVersion::V1, FormatVersion::V1);
        assert!(result.is_ok());

        let result = VersionUtils::validate_compatibility(FormatVersion::V1, FormatVersion::V2);
        assert!(result.is_err());
    }

    #[test]
    fn test_supported_versions() {
        let versions = FormatVersion::supported_versions();
        assert_eq!(versions.len(), 4);
        assert!(versions.contains(&FormatVersion::Legacy));
        assert!(versions.contains(&FormatVersion::V1));
        assert!(versions.contains(&FormatVersion::V2));
        assert!(versions.contains(&FormatVersion::V3));
    }

    #[test]
    fn test_version_error_handling() {
        let err = ZdbVersionError::UnsupportedVersion {
            found: 99,
            supported: vec![FormatVersion::V1 as u8, FormatVersion::V2 as u8],
            offset: None,
            key: None,
        };
        let msg = format!("{err}");
        assert!(msg.contains("version") || msg.contains("Unsupported"));
        assert!(msg.contains("99"));

        let version_err = ZdbVersionError::UnsupportedVersion {
            found: 99,
            supported: vec![FormatVersion::V1 as u8],
            offset: None,
            key: None,
        };
        let io_err: std::io::Error = ZdbError::from(version_err).into();
        assert_eq!(io_err.kind(), std::io::ErrorKind::Unsupported);
    }
}
