use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::RewriteError;
use crate::llm::openai_compat::OpenAiCompatClient;
use crate::pipeline::{
    IdentityRewritePipeline, LlmRewritePipeline, RewriteContext, RewritePipeline, RewriteResult,
};
use crate::preprocess::{Preprocessor, UserDictionary};
use crate::profile::Profile;
use crate::trace::RewriteTrace;

pub const DEEPSEEK_API_KEY_ENV: &str = "DEEPSEEK_API_KEY";
pub const DASHSCOPE_API_KEY_ENV: &str = "DASHSCOPE_API_KEY";
pub const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";
pub const DEFAULT_REWRITE_MODEL: &str = "deepseek-chat";
pub const DEFAULT_REWRITE_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewriteProvider {
    DeepSeek,
    DashScope,
    OpenAi,
}

impl RewriteProvider {
    pub fn label(self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek",
            Self::DashScope => "dashscope",
            Self::OpenAi => "openai",
        }
    }

    pub fn api_key_env_var(self) -> &'static str {
        match self {
            Self::DeepSeek => DEEPSEEK_API_KEY_ENV,
            Self::DashScope => DASHSCOPE_API_KEY_ENV,
            Self::OpenAi => OPENAI_API_KEY_ENV,
        }
    }

    pub fn default_model(self) -> Option<&'static str> {
        match self {
            Self::DeepSeek => Some(DEFAULT_REWRITE_MODEL),
            Self::DashScope => Some("qwen-plus"),
            Self::OpenAi => None,
        }
    }

    fn build_client(self, api_key: String) -> Result<OpenAiCompatClient, RewriteError> {
        let client = match self {
            Self::DeepSeek => OpenAiCompatClient::deepseek(api_key),
            Self::DashScope => OpenAiCompatClient::dashscope(api_key),
            Self::OpenAi => OpenAiCompatClient::openai(api_key),
        }?;
        Ok(client)
    }
}

impl Default for RewriteProvider {
    fn default() -> Self {
        Self::DeepSeek
    }
}

impl FromStr for RewriteProvider {
    type Err = RewriteProviderParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "deepseek" => Ok(Self::DeepSeek),
            "dashscope" => Ok(Self::DashScope),
            "openai" | "open_ai" => Ok(Self::OpenAi),
            other => Err(RewriteProviderParseError(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown rewrite provider `{0}`")]
pub struct RewriteProviderParseError(pub String);

#[derive(Debug, Clone)]
pub struct RewriteSettings {
    pub enabled: bool,
    pub provider: RewriteProvider,
    pub default_profile: Profile,
    pub model: Option<String>,
    pub timeout: Duration,
    pub user_dictionary: UserDictionary,
    pub api_key: Option<String>,
    pub load_env: bool,
}

impl RewriteSettings {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    pub fn enabled(default_profile: Profile) -> Self {
        Self {
            enabled: true,
            default_profile,
            ..Self::default()
        }
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn without_env(mut self) -> Self {
        self.load_env = false;
        self
    }

    pub fn build_engine(self) -> Result<RewriteEngine, RewriteError> {
        if !self.enabled || !self.default_profile.should_call_llm() {
            return Ok(RewriteEngine::Disabled {
                context: self.context(Profile::Off, DEFAULT_REWRITE_MODEL.to_string()),
            });
        }

        let provider = self.provider;
        let context = self.context(self.default_profile, self.resolve_model()?);
        let Some(api_key) = self.resolve_api_key() else {
            return Ok(RewriteEngine::MissingApiKey {
                provider,
                env_var: provider.api_key_env_var(),
                context,
            });
        };

        let pipeline = LlmRewritePipeline::new(provider.build_client(api_key)?);
        Ok(RewriteEngine::Llm { pipeline, context })
    }

    fn resolve_model(&self) -> Result<String, RewriteError> {
        if let Some(model) = self
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            return Ok(model.to_string());
        }
        self.provider
            .default_model()
            .map(str::to_string)
            .ok_or_else(|| RewriteError::MissingModel(self.provider.label().to_string()))
    }

    fn resolve_api_key(&self) -> Option<String> {
        if let Some(key) = self
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            return Some(key.to_string());
        }
        if !self.load_env {
            return None;
        }

        let _ = dotenvy::dotenv();
        std::env::var(self.provider.api_key_env_var())
            .ok()
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())
    }

    fn context(&self, default_profile: Profile, model: String) -> RewriteContext {
        RewriteContext {
            default_profile,
            user_dictionary: Arc::new(self.user_dictionary.clone()),
            model,
            timeout: self.timeout,
        }
    }
}

impl Default for RewriteSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: RewriteProvider::DeepSeek,
            default_profile: Profile::Clean,
            model: None,
            timeout: DEFAULT_REWRITE_TIMEOUT,
            user_dictionary: UserDictionary::default(),
            api_key: None,
            load_env: true,
        }
    }
}

