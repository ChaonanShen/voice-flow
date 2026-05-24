use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::profile::Profile;

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
        let command = detect_command(&without_fillers);
        let command_removed = match &command {
            Some(command) => command.cleaned_text.as_str(),
            None => &without_fillers,
        };
        let cleaned_text = self.dictionary.apply(command_removed);
        PreprocessOutput {
            cleaned_text: normalize_spaces(cleaned_text.trim()),
            command_profile: command.map(|command| command.profile),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreprocessOutput {
    pub cleaned_text: String,
    pub command_profile: Option<Profile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DetectedCommand {
    profile: Profile,
    cleaned_text: String,
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

fn detect_command(text: &str) -> Option<DetectedCommand> {
    for (profile, regex) in command_regexes() {
        if regex.is_match(text) {
            let cleaned_text = regex.replace(text, "").to_string();
            return Some(DetectedCommand {
                profile: *profile,
                cleaned_text: trim_command_punctuation(&cleaned_text),
            });
        }
    }
    None
}

fn command_regexes() -> &'static [(Profile, Regex)] {
    static REGEXES: OnceLock<Vec<(Profile, Regex)>> = OnceLock::new();
    REGEXES.get_or_init(|| {
        vec![
            (
                Profile::Polish,
                Regex::new(r"(改\s*正式\s*一点|正式\s*一点|更\s*正式|语气\s*正式\s*一点)")
                    .expect("valid polish command regex"),
            ),
            (
                Profile::Email,
                Regex::new(r"(写成邮件|改成邮件|写个邮件)").expect("valid email command regex"),
            ),
            (
                Profile::Wechat,
                Regex::new(r"(写成微信|改成微信|聊天语气)").expect("valid wechat command regex"),
            ),
            (
                Profile::Bullets,
                Regex::new(r"(改成要点|写成要点|列点)").expect("valid bullets command regex"),
            ),
            (
                Profile::Commit,
                Regex::new(r"(写成\s*commit|提交信息)").expect("valid commit command regex"),
            ),
            (
                Profile::Prompt,
                Regex::new(r"(写成\s*prompt|改成\s*prompt|提示词)")
                    .expect("valid prompt command regex"),
            ),
            (
                Profile::Clean,
                Regex::new(r"(改短一点|压缩一下|翻译成英文|改成英文)")
                    .expect("valid clean command regex"),
            ),
        ]
    })
}

fn trim_command_punctuation(text: &str) -> String {
    text.trim_matches(|c: char| {
        c.is_whitespace() || matches!(c, '，' | ',' | '。' | '.' | '：' | ':')
    })
    .to_string()
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
        assert_eq!(pre.run("没有口头禅的句子").command_profile, None);
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

    #[test]
    fn detects_voice_commands_and_removes_command_text() {
        let pre = Preprocessor::default();
        let cases = [
            (
                "下午晚到十分钟，改正式一点",
                Profile::Polish,
                "下午晚到十分钟",
            ),
            ("写成邮件，下午晚到十分钟", Profile::Email, "下午晚到十分钟"),
            ("下午晚到，写成微信", Profile::Wechat, "下午晚到"),
            ("改成要点：A B C", Profile::Bullets, "A B C"),
            ("写成 commit 修复录音错误", Profile::Commit, "修复录音错误"),
            (
                "写成 prompt 做一个改写引擎",
                Profile::Prompt,
                "做一个改写引擎",
            ),
        ];

        for (input, profile, cleaned) in cases {
            let out = pre.run(input);
            assert_eq!(out.command_profile, Some(profile), "input={input}");
            assert_eq!(out.cleaned_text, cleaned, "input={input}");
        }
    }

    #[test]
    fn command_detection_tolerates_spaces() {
        let pre = Preprocessor::default();
        let out = pre.run("下午晚到十分钟，改正式 一点");

        assert_eq!(out.command_profile, Some(Profile::Polish));
        assert_eq!(out.cleaned_text, "下午晚到十分钟");
    }
}
