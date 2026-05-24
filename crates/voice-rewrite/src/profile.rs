use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Off,
    Clean,
}

impl Profile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Clean => "clean",
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
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(Profile::Off.label(), "off");
        assert_eq!(Profile::Clean.label(), "clean");
    }
}
