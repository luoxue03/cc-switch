//! Thinking 优化器

use super::types::OptimizerConfig;
use regex::Regex;
use serde_json::{json, Value};
use std::sync::LazyLock;

/// 根据模型类型自动优化 thinking 配置
///
/// 三路径分发：
/// - skip: haiku 模型直接跳过
/// - adaptive: current adaptive-thinking Claude models use adaptive thinking
/// - legacy: 其他模型注入 enabled thinking + budget_tokens
pub fn optimize(body: &mut Value, config: &OptimizerConfig) {
    if !config.thinking_optimizer {
        return;
    }

    let model = match body.get("model").and_then(|m| m.as_str()) {
        Some(m) => m.to_lowercase(),
        None => return,
    };

    if model.contains("haiku") {
        log::info!("[OPT] thinking: skip(haiku)");
        return;
    }

    if uses_adaptive_thinking(&model) {
        log::info!("[OPT] thinking: adaptive({model})");
        body["thinking"] = json!({"type": "adaptive"});
        body["output_config"] = json!({"effort": "max"});
        append_beta(body, "context-1m-2025-08-07");
        return;
    }

    // legacy path
    log::info!("[OPT] thinking: legacy({model})");

    let max_tokens = body
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(16384);

    let budget_target = max_tokens.saturating_sub(1);

    let thinking_type = body
        .get("thinking")
        .and_then(|t| t.get("type"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());

    match thinking_type.as_deref() {
        None | Some("disabled") => {
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": budget_target
            });
            append_beta(body, "interleaved-thinking-2025-05-14");
        }
        Some("enabled") => {
            let current_budget = body
                .get("thinking")
                .and_then(|t| t.get("budget_tokens"))
                .and_then(|b| b.as_u64())
                .unwrap_or(0);
            if current_budget < budget_target {
                body["thinking"]["budget_tokens"] = json!(budget_target);
            }
            append_beta(body, "interleaved-thinking-2025-05-14");
        }
        _ => {
            append_beta(body, "interleaved-thinking-2025-05-14");
        }
    }
}

pub(crate) fn uses_adaptive_thinking(model: &str) -> bool {
    let normalized = normalize_model_name(model);
    [
        "fable-5",
        "mythos-5",
        "mythos-preview",
        "sonnet-5",
        "opus-5",
        "opus-4-8",
        "opus-4-7",
        "opus-4-6",
        "sonnet-4-6",
    ]
    .iter()
    .any(|needle| matches_model_family(&normalized, needle))
}

/// Models where omitting `thinking` still leaves adaptive thinking enabled
/// (Claude thinking docs, per-model table: the 5.x Fable / Mythos / Opus /
/// Sonnet generations and Mythos Preview; Opus 4.x and Sonnet 4.x are off).
pub(crate) fn adaptive_thinking_is_default(model: &str) -> bool {
    let normalized = normalize_model_name(model);
    [
        "fable-5",
        "mythos-5",
        "mythos-preview",
        "opus-5",
        "sonnet-5",
    ]
    .iter()
    .any(|needle| matches_model_family(&normalized, needle))
}

/// Models that reject `thinking: {"type":"disabled"}`.
pub(crate) fn thinking_cannot_be_disabled(model: &str) -> bool {
    let normalized = normalize_model_name(model);
    ["fable-5", "mythos-5"]
        .iter()
        .any(|needle| matches_model_family(&normalized, needle))
        || matches_model_family(&normalized, "opus-5-5")
}

/// Opus 5.5 is always adaptive and accepts a distinct xhigh effort level.
pub(crate) fn is_opus_5_5(model: &str) -> bool {
    let normalized = normalize_model_name(model);
    matches_model_family(&normalized, "opus-5-5")
}

