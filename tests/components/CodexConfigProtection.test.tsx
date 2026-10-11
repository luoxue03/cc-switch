import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CodexConfigProtection } from "@/components/settings/CodexConfigProtection";
import {
  codexProtectionApi,
  type CodexProtectionSnapshot,
} from "@/lib/api/codex-protection";

vi.mock("@/lib/api/codex-protection", () => ({
  codexProtectionApi: {
    get: vi.fn(),
    save: vi.fn(),
    readEditor: vi.fn(),
    saveEditor: vi.fn(),
  },
}));
const base: CodexProtectionSnapshot = {
  configPath: "/test/.codex/config.toml",
  configRevision: "config-1",
  rulesRevision: "rules-1",
  protectedPaths: [["model_provider"], ["openai_base_url"]],
  warning: null,
  fields: [
    {
      path: ["model_provider"],
      name: "model_provider",
      value: '"openai"',
      present: true,
      sensitive: false,
    },
    {
      path: ["openai_base_url"],
      name: "openai_base_url",
      value: '"http://127.0.0.1:15721/v1"',
      present: true,
      sensitive: false,
    },
    {
      path: ["model"],
      name: "model",
      value: '"GPT-6-Astra"',
      present: true,
      sensitive: false,
    },
    {
      path: ["custom", "a.b", "flag"],
      name: 'custom."a.b".flag',
      value: "true",
      present: true,
      sensitive: false,
    },
    {
      path: ["api_key"],
      name: "api_key",
      value: "[hidden]",
      present: true,
      sensitive: true,
    },
  ],
};
const mount = async () => {
  render(<CodexConfigProtection />);
  await screen.findByText("model_provider");
};
beforeEach(() => {
  vi.mocked(codexProtectionApi.get)
    .mockReset()
    .mockResolvedValue(structuredClone(base));
  vi.mocked(codexProtectionApi.save).mockReset().mockResolvedValue("rules-2");
  vi.mocked(codexProtectionApi.readEditor)
    .mockReset()
    .mockResolvedValue({
      configPath: base.configPath,
      text: "model_provider='openai'\n",
      revision: "config-1",
    });
  vi.mocked(codexProtectionApi.saveEditor).mockReset().mockResolvedValue();
});
describe("Codex configuration protection", () => {
  it("lists custom nested fields and defaults but never saves on mount or toggle", async () => {
    await mount();
    expect(
      screen.getByRole("checkbox", {
        name: "codexProtection.protect model_provider",
      }),
    ).toBeChecked();
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: 'codexProtection.protect custom."a.b".flag',
      }),
    );
    expect(codexProtectionApi.save).not.toHaveBeenCalled();
    expect(codexProtectionApi.readEditor).not.toHaveBeenCalled();
    expect(codexProtectionApi.saveEditor).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "codexProtection.saveRules" }),
    );
    await waitFor(() =>
      expect(codexProtectionApi.save).toHaveBeenCalledWith(
        [...base.protectedPaths, ["custom", "a.b", "flag"]],
        "rules-1",
      ),
    );
    expect(codexProtectionApi.saveEditor).not.toHaveBeenCalled();
    expect(screen.getByText("codexProtection.hidden")).toBeInTheDocument();
  });
  it("permits clearing all defaults and sends an explicit empty selection", async () => {
    await mount();
    fireEvent.click(
      screen.getByRole("button", { name: "codexProtection.clear" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "codexProtection.saveRules" }),
    );
    await waitFor(() =>
      expect(codexProtectionApi.save).toHaveBeenCalledWith([], "rules-1"),
    );
  });
  it("explicit editor save includes the file revision and may edit protected keys", async () => {
    await mount();
    fireEvent.click(
      screen.getByRole("button", { name: "codexProtection.edit" }),
    );
    const input = await screen.findByRole("textbox", {
      name: "codexProtection.editor",
    });
    fireEvent.change(input, { target: { value: "model_provider='mine'\n" } });
    expect(codexProtectionApi.saveEditor).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "codexProtection.saveToml" }),
    );
    await waitFor(() =>
      expect(codexProtectionApi.saveEditor).toHaveBeenCalledWith(
        {
          configPath: base.configPath,
          text: "model_provider='openai'\n",
          revision: "config-1",
        },
        "model_provider='mine'\n",
      ),
    );
    expect(codexProtectionApi.save).not.toHaveBeenCalled();
  });
  it("keeps the edit draft on a stale-file conflict and exposes the error", async () => {
    vi.mocked(codexProtectionApi.saveEditor).mockRejectedValueOnce(
      "external edit conflict",
    );
    await mount();
    fireEvent.click(
      screen.getByRole("button", { name: "codexProtection.edit" }),
    );
    const input = await screen.findByRole("textbox", {
      name: "codexProtection.editor",
    });
    fireEvent.change(input, { target: { value: "model='my draft'" } });
    fireEvent.click(
      screen.getByRole("button", { name: "codexProtection.saveToml" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "external edit conflict",
    );
    expect(input).toHaveValue("model='my draft'");
  });
  it("does not lose pending selections when saving TOML", async () => {
    await mount();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "codexProtection.protect model" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "codexProtection.edit" }),
    );
    fireEvent.change(
      await screen.findByRole("textbox", { name: "codexProtection.editor" }),
      { target: { value: "model='manual'" } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: "codexProtection.saveToml" }),
    );
    await waitFor(() =>
      expect(codexProtectionApi.get).toHaveBeenCalledTimes(2),
    );
    expect(
      screen.getByRole("checkbox", { name: "codexProtection.protect model" }),
    ).toBeChecked();
  });
  it("ignores a stale read when the configuration directory changes", async () => {
    let resolve!: (value: CodexProtectionSnapshot) => void;
    vi.mocked(codexProtectionApi.get).mockReturnValueOnce(
      new Promise((r) => {
        resolve = r;
      }),
    );
    const { rerender } = render(<CodexConfigProtection configDir="old" />);
    vi.mocked(codexProtectionApi.get).mockResolvedValueOnce({
      ...base,
      configPath: "/new/config.toml",
    });
    rerender(<CodexConfigProtection configDir="new" />);
    await screen.findByText("/new/config.toml");
    await act(async () => resolve(base));
    expect(screen.queryByText(base.configPath)).not.toBeInTheDocument();
  });
});
