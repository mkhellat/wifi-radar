//! MAC helpers.

/// Normalize a MAC to lowercase colon-separated hex (`aa:bb:cc:dd:ee:ff`).
///
/// Accepts `aa-bb-…`, `aabbcc…`, and mixed case. Returns lowercase with colons
/// when 12 hex digits can be extracted; otherwise returns a lowercased trim.
pub fn normalize_mac(raw: &str) -> String {
    let hex: String = raw
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if hex.len() != 12 {
        return raw.trim().to_ascii_lowercase();
    }
    let bytes = hex.as_bytes();
    format!(
        "{}{}:{}{}:{}{}:{}{}:{}{}:{}{}",
        bytes[0] as char,
        bytes[1] as char,
        bytes[2] as char,
        bytes[3] as char,
        bytes[4] as char,
        bytes[5] as char,
        bytes[6] as char,
        bytes[7] as char,
        bytes[8] as char,
        bytes[9] as char,
        bytes[10] as char,
        bytes[11] as char,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_dashed_and_bare() {
        assert_eq!(normalize_mac("AA-BB-CC-DD-EE-FF"), "aa:bb:cc:dd:ee:ff");
        assert_eq!(normalize_mac("aabbccddeeff"), "aa:bb:cc:dd:ee:ff");
        assert_eq!(normalize_mac("aa:bb:cc:dd:ee:ff"), "aa:bb:cc:dd:ee:ff");
    }
}