fn matches_model_family(model: &str, family: &str) -> bool {
    model.match_indices(family).any(|(start, _)| {
        let end = start + family.len();
        let left_boundary = start == 0 || !model.as_bytes()[start - 1].is_ascii_alphanumeric();
        let right_boundary = end == model.len() || !model.as_bytes()[end].is_ascii_alphanumeric();
        left_boundary && right_boundary
    })
}

fn normalize_model_name(model: &str) -> String {
    static VERSION_FIRST_CLAUDE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?P<prefix>^|[^a-z0-9])claude-(?P<version>[0-9]+(?:-[0-9]+)*)-(?P<family>opus|sonnet|fable|mythos|haiku)(?P<suffix>$|[^a-z0-9])")
            .expect("valid Claude alias regex")
    });
    // Prefixes such as chat/, any/, or vendor/account/ select gateway routes,
    // not model capabilities. Keep the complete original ID at every call site.
    let model = model.trim().rsplit('/').next().unwrap_or(model);
    let normalized = model.to_ascii_lowercase().replace(['.', '_'], "-");
    // Gateway aliases may use Claude-5.5-opus instead of claude-opus-5-5.
    // Normalize for capability detection only; never rewrite the upstream model ID.
    VERSION_FIRST_CLAUDE
        .replace_all(&normalized, "${prefix}claude-${family}-${version}${suffix}")
        .into_owned()
}

