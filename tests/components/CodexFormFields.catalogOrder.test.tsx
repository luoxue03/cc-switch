import { render, screen } from "@testing-library/react";
import type { PropsWithChildren } from "react";
import { useForm } from "react-hook-form";
import { describe, expect, it, vi } from "vitest";
import { CodexFormFields } from "@/components/providers/forms/CodexFormFields";
import { Form } from "@/components/ui/form";

const FormShell = ({ children }: PropsWithChildren) => {
  const form = useForm();

  return <Form {...form}>{children}</Form>;
};

function renderCodexFormFields() {
  return render(
    <FormShell>
      <CodexFormFields
        codexApiKey="sk-test"
        onApiKeyChange={vi.fn()}
        category="third_party"
        shouldShowApiKeyLink={false}
        websiteUrl=""
        shouldShowSpeedTest={false}
        codexBaseUrl="https://api.example.com/v1"
        onBaseUrlChange={vi.fn()}
        isFullUrl={false}
        onFullUrlChange={vi.fn()}
        isEndpointModalOpen={false}
        onEndpointModalToggle={vi.fn()}
        autoSelect={false}
        onAutoSelectChange={vi.fn()}
        codexModel=""
        onModelChange={vi.fn()}
        apiFormat="openai_chat"
        onApiFormatChange={vi.fn()}
        anthropicAuthField="ANTHROPIC_AUTH_TOKEN"
        onAnthropicAuthFieldChange={vi.fn()}
        impersonateClaudeCode={false}
        onImpersonateClaudeCodeChange={vi.fn()}
        maxOutputTokens=""
        onMaxOutputTokensChange={vi.fn()}
        codexChatReasoning={{}}
        onCodexChatReasoningChange={vi.fn()}
        promptCacheRouting="auto"
        onPromptCacheRoutingChange={vi.fn()}
        catalogModels={[
          { model: "chat/deepseek-v4-pro", displayName: "DeepSeek-V4-Pro" },
        ]}
        onCatalogModelsChange={vi.fn()}
        speedTestEndpoints={[]}
        customUserAgent=""
        onCustomUserAgentChange={vi.fn()}
        localProxyHeadersOverride=""
        onLocalProxyHeadersOverrideChange={vi.fn()}
        localProxyBodyOverride=""
        onLocalProxyBodyOverrideChange={vi.fn()}
      />
    </FormShell>,
  );
}

describe("CodexFormFields catalog ordering", () => {
  it("renders drag sorting and concrete route modes without a default option", () => {
    renderCodexFormFields();

    expect(
      screen.getByRole("button", { name: "拖拽排序" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: "路由模式" }),
    ).toHaveTextContent("Chat");
    expect(screen.queryByText("默认")).not.toBeInTheDocument();
  });
});
