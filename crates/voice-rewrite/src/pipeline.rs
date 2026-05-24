use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::RewriteError;
use crate::llm::{ChatRequest, LlmClient, ResponseFormat};
use crate::postprocess::{validate_rewrite, PostprocessDecision};
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
    pub variants: HashMap<String, String>,
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
            variants: HashMap::new(),
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
        let selected_profile = preprocessed.command_profile.unwrap_or(ctx.default_profile);
        let mut trace = RewriteTrace::new(selected_profile)
            .with_preprocess_duration(preprocess_started.elapsed());

        if !selected_profile.should_call_llm() || preprocessed.cleaned_text.chars().count() < 5 {
            return Ok(RewriteResult {
                main: preprocessed.cleaned_text,
                variants: HashMap::new(),
                trace,
            });
        }

        let Some(system) = system_prompt(selected_profile) else {
            return Ok(RewriteResult {
                main: preprocessed.cleaned_text,
                variants: HashMap::new(),
                trace,
            });
        };

        trace.mark_llm_called();
        let llm_started = Instant::now();
        let response_format = if selected_profile == Profile::Multi {
            ResponseFormat::JsonObject
        } else {
            ResponseFormat::Text
        };
        let result = self
            .llm
            .complete(ChatRequest {
                model: ctx.model,
                system: system.to_string(),
                user: preprocessed.cleaned_text.clone(),
                response_format,
                temperature: temperature_for_profile(selected_profile),
                max_tokens: max_tokens_for_profile(selected_profile),
                timeout: ctx.timeout,
            })
            .await;
        trace.set_llm_duration(llm_started.elapsed());

        match result {
            Ok(response) => {
                if selected_profile == Profile::Multi {
                    match parse_multi_response(&response.content) {
                        Ok(mut variants) => {
                            for value in variants.values_mut() {
                                if let PostprocessDecision::Reject(_) =
                                    validate_rewrite(&preprocessed.cleaned_text, value)
                                {
                                    value.clear();
                                }
                            }
                            let main = variants.get("clean").cloned().unwrap_or_default();
                            if main.trim().is_empty() {
                                trace.mark_fallback(
                                    "multi response missing clean output or failed postprocess",
                                );
                                return Ok(fallback(preprocessed.cleaned_text, trace));
                            }
                            return Ok(RewriteResult {
                                main,
                                variants,
                                trace,
                            });
                        }
                        Err(error) => {
                            trace.mark_fallback(error);
                            return Ok(fallback(preprocessed.cleaned_text, trace));
                        }
                    }
                }

                let rewritten = response.content.trim().to_string();
                match validate_rewrite(&preprocessed.cleaned_text, &rewritten) {
                    PostprocessDecision::Accept => {}
                    PostprocessDecision::Reject(reason) => {
                        trace.mark_fallback(reason);
                        return Ok(fallback(preprocessed.cleaned_text, trace));
                    }
                }
                Ok(RewriteResult {
                    main: rewritten,
                    variants: HashMap::new(),
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
        variants: HashMap::new(),
        trace,
    }
}

fn parse_multi_response(content: &str) -> Result<HashMap<String, String>, String> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| format!("failed to parse multi profile JSON: {e}"))?;
    let Some(object) = value.as_object() else {
        return Err("multi profile JSON must be an object".to_string());
    };

    let mut variants = HashMap::new();
    for key in ["clean", "polish", "wechat", "bullets"] {
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

fn temperature_for_profile(profile: Profile) -> f32 {
    match profile {
        Profile::Multi => 0.5,
        Profile::Polish | Profile::Email | Profile::Wechat | Profile::Prompt => 0.4,
        _ => 0.3,
    }
}

fn max_tokens_for_profile(profile: Profile) -> u32 {
    match profile {
        Profile::Multi => 800,
        Profile::Email | Profile::Bullets | Profile::Prompt => 700,
        Profile::Commit => 200,
        _ => 500,
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
                "我今天下午可能因地铁晚点而晚到十分钟，请老师不必等我。",
                "润色",
            ),
            (
                Profile::Email,
                "老师您好：\n\n今天下午我可能因为地铁晚点会晚到十分钟，请您不必等我。\n\n谢谢老师。",
                "老师您好：\n\n今天下午我可能因为地铁晚点会晚到十分钟，请您不必等我。\n\n谢谢老师。",
                "邮件",
            ),
            (
                Profile::Wechat,
                "老师，今天下午地铁可能晚点，我会晚到十分钟，您不用等我。",
                "老师，今天下午地铁可能晚点，我会晚到十分钟，您不用等我。",
                "微信",
            ),
            (
                Profile::Bullets,
                "- 今天下午可能晚到十分钟\n- 原因是地铁晚点\n- 请老师不要等我",
                "- 今天下午可能晚到十分钟\n- 原因是地铁晚点\n- 请老师不要等我",
                "要点",
            ),
            (
                Profile::Commit,
                "feat(rewrite): add clean profile rewrite pipeline",
                "feat(rewrite): add clean profile rewrite pipeline",
                "Conventional Commit",
            ),
            (
                Profile::Prompt,
                "目标：实现一个语音输入改写管道。\n约束：保持 ASR 与 rewrite 解耦，并补充 mock 测试。\n输出：可运行的 Rust 代码和测试。",
                "目标：实现一个语音输入改写管道。\n约束：保持 ASR 与 rewrite 解耦，并补充 mock 测试。\n输出：可运行的 Rust 代码和测试。",
                "AI prompt",
            ),
            (
                Profile::Multi,
                r#"{"clean":"clean text","polish":"polish text","wechat":"wechat text","bullets":"- bullet"}"#,
                "clean text",
                "JSON",
            ),
        ];

        for (profile, llm_output, expected, prompt_marker) in cases {
            let mock = MockLlmClient::ok(llm_output);
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
    async fn voice_command_overrides_default_profile() {
        let mock = MockLlmClient::ok("老师您好：\n\n今天下午我会晚到十分钟，请您不必等我。");
        let pipeline = LlmRewritePipeline::new(mock.clone());
        let result = pipeline
            .process(
                "写成邮件，下午晚到十分钟，让老师不要等我",
                context(Profile::Clean),
            )
            .await
            .unwrap();

        assert_eq!(
            result.main,
            "老师您好：\n\n今天下午我会晚到十分钟，请您不必等我。"
        );
        assert_eq!(result.trace.profile, Profile::Email);
        let calls = mock.calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].system.contains("邮件"));
        assert_eq!(calls[0].user, "下午晚到十分钟，让老师不要等我");
    }

    #[tokio::test]
    async fn multi_profile_parses_json_variants() {
        let mock = MockLlmClient::ok(
            r#"{"clean":"我会晚到十分钟。","polish":"我可能会晚到十分钟，请不必等我。","wechat":"我会晚到十分钟，别等我啦。","bullets":"- 晚到十分钟\n- 不用等"}"#,
        );
        let pipeline = LlmRewritePipeline::new(mock.clone());
        let result = pipeline
            .process("嗯我会晚到十分钟让他别等我", context(Profile::Multi))
            .await
            .unwrap();

        assert_eq!(result.main, "我会晚到十分钟。");
        assert_eq!(
            result.variants["polish"],
            "我可能会晚到十分钟，请不必等我。"
        );
        assert_eq!(result.variants["wechat"], "我会晚到十分钟，别等我啦。");
        assert_eq!(result.variants["bullets"], "- 晚到十分钟\n- 不用等");
        let calls = mock.calls();
        assert_eq!(calls[0].response_format, ResponseFormat::JsonObject);
        assert_eq!(calls[0].temperature, 0.5);
        assert_eq!(calls[0].max_tokens, 800);
    }

    #[tokio::test]
    async fn multi_profile_falls_back_on_invalid_json() {
        let mock = MockLlmClient::ok("不是 JSON");
        let pipeline = LlmRewritePipeline::new(mock);
        let result = pipeline
            .process("嗯我会晚到十分钟让他别等我", context(Profile::Multi))
            .await
            .unwrap();

        assert_eq!(result.main, "我会晚到十分钟让他别等我");
        assert!(result.trace.fallback);
        assert!(result
            .trace
            .error
            .unwrap()
            .contains("failed to parse multi profile JSON"));
    }

    #[tokio::test]
    async fn multi_profile_allows_missing_optional_fields() {
        let mock = MockLlmClient::ok(r#"{"clean":"我会晚到十分钟。"}"#);
        let pipeline = LlmRewritePipeline::new(mock);
        let result = pipeline
            .process("嗯我会晚到十分钟让他别等我", context(Profile::Multi))
            .await
            .unwrap();

        assert_eq!(result.main, "我会晚到十分钟。");
        assert_eq!(result.variants["polish"], "");
        assert_eq!(result.variants["wechat"], "");
        assert_eq!(result.variants["bullets"], "");
        assert!(!result.trace.fallback);
    }

    #[tokio::test]
    async fn postprocess_falls_back_when_rewrite_drops_number() {
        let mock = MockLlmClient::ok("下午开会。");
        let pipeline = LlmRewritePipeline::new(mock);
        let result = pipeline
            .process("下午 3 点开会", context(Profile::Polish))
            .await
            .unwrap();

        assert_eq!(result.main, "下午 3 点开会");
        assert!(result.trace.fallback);
        assert!(result.trace.error.unwrap().contains("missing number"));
    }

    #[tokio::test]
    async fn postprocess_falls_back_when_rewrite_drops_proper_noun() {
        let mock = MockLlmClient::ok("它比那个好用。");
        let pipeline = LlmRewritePipeline::new(mock);
        let result = pipeline
            .process("Claude 比 GPT 好用", context(Profile::Polish))
            .await
            .unwrap();

        assert_eq!(result.main, "Claude 比 GPT 好用");
        assert!(result.trace.fallback);
        assert!(result.trace.error.unwrap().contains("proper noun"));
    }

    #[tokio::test]
    async fn multi_profile_clears_variants_that_fail_postprocess() {
        let mock = MockLlmClient::ok(
            r#"{"clean":"下午 3 点开会。","polish":"下午开会。","wechat":"下午 3 点开会","bullets":"- 下午 3 点开会"}"#,
        );
        let pipeline = LlmRewritePipeline::new(mock);
        let result = pipeline
            .process("下午 3 点开会", context(Profile::Multi))
            .await
            .unwrap();

        assert_eq!(result.main, "下午 3 点开会。");
        assert_eq!(result.variants["polish"], "");
        assert_eq!(result.variants["wechat"], "下午 3 点开会");
        assert_eq!(result.variants["bullets"], "- 下午 3 点开会");
        assert!(!result.trace.fallback);
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
