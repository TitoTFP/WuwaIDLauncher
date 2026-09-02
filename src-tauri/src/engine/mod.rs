pub const MAX_UID_TEXT_LENGTH: usize = 64;

pub fn validate_uid_text(value: &str) -> Result<(), String> {
    if value.chars().count() > MAX_UID_TEXT_LENGTH {
        return Err(format!(
            "uid_text_invalid: teks UID maksimal {MAX_UID_TEXT_LENGTH} karakter"
        ));
    }
    if value
        .chars()
        .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
    {
        return Err(
            "uid_text_invalid: teks UID tidak boleh memuat karakter kontrol atau pemisah baris"
                .to_string(),
        );
    }
    Ok(())
}

pub fn is_effectively_empty_uid_text(value: &str) -> bool {
    value
        .chars()
        .all(|character| character == '\u{FEFF}' || character.is_whitespace())
}

pub mod active_player;
pub mod atom_feed;
pub mod downloader;
pub mod elevation;
pub mod installer;
pub mod media;
pub mod metadata;
pub mod method;
pub mod operations;
pub mod pak;
pub mod patch_asset;
pub mod patch_status;
pub mod path;
pub mod repak;
pub mod runtime;
pub mod settings;
pub mod signature;
pub mod updater;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_customization_validator_rejects_line_separators() {
        assert!(validate_uid_text(&"界".repeat(64)).is_ok());
        assert!(validate_uid_text(&"界".repeat(65)).is_err());
        for value in [
            "Halo\nNozomi",
            "Halo\rNozomi",
            "Halo\u{0000}Nozomi",
            "Halo\u{0085}Nozomi",
            "Halo\u{2028}Nozomi",
            "Halo\u{2029}Nozomi",
        ] {
            assert!(
                validate_uid_text(value).is_err(),
                "accepted invalid UID text: {value:?}"
            );
        }
    }

    #[test]
    fn uid_customization_effective_empty_includes_bom() {
        assert!(is_effectively_empty_uid_text(""));
        assert!(is_effectively_empty_uid_text(" \u{FEFF}\t"));
        assert!(!is_effectively_empty_uid_text("Halo\u{FEFF}"));
    }
}
