import type { CodexApiFormat, CodexModelRouteMode } from "@/types";

// Match CodexCatalogToolProfile::from_route_mode in the backend.
export function normalizeCodexRouteMode(
  value: unknown,
): CodexModelRouteMode | undefined {
  if (typeof value !== "string") return undefined;
  switch (value.trim().toLowerCase()) {
    case "chat":
    case "openai_chat":
    case "openai-chat":
      return "chat";
    case "responses":
    case "openai_responses":
    case "openai-responses":
      return "responses";
    case "anthropic":
    case "anthropic_messages":
    case "anthropic-messages":
    case "messages":
    case "claude":
      return "anthropic";
    default:
      return undefined;
  }
}

export function defaultCodexRouteMode(
  format: CodexApiFormat,
): CodexModelRouteMode {
  return normalizeCodexRouteMode(format)!;
}
