import { describe, expect, it } from "vitest";
import {
  buildCodexRouteSettingsMeta,
  normalizeCodexCatalogModelsForSave,
} from "@/components/providers/forms/ProviderForm";

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
