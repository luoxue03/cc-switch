import { describe, expect, it } from "vitest";
import {
  buildCodexRouteSettingsMeta,
  normalizeCodexCatalogModelsForSave,
} from "@/components/providers/forms/ProviderForm";
import { mapCodexCatalogModelForForm } from "@/components/providers/forms/hooks/useCodexConfigState";

describe("ProviderForm Codex catalog helpers", () => {
  it("normalizes catalog rows and removes empty or duplicate models", () => {
    expect(
      normalizeCodexCatalogModelsForSave([
        { model: " deepseek-v4-flash ", displayName: " DeepSeek " },
        { model: "deepseek-v4-flash", displayName: "Duplicate" },
        { model: "", displayName: "Empty" },
        { model: "kimi-k2", contextWindow: "128000 tokens" },
        { model: " gpt-5.5 ", routeMode: "responses" },
        { model: " claude-sonnet ", routeMode: "anthropic" },
      ]),
    ).toEqual([
      {
        model: "deepseek-v4-flash",
        displayName: "DeepSeek",
        routeMode: "responses",
      },
      { model: "kimi-k2", contextWindow: 128000, routeMode: "responses" },
      { model: "gpt-5.5", routeMode: "responses" },
      { model: "claude-sonnet", routeMode: "anthropic" },
    ]);
  });

  it("uses the provider fallback route mode when rows do not specify one", () => {
    expect(
      normalizeCodexCatalogModelsForSave(
        [{ model: " deepseek-v4-pro ", displayName: " DeepSeek " }],
        "chat",
      ),
    ).toEqual([
      { model: "deepseek-v4-pro", displayName: "DeepSeek", routeMode: "chat" },
    ]);
  });

  it("preserves native-profile overrides (parallel tool calls + input modalities + base instructions)", () => {
    expect(
      normalizeCodexCatalogModelsForSave([
        {
          model: "MiniMax-M3",
          displayName: "MiniMax-M3",
          contextWindow: 1000000,
          supportsParallelToolCalls: true,
          inputModalities: ["text", "image"],
          baseInstructions:
            "  You are Codex, a coding agent based on MiniMax-M3.  ",
        },
        // false must be preserved (not dropped as falsy); empty modalities dropped;
        // empty/whitespace baseInstructions dropped
        {
          model: "mimo-v2.5-pro",
          supportsParallelToolCalls: false,
          inputModalities: [],
          baseInstructions: "   ",
        },
      ]),
    ).toEqual([
      {
        model: "MiniMax-M3",
        displayName: "MiniMax-M3",
        contextWindow: 1000000,
        supportsParallelToolCalls: true,
        inputModalities: ["text", "image"],
        baseInstructions: "You are Codex, a coding agent based on MiniMax-M3.",
        routeMode: "responses",
      },
      {
        model: "mimo-v2.5-pro",
        supportsParallelToolCalls: false,
        routeMode: "responses",
      },
    ]);
  });

  it("preserves per-model reasoning levels and default level", () => {
    expect(
      normalizeCodexCatalogModelsForSave([
        {
          model: "deepseek-v4-flash",
          displayName: "DeepSeek V4 Flash",
          reasoningLevels: ["none", "low", "medium", "high", "xhigh", "max"],
          defaultReasoningLevel: " xhigh ",
        },
        // empty levels / whitespace default are dropped
        {
          model: "plain-model",
          reasoningLevels: [],
          defaultReasoningLevel: "   ",
        },
      ]),
    ).toEqual([
      {
        model: "deepseek-v4-flash",
        displayName: "DeepSeek V4 Flash",
        reasoningLevels: ["none", "low", "medium", "high", "xhigh", "max"],
        defaultReasoningLevel: "xhigh",
        routeMode: "responses",
      },
      { model: "plain-model", routeMode: "responses" },
    ]);
  });

  it("round-trips reasoning levels through load and save without loss", () => {
    // load→save 回环：加载映射（mapCodexCatalogModelForForm）与保存归一化
    // （normalizeCodexCatalogModelsForSave）各锁半边时，回环丢字段两边都测不出——
    // 而编辑保存丢表会让依赖逐模型档位的功能（zen 钳制）静默失效且 UI 无可察觉。
    const stored = [
      {
        model: "glm-5.2",
        displayName: "GLM 5.2",
        reasoningLevels: ["high", "max"],
      },
      // 手写/旧数据可能是 snake_case，加载侧兼容后保存侧同样要留住
      { model: "deepseek-v4-flash", reasoning_levels: ["low", "high", "max"] },
      { model: "glm-5.1" }, // toggle 型：无表，全程不得凭空造表
    ];

    const roundTripped = normalizeCodexCatalogModelsForSave(
      stored.map(mapCodexCatalogModelForForm),
    );

    expect(roundTripped).toEqual([
      {
        model: "glm-5.2",
        displayName: "GLM 5.2",
        reasoningLevels: ["high", "max"],
        routeMode: "responses",
      },
      {
        model: "deepseek-v4-flash",
        reasoningLevels: ["low", "high", "max"],
        routeMode: "responses",
      },
      { model: "glm-5.1", routeMode: "responses" },
    ]);
  });

  it("trims reasoning level values on save", () => {
    // 手编 JSON 里的 " high " 不得原样落库/发给上游。
    expect(
      normalizeCodexCatalogModelsForSave([
        { model: "glm-5.2", reasoningLevels: [" high ", "max"] },
      ]),
    ).toEqual([
      {
        model: "glm-5.2",
        reasoningLevels: ["high", "max"],
        routeMode: "responses",
      },
    ]);
  });

  it("persists Chat and Anthropic settings together for the current Codex provider", () => {
    expect(
      buildCodexRouteSettingsMeta({
        enabled: true,
        chatReasoning: {
          supportsThinking: true,
          supportsEffort: true,
          effortParam: "reasoning_effort",
        },
        promptCacheRouting: "enabled",
        anthropicAuthField: "ANTHROPIC_API_KEY",
        impersonateClaudeCode: true,
        maxOutputTokens: "65536",
      }),
    ).toMatchObject({
      codexChatReasoning: {
        supportsThinking: true,
        supportsEffort: true,
        effortParam: "reasoning_effort",
      },
      promptCacheRouting: "enabled",
      apiKeyField: "ANTHROPIC_API_KEY",
      impersonateClaudeCode: true,
      maxOutputTokens: 65536,
    });
  });

  it("does not write Codex route settings for another app or an official provider", () => {
    expect(
      buildCodexRouteSettingsMeta({
        enabled: false,
        chatReasoning: { supportsThinking: true },
        promptCacheRouting: "enabled",
        anthropicAuthField: "ANTHROPIC_API_KEY",
        impersonateClaudeCode: true,
        maxOutputTokens: "65536",
      }),
    ).toEqual({
      codexChatReasoning: undefined,
      promptCacheRouting: undefined,
      apiKeyField: undefined,
      impersonateClaudeCode: undefined,
      maxOutputTokens: undefined,
    });
  });
});
