import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { PropsWithChildren } from "react";
import { useForm } from "react-hook-form";
import { describe, expect, it, vi } from "vitest";
import { CodexFormFields } from "@/components/providers/forms/CodexFormFields";
import { Form } from "@/components/ui/form";
import type { CodexCatalogModel } from "@/types";

const FormShell = ({ children }: PropsWithChildren) => {
  const form = useForm();

  return <Form {...form}>{children}</Form>;
};

function renderCodexFormFields({
  catalogModels,
  onCatalogModelsChange,
}: {
  catalogModels: CodexCatalogModel[];
  onCatalogModelsChange: (models: CodexCatalogModel[]) => void;
}) {
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
        apiFormat="openai_chat"
        onApiFormatChange={vi.fn()}
        codexChatReasoning={{}}
        onCodexChatReasoningChange={vi.fn()}
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
  it("moves catalog rows and emits the reordered model list", async () => {
    const onCatalogModelsChange = vi.fn();
    renderCodexFormFields({
      onCatalogModelsChange,
      catalogModels: [
        { model: "chat/deepseek-v4-pro", displayName: "DeepSeek-V4-Pro" },
        { model: "GPT-5.5", displayName: "GPT-5.5", routeMode: "responses" },
      ],
    });

    fireEvent.click(screen.getAllByRole("button", { name: "下移" })[0]);

    await waitFor(() => {
      expect(onCatalogModelsChange).toHaveBeenCalled();
    });

    const lastCall = onCatalogModelsChange.mock.lastCall?.[0] as
      | CodexCatalogModel[]
      | undefined;
    expect(lastCall?.map((model) => model.model)).toEqual([
      "GPT-5.5",
      "chat/deepseek-v4-pro",
    ]);
    expect(lastCall?.[0]).toMatchObject({
      displayName: "GPT-5.5",
      routeMode: "responses",
    });
  });
});
