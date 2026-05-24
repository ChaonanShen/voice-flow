use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostprocessDecision {
    Accept,
    Reject(String),
}

pub fn validate_rewrite(original: &str, rewritten: &str) -> PostprocessDecision {
    let original = original.trim();
    let rewritten = rewritten.trim();

    if rewritten.is_empty() {
        return PostprocessDecision::Reject("rewrite output is empty".to_string());
    }

    let original_chars = original.chars().count();
    let rewritten_chars = rewritten.chars().count();
    if original_chars > 20 && rewritten_chars * 10 < original_chars * 3 {
        return PostprocessDecision::Reject(format!(
            "rewrite output too short: {rewritten_chars} chars from {original_chars}"
        ));
    }

    for number in ascii_numbers(original) {
        if !rewritten.contains(&number) && !contains_chinese_number(rewritten, &number) {
            return PostprocessDecision::Reject(format!("missing number `{number}`"));
        }
    }

    for token in proper_nouns(original) {
        if !rewritten.contains(&token) {
            return PostprocessDecision::Reject(format!("missing proper noun `{token}`"));
        }
    }

    PostprocessDecision::Accept
}

fn ascii_numbers(text: &str) -> Vec<String> {
    number_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn proper_nouns(text: &str) -> Vec<String> {
    proper_noun_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn number_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d+").expect("valid number regex"))
}

fn proper_noun_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[A-Z][A-Za-z0-9_-]*\b").expect("valid proper noun regex"))
}

fn contains_chinese_number(text: &str, number: &str) -> bool {
    let Ok(value) = number.parse::<u32>() else {
        return false;
    };
    match value {
        0 => text.contains('零'),
        1 => text.contains('一'),
        2 => text.contains('二') || text.contains('两'),
        3 => text.contains('三'),
        4 => text.contains('四'),
        5 => text.contains('五'),
        6 => text.contains('六'),
        7 => text.contains('七'),
        8 => text.contains('八'),
        9 => text.contains('九'),
        10 => text.contains('十'),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_overly_short_output_for_long_input() {
        let decision = validate_rewrite(
            "今天下午三点我们要开一个比较长的项目复盘会议，讨论语音输入和改写引擎的后续计划",
            "开会",
        );

        assert!(matches!(decision, PostprocessDecision::Reject(_)));
    }

    #[test]
    fn accepts_short_output_for_short_input() {
        let decision = validate_rewrite("下午开会", "开会");

        assert_eq!(decision, PostprocessDecision::Accept);
    }

    #[test]
    fn rejects_missing_ascii_number() {
        let decision = validate_rewrite("下午 3 点开会", "下午开会");

        assert!(matches!(decision, PostprocessDecision::Reject(reason) if reason.contains("3")));
    }

    #[test]
    fn accepts_chinese_number_equivalent() {
        let decision = validate_rewrite("下午 3 点开会", "下午三点开会");

        assert_eq!(decision, PostprocessDecision::Accept);
    }

    #[test]
    fn rejects_missing_proper_noun() {
        let decision = validate_rewrite("Claude 比 GPT 好用", "它比那个好用");

        assert!(
            matches!(decision, PostprocessDecision::Reject(reason) if reason.contains("Claude"))
        );
    }
}
