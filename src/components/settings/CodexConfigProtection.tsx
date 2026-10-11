import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  FilePenLine,
  RefreshCw,
  Save,
  Search,
  ShieldCheck,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { HoverTip } from "@/components/ui/hover-tip";
import {
  codexProtectionApi,
  type CodexProtectionSnapshot,
  type CodexProtectedEditor,
} from "@/lib/api/codex-protection";
import { toast } from "@/lib/toast";

const keyOf = (path: string[]) => JSON.stringify(path);
const includesParent = (paths: string[][], path: string[]) =>
  paths.some(
    (parent) =>
      parent.length < path.length &&
      parent.every((part, index) => path[index] === part),
  );

export function CodexConfigProtection({ configDir }: { configDir?: string }) {
  const { t } = useTranslation();
  const [snapshot, setSnapshot] = useState<CodexProtectionSnapshot | null>(
    null,
  );
  const [paths, setPaths] = useState<string[][]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [editor, setEditor] = useState<CodexProtectedEditor | null>(null);
  const [text, setText] = useState("");
  const owner = useRef(0);
  const rulesDirty =
    snapshot &&
    JSON.stringify(paths) !== JSON.stringify(snapshot.protectedPaths);
  const editorDirty = editor && text !== editor.text;
  const load = useCallback(async () => {
    const generation = owner.current;
    setBusy(true);
    setError("");
    try {
      const next = await codexProtectionApi.get();
      if (generation !== owner.current) return;
      setSnapshot(next);
      setPaths(next.protectedPaths);
    } catch (e) {
      if (generation === owner.current) setError(String(e));
    } finally {
      if (generation === owner.current) setBusy(false);
    }
  }, []);
  useEffect(() => {
    owner.current += 1;
    setSnapshot(null);
    setEditor(null);
    setText("");
    void load();
    return () => {
      owner.current += 1;
    };
  }, [configDir, load]);

  const saveRules = async () => {
    if (!snapshot) return;
    const generation = owner.current;
    setBusy(true);
    setError("");
    try {
      const rulesRevision = await codexProtectionApi.save(
        paths,
        snapshot.rulesRevision,
      );
      if (generation !== owner.current) return;
      setSnapshot({ ...snapshot, protectedPaths: paths, rulesRevision });
      toast.success(t("codexProtection.rulesSaved"));
    } catch (e) {
      if (generation === owner.current) setError(String(e));
    } finally {
      if (generation === owner.current) setBusy(false);
    }
  };
  const readEditor = async () => {
    const generation = owner.current;
    setBusy(true);
    setError("");
    try {
      const next = await codexProtectionApi.readEditor();
      if (generation !== owner.current) return;
      setEditor(next);
      setText(next.text);
    } catch (e) {
      if (generation === owner.current) setError(String(e));
    } finally {
      if (generation === owner.current) setBusy(false);
    }
  };
  const saveText = async () => {
    if (!editor) return;
    const generation = owner.current;
    setBusy(true);
    setError("");
    try {
      await codexProtectionApi.saveEditor(editor, text);
      if (generation !== owner.current) return;
      const next = await codexProtectionApi.readEditor();
      if (generation !== owner.current) return;
      setEditor(next);
      setText(next.text);
      // Refresh values without throwing away unsaved protection selections.
      const current = await codexProtectionApi.get();
      if (generation !== owner.current) return;
      setSnapshot((previous) =>
        previous
          ? {
              ...previous,
              fields: current.fields,
              configRevision: current.configRevision,
              warning: current.warning,
            }
          : current,
      );
      toast.success(t("codexProtection.tomlSaved"));
    } catch (e) {
      if (generation === owner.current) setError(String(e));
    } finally {
      if (generation === owner.current) setBusy(false);
    }
  };
  const toggle = (path: string[], checked: boolean) =>
    setPaths((current) =>
      checked
        ? [...current.filter((p) => keyOf(p) !== keyOf(path)), path]
        : current.filter((p) => keyOf(p) !== keyOf(path)),
    );

  return (
    <section
      className="min-w-0 space-y-3 border-t border-border px-4 py-4"
      aria-label={t("codexProtection.title")}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="flex items-center gap-2 text-sm font-medium">
          <ShieldCheck className="h-4 w-4" />
          {t("codexProtection.title")}
        </h3>
        <div className="flex flex-wrap gap-2">
          <HoverTip content={t("codexProtection.refresh")}>
            <Button
              variant="ghost"
              size="icon"
              type="button"
              aria-label={t("codexProtection.refresh")}
              disabled={busy || !!rulesDirty}
              onClick={() => void load()}
            >
              <RefreshCw className="h-4 w-4" />
            </Button>
          </HoverTip>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={busy || !snapshot || !rulesDirty}
            onClick={() => void saveRules()}
          >
            <Save className="mr-1 h-4 w-4" />
            {t("codexProtection.saveRules")}
          </Button>
        </div>
      </div>
      {snapshot && (
        <p className="break-all font-mono text-xs text-fg-2">
          {snapshot.configPath}
        </p>
      )}
      <p className="text-xs text-fg-2">{t("codexProtection.scope")}</p>
      <p className="text-xs text-fg-2">{t("codexProtection.catalogNotice")}</p>
      {error && (
        <p
          role="alert"
          className="whitespace-pre-wrap break-words text-sm text-destructive"
        >
          {error}
        </p>
      )}
      {snapshot?.warning && (
        <p role="alert" className="text-sm text-warning-text">
          {snapshot.warning}
        </p>
      )}
      <div className="relative">
        <Search className="absolute left-2 top-2.5 h-4 w-4 text-fg-2" />
        <Input
          className="pl-8"
          aria-label={t("codexProtection.search")}
          placeholder={t("codexProtection.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <div className="max-h-80 overflow-auto border-y border-border">
        <table className="w-full table-fixed text-xs">
          <thead className="sticky top-0 bg-background text-left">
            <tr>
              <th className="w-16 whitespace-nowrap p-2">
                {t("codexProtection.protect")}
              </th>
              <th className="w-[42%] p-2">{t("codexProtection.key")}</th>
              <th className="p-2">{t("codexProtection.value")}</th>
            </tr>
          </thead>
          <tbody>
            {snapshot?.fields
              .filter((field) =>
                field.name.toLowerCase().includes(query.toLowerCase()),
              )
              .map((field) => {
                const inherited = includesParent(paths, field.path);
                const checked =
                  inherited ||
                  paths.some((p) => keyOf(p) === keyOf(field.path));
                return (
                  <tr
                    key={keyOf(field.path)}
                    className="border-t border-border/50 align-top"
                  >
                    <td className="p-2">
                      <Checkbox
                        checked={checked}
                        disabled={busy || inherited}
                        onCheckedChange={(value) => toggle(field.path, value)}
                        aria-label={`${t("codexProtection.protect")} ${field.name}`}
                      />
                    </td>
                    <td className="break-all p-2 font-mono">{field.name}</td>
                    <td className="whitespace-pre-wrap break-all p-2 font-mono text-fg-2">
                      {!field.present
                        ? t("codexProtection.absent")
                        : field.sensitive
                          ? t("codexProtection.hidden")
                          : field.value}
                    </td>
                  </tr>
                );
              })}
          </tbody>
        </table>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={busy || !snapshot || paths.length === 0}
          onClick={() => setPaths([])}
        >
          {t("codexProtection.clear")}
        </Button>
        {rulesDirty && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={busy}
            onClick={() => void load()}
          >
            {t("codexProtection.discardRules")}
          </Button>
        )}
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy || !!editorDirty}
          onClick={() => void readEditor()}
        >
          <FilePenLine className="mr-1 h-4 w-4" />
          {t("codexProtection.edit")}
        </Button>
      </div>
      {editor && (
        <div className="space-y-2">
          <p className="text-xs text-fg-2">
            {t("codexProtection.manualNotice")}
          </p>
          <textarea
            spellCheck={false}
            aria-label={t("codexProtection.editor")}
            value={text}
            onChange={(e) => setText(e.target.value)}
            disabled={busy}
            className="min-h-64 w-full resize-y rounded-md border border-input bg-background p-3 font-mono text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={busy}
              onClick={() => {
                setEditor(null);
                setText("");
              }}
            >
              {t("codexProtection.discard")}
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy || !editorDirty}
              onClick={() => void saveText()}
            >
              <Save className="mr-1 h-4 w-4" />
              {t("codexProtection.saveToml")}
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}
