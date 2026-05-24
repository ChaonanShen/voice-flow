use std::sync::Arc;
use std::time::Duration;

use voice_rewrite::llm::openai_compat::OpenAiCompatClient;
use voice_rewrite::{
    LlmRewritePipeline, Profile, RewriteContext, RewritePipeline, UserDictionary,
    DEEPSEEK_API_KEY_ENV, DEFAULT_REWRITE_MODEL,
};

const TEACHER_LATE_INPUT: &str =
    "嗯，跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我。";

#[tokio::test]
#[ignore = "requires DEEPSEEK_API_KEY and calls the live DeepSeek API"]
async fn live_deepseek_clean_smoke() {
    let result = rewrite(Profile::Clean, TEACHER_LATE_INPUT, Duration::from_secs(8)).await;

    eprintln!("input:\n{TEACHER_LATE_INPUT}\n\n[clean]\n{}", result.main);
    assert_live_success(&result);
    assert!(!result.main.contains('嗯'));
    assert_keeps_teacher_late_facts(&result.main);
}

#[tokio::test]
#[ignore = "requires DEEPSEEK_API_KEY and calls the live DeepSeek API"]
async fn live_deepseek_multi_smoke() {
    let result = rewrite(Profile::Multi, TEACHER_LATE_INPUT, Duration::from_secs(10)).await;

    eprintln!("input:\n{TEACHER_LATE_INPUT}");
    for key in ["clean", "polish", "wechat", "bullets"] {
        eprintln!("\n[{key}]\n{}", result.variants[key]);
    }

    assert_live_success(&result);
    assert_eq!(result.variants.len(), 4);
    for key in ["clean", "polish", "wechat", "bullets"] {
        assert!(
            !result.variants[key].trim().is_empty(),
            "{key} should not be empty"
        );
    }
    assert_keeps_teacher_late_facts(&result.main);
}

#[tokio::test]
#[ignore = "requires DEEPSEEK_API_KEY and calls the live DeepSeek API"]
async fn live_deepseek_timeout_falls_back() {
    let result = rewrite(Profile::Clean, TEACHER_LATE_INPUT, Duration::from_millis(1)).await;

    assert!(result.trace.fallback);
    assert_eq!(
        result.main,
        "跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我"
    );
}

async fn rewrite(profile: Profile, input: &str, timeout: Duration) -> voice_rewrite::RewriteResult {
    let _ = dotenvy::dotenv();
    let api_key = std::env::var(DEEPSEEK_API_KEY_ENV)
        .unwrap_or_else(|_| panic!("missing {DEEPSEEK_API_KEY_ENV}"));
    let client = OpenAiCompatClient::deepseek(api_key).unwrap();
    let pipeline = LlmRewritePipeline::new(client);
    pipeline
        .process(
            input,
            RewriteContext {
                default_profile: profile,
                user_dictionary: Arc::new(UserDictionary::default()),
                model: DEFAULT_REWRITE_MODEL.to_string(),
                timeout,
            },
        )
        .await
        .unwrap()
}

fn assert_live_success(result: &voice_rewrite::RewriteResult) {
    assert!(!result.trace.fallback, "fallback: {:?}", result.trace.error);
    assert!(result.trace.llm_called);
    assert!(!result.main.trim().is_empty());
}

fn assert_keeps_teacher_late_facts(text: &str) {
    for fact in ["老师", "地铁", "十分钟"] {
        assert!(text.contains(fact), "missing fact `{fact}` in `{text}`");
    }
}
