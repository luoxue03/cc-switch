import { invoke } from "@tauri-apps/api/core";

export interface CodexProtectionField {
  path: string[];
  name: string;
  value: string;
  present: boolean;
  sensitive: boolean;
}

export interface CodexProtectionSnapshot {
  configPath: string;
  configRevision: string | null;
  rulesRevision: string;
  protectedPaths: string[][];
  fields: CodexProtectionField[];
  warning: string | null;
}

export interface CodexProtectedEditor {
  configPath: string;
  text: string;
  revision: string | null;
}

export const codexProtectionApi = {
  get: () => invoke<CodexProtectionSnapshot>("get_codex_protection"),
  save: (paths: string[][], expectedRevision: string) =>
    invoke<string>("save_codex_protection", { paths, expectedRevision }),
  readEditor: () => invoke<CodexProtectedEditor>("read_codex_protected_editor"),
  saveEditor: (editor: CodexProtectedEditor, text: string) =>
    invoke<void>("save_codex_protected_editor", {
      configPath: editor.configPath,
      text,
      expectedRevision: editor.revision,
    }),
};
