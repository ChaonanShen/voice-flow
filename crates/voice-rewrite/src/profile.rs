use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Off,
    Clean,
    Polish,
    Email,
    Wechat,
    Bullets,
    Commit,
    Prompt,
}

impl Profile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Clean => "clean",
            Self::Polish => "polish",
            Self::Email => "email",
            Self::Wechat => "wechat",
            Self::Bullets => "bullets",
            Self::Commit => "commit",
            Self::Prompt => "prompt",
        }
    }

    pub fn should_call_llm(self) -> bool {
        !matches!(self, Self::Off)
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self::Clean
    }
}

impl FromStr for Profile {
    type Err = ProfileParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "clean" => Ok(Self::Clean),
            "polish" => Ok(Self::Polish),
            "email" => Ok(Self::Email),
            "wechat" => Ok(Self::Wechat),
            "bullets" => Ok(Self::Bullets),
            "commit" => Ok(Self::Commit),
            "prompt" => Ok(Self::Prompt),
            other => Err(ProfileParseError(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown rewrite profile `{0}`")]
pub struct ProfileParseError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_profiles() {
        assert_eq!("off".parse::<Profile>().unwrap(), Profile::Off);
        assert_eq!("clean".parse::<Profile>().unwrap(), Profile::Clean);
        assert_eq!(" CLEAN ".parse::<Profile>().unwrap(), Profile::Clean);
        assert_eq!("polish".parse::<Profile>().unwrap(), Profile::Polish);
        assert_eq!("email".parse::<Profile>().unwrap(), Profile::Email);
        assert_eq!("wechat".parse::<Profile>().unwrap(), Profile::Wechat);
        assert_eq!("bullets".parse::<Profile>().unwrap(), Profile::Bullets);
        assert_eq!("commit".parse::<Profile>().unwrap(), Profile::Commit);
        assert_eq!("prompt".parse::<Profile>().unwrap(), Profile::Prompt);
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(Profile::Off.label(), "off");
        assert_eq!(Profile::Clean.label(), "clean");
        assert_eq!(Profile::Polish.label(), "polish");
        assert_eq!(Profile::Email.label(), "email");
        assert_eq!(Profile::Wechat.label(), "wechat");
        assert_eq!(Profile::Bullets.label(), "bullets");
        assert_eq!(Profile::Commit.label(), "commit");
        assert_eq!(Profile::Prompt.label(), "prompt");
    }
}
