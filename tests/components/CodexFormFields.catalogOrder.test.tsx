import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { PropsWithChildren } from "react";
import { useForm } from "react-hook-form";
import { describe, expect, it, vi } from "vitest";
import { CodexFormFields } from "@/components/providers/forms/CodexFormFields";
import { Form } from "@/components/ui/form";
import { providersApi } from "@/lib/api/providers";
import type { CodexCatalogModel } from "@/types";
import { open, save } from "@tauri-apps/plugin-dialog";

vi.mock("@/lib/api/providers", () => ({
  providersApi: {
    readCodexModelCatalogFile: vi.fn(),
    exportCodexModelMappingFile: vi.fn(),
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

const FormShell = ({ children }: PropsWithChildren) => {
  const form = useForm();

  return <Form {...form}>{children}</Form>;
};

function renderCodexFormFields({
  catalogModels = [
    { model: "chat/deepseek-v4-pro", displayName: "DeepSeek-V4-Pro" },
  ],
  onCatalogModelsChange = vi.fn(),
}: {
  catalogModels?: CodexCatalogModel[];
  onCatalogModelsChange?: (models: CodexCatalogModel[]) => void;
} = {}) {
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
        catalogModels={catalogModels}
        onCatalogModelsChange={onCatalogModelsChange}
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

  it("imports the local catalog while preserving existing per-model routes", async () => {
    const user = userEvent.setup();
    const onCatalogModelsChange = vi.fn();
    vi.mocked(open).mockResolvedValue("/tmp/custom-model-catalog.json");
    vi.mocked(providersApi.readCodexModelCatalogFile).mockResolvedValue({
      models: [
        {
          model: "chat/deepseek-v4-pro",
          displayName: "Imported DeepSeek",
        },
        { model: "new-model", displayName: "New Model" },
      ],
    });

    renderCodexFormFields({
      catalogModels: [
        {
          model: "chat/deepseek-v4-pro",
          displayName: "DeepSeek-V4-Pro",
          routeMode: "responses",
        },
      ],
      onCatalogModelsChange,
    });

    await user.click(screen.getByRole("button", { name: "从本地导入" }));

    expect(open).toHaveBeenCalledWith(
      expect.objectContaining({
        multiple: false,
        directory: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      }),
    );
    expect(providersApi.readCodexModelCatalogFile).toHaveBeenCalledWith(
      "/tmp/custom-model-catalog.json",
    );
    expect(screen.getByDisplayValue("Imported DeepSeek")).toBeInTheDocument();
    expect(screen.getByDisplayValue("New Model")).toBeInTheDocument();
    const routeSelectors = screen.getAllByRole("combobox", {
      name: "路由模式",
    });
    expect(routeSelectors[0]).toHaveTextContent("Responses");
    expect(routeSelectors[1]).toHaveTextContent("Chat");
    await waitFor(() =>
      expect(onCatalogModelsChange).toHaveBeenLastCalledWith([
        expect.objectContaining({
          model: "chat/deepseek-v4-pro",
          routeMode: "responses",
        }),
        expect.objectContaining({ model: "new-model", routeMode: "chat" }),
      ]),
    );
  });

  it("exports a portable mapping with explicit route modes", async () => {
    const user = userEvent.setup();
    vi.mocked(save).mockResolvedValue("/tmp/cc-switch-model-mapping.json");
    vi.mocked(providersApi.exportCodexModelMappingFile).mockResolvedValue();

    renderCodexFormFields({
      catalogModels: [
        {
          model: "deepseek-v4-pro",
          displayName: "DeepSeek V4 Pro",
          contextWindow: 1_000_000,
          routeMode: "chat",
          inputModalities: ["text"],
        },
        {
          model: "gpt-5.5",
          displayName: "GPT-5.5",
          routeMode: "responses",
        },
      ],
    });

    await user.click(screen.getByRole("button", { name: "导出映射" }));

    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: "cc-switch-model-mapping.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      }),
    );
    expect(providersApi.exportCodexModelMappingFile).toHaveBeenCalledWith(
      "/tmp/cc-switch-model-mapping.json",
      [
        expect.objectContaining({
          model: "deepseek-v4-pro",
          displayName: "DeepSeek V4 Pro",
          contextWindow: "1000000",
          routeMode: "chat",
          inputModalities: ["text"],
        }),
        expect.objectContaining({
          model: "gpt-5.5",
          displayName: "GPT-5.5",
          routeMode: "responses",
        }),
      ],
    );
  });
});
