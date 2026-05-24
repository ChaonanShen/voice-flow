use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserDictionary {
    entries: Vec<(String, String)>,
}

impl UserDictionary {
    pub fn new(entries: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut entries: Vec<(String, String)> = entries
            .into_iter()
            .filter(|(from, _)| !from.is_empty())
            .collect();
        entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        Self { entries }
    }

    pub fn from_hash_map(entries: HashMap<String, String>) -> Self {
        Self::new(entries)
    }

    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (from, to) in &self.entries {
            out = out.replace(from, to);
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Preprocessor {
    dictionary: UserDictionary,
}

impl Preprocessor {
    pub fn new(dictionary: UserDictionary) -> Self {
        Self { dictionary }
    }

    pub fn run(&self, text: &str) -> PreprocessOutput {
        let without_fillers = remove_fillers(text);
        let cleaned_text = self.dictionary.apply(&without_fillers);
        PreprocessOutput {
            cleaned_text: normalize_spaces(cleaned_text.trim()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreprocessOutput {
    pub cleaned_text: String,
}

fn remove_fillers(text: &str) -> String {
    filler_regex()
        .replace_all(text, "")
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '，' | ',' | '。' | '.'))
        .to_string()
}

fn filler_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(嗯+|啊+|呃+|额+|那个就是说|就是说|那个|这个|就是|那么)")
            .expect("valid filler regex")
    })
}

fn normalize_spaces(text: &str) -> String {
    space_regex().replace_all(text, " ").trim().to_string()
}

fn space_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").expect("valid whitespace regex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_chinese_fillers() {
        let pre = Preprocessor::default();

        assert_eq!(pre.run("嗯我今天有点忙").cleaned_text, "我今天有点忙");
        assert_eq!(
            pre.run("那个就是说我们明天再聊").cleaned_text,
            "我们明天再聊"
        );
        assert_eq!(pre.run("啊啊啊我想想啊").cleaned_text, "我想想");
    }

    #[test]
    fn leaves_plain_text_unchanged() {
        let pre = Preprocessor::default();
        assert_eq!(pre.run("没有口头禅的句子").cleaned_text, "没有口头禅的句子");
    }

    #[test]
    fn applies_dictionary_after_fillers() {
        let pre = Preprocessor::new(UserDictionary::new([(
            "克劳德".to_string(),
            "Claude".to_string(),
        )]));

        assert_eq!(pre.run("那个克劳德还挺好用").cleaned_text, "Claude还挺好用");
    }

    #[test]
    fn ignores_empty_dictionary_keys() {
        let pre = Preprocessor::new(UserDictionary::new([
            ("".to_string(), "X".to_string()),
            ("我推".to_string(), "Vue".to_string()),
        ]));

        assert_eq!(pre.run("我推很好").cleaned_text, "Vue很好");
    }
}
