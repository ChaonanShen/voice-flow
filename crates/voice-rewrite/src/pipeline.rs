use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::RewriteError;
use crate::llm::{ChatRequest, LlmClient, ResponseFormat};
use crate::preprocess::{Preprocessor, UserDictionary};
use crate::profile::Profile;
use crate::prompts::system_prompt;
use crate::trace::RewriteTrace;

#[async_trait]
pub trait RewritePipeline: Send + Sync {
    async fn process(&self, text: &str, ctx: RewriteContext)
        -> Result<RewriteResult, RewriteError>;
}

#[derive(Clone)]
pub struct RewriteContext {
    pub default_profile: Profile,
    pub user_dictionary: Arc<UserDictionary>,
    pub model: String,
    pub timeout: Duration,
}

impl RewriteContext {
    pub fn clean(model: impl Into<String>, timeout: Duration) -> Self {
        Self {
            default_profile: Profile::Clean,
            user_dictionary: Arc::new(UserDictionary::default()),
            model: model.into(),
            timeout,
        }
    }

    pub fn off() -> Self {
        Self {
            default_profile: Profile::Off,
            user_dictionary: Arc::new(UserDictionary::default()),
            model: "deepseek-chat".to_string(),
            timeout: Duration::from_secs(4),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewriteResult {
    pub main: String,
    #[serde(default)]
    pub variants: std::collections::HashMap<String, String>,
    pub trace: RewriteTrace,
}

pub struct IdentityRewritePipeline;

#[async_trait]
impl RewritePipeline for IdentityRewritePipeline {
    async fn process(
        &self,
        text: &str,
        ctx: RewriteContext,
    ) -> Result<RewriteResult, RewriteError> {
        let started = Instant::now();
        let pre = Preprocessor::new((*ctx.user_dictionary).clone()).run(text);
        let trace = RewriteTrace::new(Profile::Off).with_preprocess_duration(started.elapsed());
        Ok(RewriteResult {
            main: pre.cleaned_text,
            variants: std::collections::HashMap::new(),
            trace,
        })
    }
}

pub struct LlmRewritePipeline<C> {
    llm: C,
}

impl<C> LlmRewritePipeline<C> {
    pub fn new(llm: C) -> Self {
        Self { llm }
    }

    pub fn llm(&self) -> &C {
        &self.llm
    }
}

#[async_trait]
impl<C> RewritePipeline for LlmRewritePipeline<C>
where
    C: LlmClient,
{
    async fn process(
        &self,
        text: &str,
        ctx: RewriteContext,
    ) -> Result<RewriteResult, RewriteError> {
        let preprocess_started = Instant::now();
        let preprocessor = Preprocessor::new((*ctx.user_dictionary).clone());
        let preprocessed = preprocessor.run(text);
        let mut trace = RewriteTrace::new(ctx.default_profile)
            .with_preprocess_duration(preprocess_started.elapsed());

        if !ctx.default_profile.should_call_llm() || preprocessed.cleaned_text.chars().count() < 5 {
            return Ok(RewriteResult {
                main: preprocessed.cleaned_text,
                variants: std::collections::HashMap::new(),
                trace,
            });
        }

        let Some(system) = system_prompt(ctx.default_profile) else {
            return Ok(RewriteResult {
                main: preprocessed.cleaned_text,
                variants: std::collections::HashMap::new(),
                trace,
            });
        };

        trace.mark_llm_called();
        let llm_started = Instant::now();
        let result = self
            .llm
            .complete(ChatRequest {
                model: ctx.model,
                system: system.to_string(),
                user: preprocessed.cleaned_text.clone(),
                response_format: ResponseFormat::Text,
                timeout: ctx.timeout,
            })
            .await;
        trace.set_llm_duration(llm_started.elapsed());

        match result {
            Ok(response) => {
                let rewritten = response.content.trim().to_string();
                if rewritten.is_empty() {
                    trace.mark_fallback("llm returned empty output");
                    return Ok(fallback(preprocessed.cleaned_text, trace));
                }
                Ok(RewriteResult {
                    main: rewritten,
                    variants: std::collections::HashMap::new(),
                    trace,
                })
            }
            Err(error) => {
                trace.mark_fallback(error.to_string());
                Ok(fallback(preprocessed.cleaned_text, trace))
            }
        }
    }
}

fn fallback(main: String, trace: RewriteTrace) -> RewriteResult {
    RewriteResult {
        main,
        variants: std::collections::HashMap::new(),
        trace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::mock::MockLlmClient;
    use crate::llm::LlmError;

    fn context(profile: Profile) -> RewriteContext {
        RewriteContext {
            default_profile: profile,
            user_dictionary: Arc::new(UserDictionary::default()),
            model: "deepseek-chat".to_string(),
            timeout: Duration::from_secs(4),
        }
    }

    #[tokio::test]
    async fn off_profile_does_not_call_llm() {
        let mock = MockLlmClient::ok("should not be used");
        let pipeline = LlmRewritePipeline::new(mock.clone());
        let result = pipeline
            .process("嗯我今天有点忙", RewriteContext::off())
            .await
            .unwrap();

        assert_eq!(result.main, "我今天有点忙");
        assert_eq!(mock.call_count(), 0);
        assert!(!result.trace.llm_called);
    }

    #[tokio::test]
    async fn clean_profile_uses_llm_output() {
        let mock = MockLlmClient::ok("我今天下午会晚到十分钟。");
        let pipeline = LlmRewritePipeline::new(mock.clone());
        let result = pipeline
            .process("嗯我今天下午会晚到十分钟", context(Profile::Clean))
            .await
            .unwrap();

        assert_eq!(result.main, "我今天下午会晚到十分钟。");
        assert_eq!(mock.call_count(), 1);
        assert!(result.trace.llm_called);
        assert!(!result.trace.fallback);
    }

    #[tokio::test]
    async fn text_profiles_send_profile_specific_prompts() {
        let cases = [
            (
                Profile::Polish,
                "我今天下午可能因地铁晚点而晚到十分钟，请老师不必等我。",
                "润色",
            ),
            (
                Profile::Email,
                "老师您好：\n\n今天下午我可能因为地铁晚点会晚到十分钟，请您不必等我。\n\n谢谢老师。",
                "邮件",
            ),
            (
                Profile::Wechat,
                "老师，今天下午地铁可能晚点，我会晚到十分钟，您不用等我。",
                "微信",
            ),
            (
                Profile::Bullets,
                "- 今天下午可能晚到十分钟\n- 原因是地铁晚点\n- 请老师不要等我",
                "要点",
            ),
            (
                Profile::Commit,
                "feat(rewrite): add clean profile rewrite pipeline",
                "Conventional Commit",
            ),
            (
                Profile::Prompt,
                "目标：实现一个语音输入改写管道。\n约束：保持 ASR 与 rewrite 解耦，并补充 mock 测试。\n输出：可运行的 Rust 代码和测试。",
                "AI prompt",
            ),
        ];

        for (profile, expected, prompt_marker) in cases {
            let mock = MockLlmClient::ok(expected);
            let pipeline = LlmRewritePipeline::new(mock.clone());
            let result = pipeline
                .process(
                    "嗯我今天下午可能因为地铁晚点会晚到十分钟，让老师不要等我",
                    context(profile),
                )
                .await
                .unwrap();

            assert_eq!(result.main, expected, "profile={}", profile.label());
            assert!(result.trace.llm_called);
            let calls = mock.calls();
            assert_eq!(calls.len(), 1);
            assert!(
                calls[0].system.contains(prompt_marker),
                "profile={} system prompt should contain `{}`: {}",
                profile.label(),
                prompt_marker,
                calls[0].system
            );
        }
    }

    #[tokio::test]
    async fn llm_error_falls_back_to_preprocessed_text() {
        let mock = MockLlmClient::error(LlmError::Network("down".to_string()));
        let pipeline = LlmRewritePipeline::new(mock);
        let result = pipeline
            .process(
                "嗯我今天下午会晚到十分钟",
                RewriteContext::clean("deepseek-chat", Duration::from_secs(4)),
            )
            .await
            .unwrap();

        assert_eq!(result.main, "我今天下午会晚到十分钟");
        assert!(result.trace.fallback);
        assert!(result.trace.error.unwrap().contains("network error"));
    }

    #[tokio::test]
    async fn empty_llm_output_falls_back() {
        let mock = MockLlmClient::ok("   ");
        let pipeline = LlmRewritePipeline::new(mock);
        let result = pipeline
            .process(
                "嗯我今天下午会晚到十分钟",
                RewriteContext::clean("deepseek-chat", Duration::from_secs(4)),
            )
            .await
            .unwrap();

        assert_eq!(result.main, "我今天下午会晚到十分钟");
        assert!(result.trace.fallback);
    }
}
