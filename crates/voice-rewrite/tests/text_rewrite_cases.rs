use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use voice_rewrite::llm::{ChatRequest, ChatResponse, LlmClient, LlmError};
use voice_rewrite::{LlmRewritePipeline, Profile, RewriteContext, RewritePipeline, UserDictionary};

#[derive(Clone)]
struct ExampleLlm;

#[async_trait]
impl LlmClient for ExampleLlm {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
        let content = match profile_from_prompt(&req.system) {
            Profile::Clean => clean_example(&req.user),
            Profile::Polish => polish_example(&req.user),
            Profile::Email => email_example(&req.user),
            Profile::Wechat => wechat_example(&req.user),
            Profile::Bullets => bullets_example(&req.user),
            Profile::Commit => commit_example(&req.user),
            Profile::Prompt => prompt_example(&req.user),
            Profile::Multi => multi_example(&req.user),
            Profile::Off => req.user,
        };

        Ok(ChatResponse {
            content,
            model: Some("example-llm".to_string()),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        })
    }
}

#[tokio::test]
async fn demo_teacher_late_message_across_profiles() {
    let input = "嗯，跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我。";

    assert_rewrite(
        Profile::Clean,
        input,
        "跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我",
    )
    .await;
    assert_rewrite(
        Profile::Polish,
        input,
        "我想请老师知悉：今天下午我可能因地铁晚点而晚到十分钟左右，请老师不必等我。",
    )
    .await;
    assert_rewrite(
        Profile::Email,
        input,
        "老师您好：\n\n今天下午我可能因为地铁晚点会晚到十分钟左右，请您不必等我。\n\n谢谢老师。",
    )
    .await;
    assert_rewrite(
        Profile::Wechat,
        input,
        "老师，今天下午地铁可能晚点，我会晚到十分钟左右，您不用等我。",
    )
    .await;
    assert_rewrite(
        Profile::Bullets,
        input,
        "- 今天下午可能晚到十分钟左右\n- 原因是地铁晚点\n- 请老师不要等我",
    )
    .await;

    let multi = rewrite(Profile::Multi, input).await;
    assert_eq!(
        multi.main,
        "跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我。"
    );
    assert_eq!(
        multi.variants["polish"],
        "我想请老师知悉：今天下午我可能因地铁晚点而晚到十分钟左右，请老师不必等我。"
    );
    assert_eq!(
        multi.variants["wechat"],
        "老师，今天下午地铁可能晚点，我会晚到十分钟左右，您不用等我。"
    );
    assert_eq!(
        multi.variants["bullets"],
        "- 今天下午可能晚到十分钟左右\n- 原因是地铁晚点\n- 请老师不要等我"
    );
}

#[tokio::test]
async fn demo_engine_task_as_commit_and_prompt() {
    let input =
        "嗯给这个语音输入项目加一个改写引擎，先支持文字到文字，然后要有很多测试，暂时不要做界面。";

    assert_rewrite(
        Profile::Commit,
        input,
        "feat(rewrite): add text-to-text rewrite engine",
    )
    .await;
    assert_rewrite(
        Profile::Prompt,
        input,
        "目标：为语音输入项目实现一个改写引擎。\n上下文：当前优先支持文字到文字的纯文本流程。\n约束：先不要实现 GUI；补充覆盖多种润色效果的测试。\n期望输出：可运行的 rewrite 引擎代码、CLI 调试入口和测试用例。",
    )
    .await;
}

#[tokio::test]
async fn demo_dictionary_applies_before_rewrite() {
    let pipeline = LlmRewritePipeline::new(ExampleLlm);
    let ctx = RewriteContext {
        default_profile: Profile::Polish,
        user_dictionary: Arc::new(UserDictionary::new([(
            "克劳德".to_string(),
            "Claude".to_string(),
        )])),
        model: "example".to_string(),
        timeout: Duration::from_secs(4),
    };

    let result = pipeline
        .process("这个克劳德输出质量不错，帮我润色一下", ctx)
        .await
        .unwrap();

    assert_eq!(result.main, "Claude 的输出质量不错，请帮我润色一下。");
    assert!(!result.trace.fallback);
}

async fn assert_rewrite(profile: Profile, input: &str, expected: &str) {
    let result = rewrite(profile, input).await;

    assert_eq!(result.main, expected, "profile={}", profile.label());
    assert!(result.trace.llm_called);
    assert!(!result.trace.fallback);
}

async fn rewrite(profile: Profile, input: &str) -> voice_rewrite::RewriteResult {
    let pipeline = LlmRewritePipeline::new(ExampleLlm);
    pipeline
        .process(
            input,
            RewriteContext {
                default_profile: profile,
                user_dictionary: Arc::new(UserDictionary::default()),
                model: "example".to_string(),
                timeout: Duration::from_secs(4),
            },
        )
        .await
        .unwrap()
}

fn profile_from_prompt(system: &str) -> Profile {
    if system.contains("邮件") {
        Profile::Email
    } else if system.contains("微信") {
        Profile::Wechat
    } else if system.contains("要点") {
        Profile::Bullets
    } else if system.contains("Conventional Commit") {
        Profile::Commit
    } else if system.contains("AI prompt") {
        Profile::Prompt
    } else if system.contains("JSON") {
        Profile::Multi
    } else if system.contains("润色") {
        Profile::Polish
    } else {
        Profile::Clean
    }
}

fn clean_example(input: &str) -> String {
    input.to_string()
}

fn polish_example(input: &str) -> String {
    if input.contains("Claude") {
        "Claude 的输出质量不错，请帮我润色一下。".to_string()
    } else {
        "我想请老师知悉：今天下午我可能因地铁晚点而晚到十分钟左右，请老师不必等我。".to_string()
    }
}

fn email_example(_: &str) -> String {
    "老师您好：\n\n今天下午我可能因为地铁晚点会晚到十分钟左右，请您不必等我。\n\n谢谢老师。"
        .to_string()
}

fn wechat_example(_: &str) -> String {
    "老师，今天下午地铁可能晚点，我会晚到十分钟左右，您不用等我。".to_string()
}

fn bullets_example(_: &str) -> String {
    "- 今天下午可能晚到十分钟左右\n- 原因是地铁晚点\n- 请老师不要等我".to_string()
}

fn commit_example(_: &str) -> String {
    "feat(rewrite): add text-to-text rewrite engine".to_string()
}

fn prompt_example(_: &str) -> String {
    "目标：为语音输入项目实现一个改写引擎。\n上下文：当前优先支持文字到文字的纯文本流程。\n约束：先不要实现 GUI；补充覆盖多种润色效果的测试。\n期望输出：可运行的 rewrite 引擎代码、CLI 调试入口和测试用例。"
        .to_string()
}

fn multi_example(_: &str) -> String {
    serde_json::json!({
        "clean": "跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我。",
        "polish": "我想请老师知悉：今天下午我可能因地铁晚点而晚到十分钟左右，请老师不必等我。",
        "wechat": "老师，今天下午地铁可能晚点，我会晚到十分钟左右，您不用等我。",
        "bullets": "- 今天下午可能晚到十分钟左右\n- 原因是地铁晚点\n- 请老师不要等我"
    })
    .to_string()
}
