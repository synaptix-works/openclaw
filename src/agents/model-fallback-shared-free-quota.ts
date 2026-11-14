/** Account-wide free-tier exhaustion from the HAOMA OpenRouter gate.
 * Only exact machine-readable markers qualify: generic HTTP 429 may be model-scoped.
 * Never read prompts or user text, and never collapse independent providers.
 */
const FREE_ACCOUNT_QUOTA_MARKER =
  /(?:FREE_PROVIDER_DAILY_QUOTA_EXHAUSTED|OPENROUTER_FREE_DAILY_QUOTA_EXHAUSTED|free_model_daily_quota_exhausted)/;

export function isOpenRouterFreeAccountQuotaError(provider: string, error: unknown): boolean {
  if (provider !== "openrouter") {
    return false;
  }
  const seen = new Set<object>();
  let current: unknown = error;
  for (let depth = 0; depth < 6 && current && typeof current === "object"; depth += 1) {
    if (seen.has(current)) {
      break;
    }
    seen.add(current);
    const row = current as { code?: unknown; message?: unknown; cause?: unknown; error?: unknown };
    if (
      (typeof row.code === "string" && FREE_ACCOUNT_QUOTA_MARKER.test(row.code)) ||
      (typeof row.message === "string" && FREE_ACCOUNT_QUOTA_MARKER.test(row.message))
    ) {
      return true;
    }
    current = row.cause ?? row.error;
  }
  return false;
}