/// 追加 beta 标识到 anthropic_beta 数组（去重）
fn append_beta(body: &mut Value, beta: &str) {
    match body.get_mut("anthropic_beta") {
        Some(Value::Array(arr)) => {
            if arr.iter().any(|v| v.as_str() == Some(beta)) {
                return;
            }
            arr.push(json!(beta));
        }
        Some(Value::Null) | None => {
            body["anthropic_beta"] = json!([beta]);
        }
        _ => {
            body["anthropic_beta"] = json!([beta]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn enabled_config() -> OptimizerConfig {
        OptimizerConfig {
            enabled: true,
            thinking_optimizer: true,
            cache_injection: true,
        }
    }

    fn disabled_config() -> OptimizerConfig {
        OptimizerConfig {
            enabled: true,
            thinking_optimizer: false,
            cache_injection: true,
        }
    }

    #[test]
    fn test_adaptive_opus_4_8() {
        let mut body = json!({
            "model": "anthropic/claude-opus-4.8",
            "max_tokens": 16384,
            "thinking": {"type": "enabled", "budget_tokens": 8000},
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        assert_eq!(body["thinking"]["type"], "adaptive");
        assert!(body["thinking"].get("budget_tokens").is_none());
        assert_eq!(body["output_config"]["effort"], "max");
        let betas = body["anthropic_beta"].as_array().unwrap();
        assert!(betas.iter().any(|v| v == "context-1m-2025-08-07"));
    }

    #[test]
    fn current_generation_models_use_adaptive_thinking() {
        for model in [
            "claude-sonnet-5",
            "anthropic/claude-fable-5",
            "claude-mythos-5",
            "claude-opus-5",
            "claude-opus-4.8",
        ] {
            assert!(uses_adaptive_thinking(model), "model={model}");
        }
        for model in [
            "claude-sonnet-5",
            "claude-sonnet-5-5",
            "claude-opus-5",
            "claude-opus-5-5",
            "claude-fable-5-1",
            "claude-mythos-5-1",
        ] {
            assert!(adaptive_thinking_is_default(model), "model={model}");
        }
        for model in ["claude-opus-4-8", "claude-opus-4-5", "claude-sonnet-4-6"] {
            assert!(!adaptive_thinking_is_default(model), "model={model}");
        }
        assert!(thinking_cannot_be_disabled("claude-fable-5"));
        assert!(!thinking_cannot_be_disabled("claude-sonnet-5"));
    }

    #[test]
    fn version_first_claude_aliases_match_canonical_thinking_policy() {
        for (alias, canonical) in [
            ("Claude-5.5-opus", "claude-opus-5-5"),
            ("anthropic/Claude-5.5-opus[1m]", "claude-opus-5-5[1m]"),
            ("Claude_5_5_Opus", "claude-opus-5-5"),
            ("Claude-4.8-opus", "claude-opus-4-8"),
            ("Claude-4.5-opus", "claude-opus-4-5"),
            ("Claude-4.6-sonnet", "claude-sonnet-4-6"),
            ("Claude-5.1-fable", "claude-fable-5-1"),
            ("Claude-5.1-mythos", "claude-mythos-5-1"),
            ("chat/vendor/Claude-5.5-opus", "claude-opus-5-5"),
            ("anthropic/claude-opus-4.8", "claude-opus-4-8"),
            ("Claude-5-5-opus-thinking", "claude-opus-5-5-thinking"),
            ("Vendor/Claude_4_8_Opus[1M]", "claude-opus-4-8[1m]"),
        ] {
            assert_eq!(
                uses_adaptive_thinking(alias),
                uses_adaptive_thinking(canonical),
                "{alias}"
            );
            assert_eq!(
                adaptive_thinking_is_default(alias),
                adaptive_thinking_is_default(canonical),
                "{alias}"
            );
            assert_eq!(
                thinking_cannot_be_disabled(alias),
                thinking_cannot_be_disabled(canonical),
                "{alias}"
            );
        }
        for model in [
            "deepseek-v4-pro",
            "GPT-6-Astra",
            "myclaude-5.5-opus",
            "claude-5.5-opuslike",
            "claude-opus-5x",
            "claude-opus-4-60",
            "claude-sonnet-50",
            "claude-opus-5-5/mcf-2",
            "claude-fable-5/chat/deepseek-v4-pro",
        ] {
            assert!(!uses_adaptive_thinking(model), "{model}");
            assert!(!adaptive_thinking_is_default(model), "{model}");
            assert!(!thinking_cannot_be_disabled(model), "{model}");
        }
    }

    #[test]
    fn configured_gateway_names_only_enable_known_claude_capabilities() {
        for (model, adaptive, default_on, always_on) in [
            ("Claude-4.8-opus", true, false, false),
            ("claude-4.8-opus", true, false, false),
            ("Claude-5.1-fable", true, true, true),
            ("claude-5.1-fable", true, true, true),
            ("Claude-5.5-opus", true, true, true),
            ("GPT-6-Astra", false, false, false),
            ("chat/deepseek-v4-pro-0813", false, false, false),
            ("chat/deepseek-v4.1-flash", false, false, false),
            ("chat/ZHIPU/GLM-5.3", false, false, false),
            ("specxmaster/gpt-5.6-terra", false, false, false),
            ("specxmaster/gpt-5.6-luna", false, false, false),
            ("specxmaster/gpt-5.6-sol", false, false, false),
            ("GPT-5.6-sol", false, false, false),
            ("gpt-5.6-sol", false, false, false),
            ("any/gpt-5.6-sol", false, false, false),
            ("chat/kimi/kimi-k3", false, false, false),
            ("codex-auto-review", false, false, false),
            ("gpt-5.4-mini", false, false, false),
            ("mog-10-a", false, false, false),
            ("GPT-6-Astra-2", false, false, false),
            ("mcf-2", false, false, false),
        ] {
            assert_eq!(uses_adaptive_thinking(model), adaptive, "{model}");
            assert_eq!(adaptive_thinking_is_default(model), default_on, "{model}");
            assert_eq!(thinking_cannot_be_disabled(model), always_on, "{model}");
        }
    }

    #[test]
    fn thinking_optimizer_keeps_gateway_alias_and_output_limit_unchanged() {
        for model in [
            "Claude-4.8-opus",
            "claude-4.8-opus",
            "Claude-5.1-fable",
            "Claude-5.5-opus",
        ] {
            let mut body = json!({
                "model": model,
                "max_tokens": 65536,
                "thinking": { "type": "enabled", "budget_tokens": 8192 },
                "messages": [{ "role": "user", "content": "hello" }]
            });
            let messages = body["messages"].clone();
            optimize(&mut body, &enabled_config());
            assert_eq!(body["model"], model);
            assert_eq!(body["max_tokens"], 65536);
            assert_eq!(body["messages"], messages);
            assert_eq!(body["thinking"], json!({ "type": "adaptive" }));
        }
    }

    #[test]
    fn opus_5_5_requires_thinking_but_older_opus_keeps_existing_policy() {
        for model in [
            "Claude-5.5-opus",
            "claude-opus-5-5",
            "anthropic.claude-opus-5-5",
        ] {
            assert!(thinking_cannot_be_disabled(model), "{model}");
        }
        for model in [
            "claude-opus-5",
            "claude-opus-4-8",
            "Claude-4.8-opus",
            "claude-opus-5-50",
        ] {
            assert!(!thinking_cannot_be_disabled(model), "{model}");
        }
    }

    #[test]
    fn test_adaptive_opus_4_6() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "thinking": {"type": "enabled", "budget_tokens": 8000},
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        assert_eq!(body["thinking"]["type"], "adaptive");
        assert!(body["thinking"].get("budget_tokens").is_none());
        assert_eq!(body["output_config"]["effort"], "max");
        let betas = body["anthropic_beta"].as_array().unwrap();
        assert!(betas.iter().any(|v| v == "context-1m-2025-08-07"));
    }

    #[test]
    fn test_adaptive_sonnet_4_6() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        assert_eq!(body["thinking"]["type"], "adaptive");
        assert!(body["thinking"].get("budget_tokens").is_none());
        assert_eq!(body["output_config"]["effort"], "max");
        let betas = body["anthropic_beta"].as_array().unwrap();
        assert!(betas.iter().any(|v| v == "context-1m-2025-08-07"));
    }

    #[test]
    fn test_legacy_sonnet_4_5_thinking_null() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-5-20250514-v1:0",
            "max_tokens": 16384,
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 16383);
        let betas = body["anthropic_beta"].as_array().unwrap();
        assert!(betas.iter().any(|v| v == "interleaved-thinking-2025-05-14"));
    }

    #[test]
    fn test_legacy_budget_too_small_upgraded() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-5-20250514-v1:0",
            "max_tokens": 16384,
            "thinking": {"type": "enabled", "budget_tokens": 1024},
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 16383);
    }

    #[test]
    fn test_skip_haiku() {
        let mut body = json!({
            "model": "anthropic.claude-haiku-4-5-20250514-v1:0",
            "max_tokens": 8192,
            "messages": [{"role": "user", "content": "hello"}]
        });
        let original = body.clone();

        optimize(&mut body, &enabled_config());

        assert_eq!(body, original);
    }

    #[test]
    fn test_thinking_optimizer_disabled() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "messages": [{"role": "user", "content": "hello"}]
        });
        let original = body.clone();

        optimize(&mut body, &disabled_config());

        assert_eq!(body, original);
    }

    #[test]
    fn test_adaptive_dedup_beta() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "anthropic_beta": ["context-1m-2025-08-07"],
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        let betas = body["anthropic_beta"].as_array().unwrap();
        let count = betas
            .iter()
            .filter(|v| v == &&json!("context-1m-2025-08-07"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_legacy_disabled_thinking_injected() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-5-20250514-v1:0",
            "max_tokens": 8192,
            "thinking": {"type": "disabled"},
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 8191);
    }

    #[test]
    fn test_legacy_default_max_tokens() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-5-20250514-v1:0",
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 16383);
    }

    #[test]
    fn test_append_beta_null_field() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "anthropic_beta": null,
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        let betas = body["anthropic_beta"].as_array().unwrap();
        assert!(betas.iter().any(|v| v == "context-1m-2025-08-07"));
    }
}
