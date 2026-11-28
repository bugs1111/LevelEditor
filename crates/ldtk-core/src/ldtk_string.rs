//! LDtk's string escaping inside JSON string values, after JSON decoding.
use std::borrow::Cow;

pub fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\n', "\\n")
}

/// Decode one LDtk escaping layer, preserving unknown escapes and trailing backslashes.
pub fn unescape(text: &str) -> Cow<'_, str> {
    decode(text, false)
}

fn decode(text: &str, keep_raw_on_unknown: bool) -> Cow<'_, str> {
    let Some(first) = text.find('\\') else {
        return Cow::Borrowed(text);
    };
    let mut result = String::with_capacity(text.len());
    result.push_str(&text[..first]);
    let mut rest = &text[first..];
    while let Some((prefix, escaped)) = rest.split_once('\\') {
        result.push_str(prefix);
        if let Some(tail) = escaped.strip_prefix('n') {
            result.push('\n');
            rest = tail;
        } else if let Some(tail) = escaped.strip_prefix('\\') {
            result.push('\\');
            rest = tail;
        } else if keep_raw_on_unknown {
            return Cow::Borrowed(text);
        } else {
            result.push('\\');
            rest = escaped;
        }
    }
    result.push_str(rest);
    Cow::Owned(result)
}

/// Older Level Editor builds saved tile text directly. Prefer the LDtk representation
/// when both interpretations are valid, and preserve identifiable raw legacy data.
pub fn tile_data_text(stored: &str) -> Cow<'_, str> {
    if stored.contains('\n') || !stored.contains('\\') {
        return Cow::Borrowed(stored);
    }
    if stored.trim_start().starts_with(['{', '[', '"']) {
        tile_data_json(stored).0
    } else {
        decode(stored, true)
    }
}

/// Return the source text and its parsed value so Tiled can reuse the parse.
pub(crate) fn tile_data_json(
    stored: &str,
) -> (Cow<'_, str>, serde_json::Result<serde_json::Value>) {
    let decoded = if stored.contains('\n') {
        Cow::Borrowed(stored)
    } else {
        decode(stored, true)
    };
    let parsed = serde_json::from_str(&decoded);
    if parsed.is_err()
        && matches!(decoded, Cow::Owned(_))
        && let Ok(legacy) = serde_json::from_str(stored)
    {
        return (Cow::Borrowed(stored), Ok(legacy));
    }
    (decoded, parsed)
}

#[cfg(test)]
#[test]
fn freeform_tile_data_accepts_legacy_raw_paths_and_round_trips_encoded_text() {
    for raw in [r"C:\tiles\new", "甲\n乙", r"note:\new\"] {
        assert_eq!(tile_data_text(raw), raw);
        assert_eq!(tile_data_text(&escape(raw)), raw);
    }
}
