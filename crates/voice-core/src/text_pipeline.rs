//! Text-domain glue for ASR transcript post-processing.
//!
//! This module intentionally knows nothing about audio, clipboard, paste, or UI.

use std::sync::Arc;
use std::time::Duration;

use voice_rewrite::{
    IdentityRewritePipeline, RewriteContext, RewriteError, RewritePipeline, RewriteResult,
    RewriteSettings, UserDictionary,
};

use crate::config::RewriteConfig;

pub async fn process_transcript<P>(
    transcript: &str,
    config: &RewriteConfig,
    rewrite_pipeline: Option<&P>,
) -> Result<RewriteResult, RewriteError>
where
    P: RewritePipeline + ?Sized,
{
    let context = rewrite_context_from_config(config);
    if !config.enabled {
        return IdentityRewritePipeline.process(transcript, context).await;
    }

    match rewrite_pipeline {
        Some(pipeline) => pipeline.process(transcript, context).await,
        None => IdentityRewritePipeline.process(transcript, context).await,
    }
}

pub fn rewrite_settings_from_config(config: &RewriteConfig) -> RewriteSettings {
    RewriteSettings {
        enabled: config.enabled,
        provider: config.provider,
        default_profile: config.default_profile,
        model: config.model.clone(),
        timeout: timeout_from_config(config),
        user_dictionary: UserDictionary::from_hash_map(config.user_dictionary.clone()),
        api_key: None,
        load_env: true,
    }
}

fn rewrite_context_from_config(config: &RewriteConfig) -> RewriteContext {
    let settings = rewrite_settings_from_config(config);
    RewriteContext {
        default_profile: settings.default_profile,
        user_dictionary: Arc::new(settings.user_dictionary),
        model: settings
            .model
            .or_else(|| settings.provider.default_model().map(str::to_string))
            .unwrap_or_default(),
        timeout: settings.timeout,
    }
}

fn timeout_from_config(config: &RewriteConfig) -> Duration {
    Duration::from_millis(config.timeout_ms.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use voice_rewrite::{Profile, RewriteTrace};

    #[derive(Default)]
    struct MockRewrite {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl RewritePipeline for MockRewrite {
        async fn process(
            &self,
            text: &str,
            ctx: RewriteContext,
        ) -> Result<RewriteResult, RewriteError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(RewriteResult {
                main: format!("{}:{}", ctx.default_profile.label(), text),
                variants: HashMap::new(),
                trace: RewriteTrace::new(ctx.default_profile),
            })
        }
    }

    #[tokio::test]
    async fn disabled_config_uses_identity_pipeline() {
        let rewrite = MockRewrite::default();
        let result =
            process_transcript("嗯我今天有点忙", &RewriteConfig::default(), Some(&rewrite))
                .await
                .unwrap();

        assert_eq!(result.main, "我今天有点忙");
        assert_eq!(result.trace.profile, Profile::Off);
        assert_eq!(rewrite.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn enabled_config_calls_rewrite_pipeline() {
        let rewrite = MockRewrite::default();
        let config = RewriteConfig {
            enabled: true,
            default_profile: Profile::Email,
            ..RewriteConfig::default()
        };
        let result = process_transcript("下午晚到十分钟", &config, Some(&rewrite))
            .await
            .unwrap();

        assert_eq!(result.main, "email:下午晚到十分钟");
        assert_eq!(rewrite.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn enabled_without_pipeline_falls_back_to_identity() {
        let config = RewriteConfig {
            enabled: true,
            default_profile: Profile::Clean,
            ..RewriteConfig::default()
        };
        let result = process_transcript::<MockRewrite>("嗯下午晚到十分钟", &config, None)
            .await
            .unwrap();

        assert_eq!(result.main, "下午晚到十分钟");
        assert_eq!(result.trace.profile, Profile::Off);
    }

    #[test]
    fn builds_rewrite_settings_from_config() {
        let config = RewriteConfig {
            enabled: true,
            default_profile: Profile::Prompt,
            timeout_ms: 2_500,
            user_dictionary: HashMap::from([("克劳德".to_string(), "Claude".to_string())]),
            ..RewriteConfig::default()
        };

        let settings = rewrite_settings_from_config(&config);

        assert!(settings.enabled);
        assert_eq!(settings.default_profile, Profile::Prompt);
        assert_eq!(settings.timeout, Duration::from_millis(2_500));
        assert!(!settings.user_dictionary.is_empty());
    }
}