pub enum RewriteEngine {
    Disabled {
        context: RewriteContext,
    },
    MissingApiKey {
        provider: RewriteProvider,
        env_var: &'static str,
        context: RewriteContext,
    },
    Llm {
        pipeline: LlmRewritePipeline<OpenAiCompatClient>,
        context: RewriteContext,
    },
}

impl RewriteEngine {
    pub async fn process(&self, text: &str) -> Result<RewriteResult, RewriteError> {
        match self {
            Self::Disabled { context } => {
                IdentityRewritePipeline.process(text, context.clone()).await
            }
            Self::MissingApiKey {
                env_var, context, ..
            } => Ok(fallback_for_missing_key(text, context, *env_var)),
            Self::Llm { pipeline, context } => pipeline.process(text, context.clone()).await,
        }
    }

    pub fn missing_api_key(&self) -> Option<MissingApiKeyInfo> {
        match self {
            Self::MissingApiKey {
                provider, env_var, ..
            } => Some(MissingApiKeyInfo {
                provider: *provider,
                env_var: *env_var,
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissingApiKeyInfo {
    pub provider: RewriteProvider,
    pub env_var: &'static str,
}

fn fallback_for_missing_key(text: &str, context: &RewriteContext, env_var: &str) -> RewriteResult {
    let started = Instant::now();
    let preprocessor = Preprocessor::new((*context.user_dictionary).clone());
    let preprocessed = preprocessor.run(text);
    let selected_profile = preprocessed
        .command_profile
        .unwrap_or(context.default_profile);
    let mut trace = RewriteTrace::new(selected_profile).with_preprocess_duration(started.elapsed());
    trace.mark_fallback(format!("missing API key: {env_var}"));
    RewriteResult {
        main: preprocessed.cleaned_text,
        variants: Default::default(),
        trace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_metadata_is_stable() {
        assert_eq!(RewriteProvider::DeepSeek.label(), "deepseek");
        assert_eq!(
            RewriteProvider::DeepSeek.api_key_env_var(),
            "DEEPSEEK_API_KEY"
        );
        assert_eq!(
            RewriteProvider::DashScope.api_key_env_var(),
            "DASHSCOPE_API_KEY"
        );
        assert_eq!(RewriteProvider::OpenAi.api_key_env_var(), "OPENAI_API_KEY");
        assert_eq!(
            "openai".parse::<RewriteProvider>().unwrap(),
            RewriteProvider::OpenAi
        );
    }

    #[test]
    fn default_settings_keep_rewrite_disabled() {
        let settings = RewriteSettings::default();

        assert!(!settings.enabled);
        assert_eq!(settings.provider, RewriteProvider::DeepSeek);
        assert_eq!(settings.default_profile, Profile::Clean);
        assert_eq!(settings.model, None);
    }

    #[tokio::test]
    async fn disabled_engine_ignores_voice_commands() {
        let engine = RewriteSettings::disabled()
            .without_env()
            .build_engine()
            .unwrap();
        let result = engine.process("写成邮件，下午晚到十分钟").await.unwrap();

        assert_eq!(result.main, "下午晚到十分钟");
        assert_eq!(result.trace.profile, Profile::Off);
        assert!(!result.trace.fallback);
        assert!(!result.trace.llm_called);
    }

    #[tokio::test]
    async fn missing_api_key_falls_back_to_preprocessed_text() {
        let engine = RewriteSettings::enabled(Profile::Clean)
            .without_env()
            .build_engine()
            .unwrap();
        let info = engine.missing_api_key().unwrap();

        assert_eq!(info.provider, RewriteProvider::DeepSeek);
        assert_eq!(info.env_var, DEEPSEEK_API_KEY_ENV);

        let result = engine.process("嗯我今天下午会晚到十分钟").await.unwrap();

        assert_eq!(result.main, "我今天下午会晚到十分钟");
        assert!(result.trace.fallback);
        assert!(result.trace.error.unwrap().contains(DEEPSEEK_API_KEY_ENV));
    }

    #[test]
    fn openai_requires_an_explicit_model() {
        let result = RewriteSettings {
            provider: RewriteProvider::OpenAi,
            model: None,
            api_key: Some("sk-test".to_string()),
            ..RewriteSettings::enabled(Profile::Clean).without_env()
        }
        .build_engine();

        assert!(
            matches!(result, Err(RewriteError::MissingModel(provider)) if provider == "openai")
        );
    }

    #[test]
    fn explicit_api_key_builds_llm_engine() {
        let engine = RewriteSettings::enabled(Profile::Clean)
            .without_env()
            .with_api_key("sk-test")
            .build_engine()
            .unwrap();

        assert!(matches!(engine, RewriteEngine::Llm { .. }));
    }
}
