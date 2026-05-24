use std::collections::HashMap;

pub const MULTI_VARIANT_KEYS: [&str; 4] = ["clean", "polish", "wechat", "bullets"];

pub fn parse_multi_response(content: &str) -> Result<HashMap<String, String>, String> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| format!("failed to parse multi profile JSON: {e}"))?;
    let Some(object) = value.as_object() else {
        return Err("multi profile JSON must be an object".to_string());
    };

    let mut variants = HashMap::new();
    for key in MULTI_VARIANT_KEYS {
        let value = object
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        variants.insert(key.to_string(), value);
    }
    Ok(variants)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_expected_variant_fields() {
        let variants = parse_multi_response(
            r#"{"clean":" clean ","polish":"polish","wechat":"wechat","bullets":"- item"}"#,
        )
        .unwrap();

        assert_eq!(variants["clean"], "clean");
        assert_eq!(variants["polish"], "polish");
        assert_eq!(variants["wechat"], "wechat");
        assert_eq!(variants["bullets"], "- item");
    }

    #[test]
    fn missing_fields_become_empty_strings() {
        let variants = parse_multi_response(r#"{"clean":"text"}"#).unwrap();

        assert_eq!(variants["clean"], "text");
        assert_eq!(variants["polish"], "");
        assert_eq!(variants["wechat"], "");
        assert_eq!(variants["bullets"], "");
    }

    #[test]
    fn non_string_fields_become_empty_strings() {
        let variants = parse_multi_response(r#"{"clean":"text","polish":123}"#).unwrap();

        assert_eq!(variants["clean"], "text");
        assert_eq!(variants["polish"], "");
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_multi_response("not json").unwrap_err();

        assert!(err.contains("failed to parse multi profile JSON"));
    }

    #[test]
    fn rejects_non_object_json() {
        let err = parse_multi_response(r#"["clean"]"#).unwrap_err();

        assert_eq!(err, "multi profile JSON must be an object");
    }
}
