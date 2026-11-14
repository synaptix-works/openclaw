import { expect, it, vi } from "vitest";
import { FailoverError } from "./failover-error.js";
import { runWithModelFallback } from "./model-fallback-runner.js";
import { isOpenRouterFreeAccountQuotaError } from "./model-fallback-shared-free-quota.js";

const primary = "openrouter/nvidia/nemotron-3-ultra-550b-a55b:free";
const alternate = "openrouter/nvidia/nemotron-3.5-lightning:free";
const independent = "other-free-provider/healthy-model";

function exhausted() {
  return new FailoverError("OPENROUTER_FREE_DAILY_QUOTA_EXHAUSTED", {
    provider: "openrouter",
    model: "nvidia/nemotron-3-ultra-550b-a55b:free",
    reason: "rate_limit",
  });
}

it("recognizes only explicit account-wide marker, not normal 429", () => {
  expect(isOpenRouterFreeAccountQuotaError("openrouter", exhausted())).toBe(true);
  expect(
    isOpenRouterFreeAccountQuotaError("openrouter", {
      code: "free_model_daily_quota_exhausted",
    }),
  ).toBe(true);
  expect(
    isOpenRouterFreeAccountQuotaError("openrouter", {
      cause: new Error("FREE_PROVIDER_DAILY_QUOTA_EXHAUSTED"),
    }),
  ).toBe(true);
  expect(isOpenRouterFreeAccountQuotaError("openrouter", new Error("HTTP 429 rate_limit"))).toBe(
    false,
  );
  expect(isOpenRouterFreeAccountQuotaError("other-free-provider", exhausted())).toBe(false);
  const cycle: { cause?: unknown } = {};
  cycle.cause = cycle;
  expect(isOpenRouterFreeAccountQuotaError("openrouter", cycle)).toBe(false);
});

it("stops all same-account free models after confirmed exhausted quota", async () => {
  const run = vi.fn().mockRejectedValue(exhausted());
  await expect(
    runWithModelFallback({
      cfg: undefined,
      provider: "openrouter",
      model: "nvidia/nemotron-3-ultra-550b-a55b:free",
      manifestPlugins: [],
      fallbacksOverride: [alternate],
      run,
    }),
  ).rejects.toThrow();
  expect(run).toHaveBeenCalledTimes(1);
});

it("still permits another independent provider to complete", async () => {
  const run = vi
    .fn()
    .mockRejectedValueOnce(exhausted())
    .mockResolvedValueOnce({ output: "INDEPENDENT_OK" });
  const response = await runWithModelFallback({
    cfg: undefined,
    provider: "openrouter",
    model: "nvidia/nemotron-3-ultra-550b-a55b:free",
    manifestPlugins: [],
    fallbacksOverride: [alternate, independent],
    run,
  });
  expect(run).toHaveBeenCalledTimes(2);
  expect(run.mock.calls[1]?.[0]).toBe("other-free-provider");
  expect(response.result).toEqual({ output: "INDEPENDENT_OK" });
});

it("retains ordinary model-scoped rate-limit fallback behavior", async () => {
  const run = vi.fn().mockRejectedValueOnce(
    new FailoverError("HTTP 429 ordinary rate limit", {
      provider: "openrouter",
      model: "nvidia/nemotron-3-ultra-550b-a55b:free",
      reason: "rate_limit",
    }),
  ).mockResolvedValueOnce({ output: "SIBLING_OK" });
  const response = await runWithModelFallback({
    cfg: undefined,
    provider: "openrouter",
    model: "nvidia/nemotron-3-ultra-550b-a55b:free",
    manifestPlugins: [],
    fallbacksOverride: [alternate],
    run,
  });
  expect(run).toHaveBeenCalledTimes(2);
  expect(response.result).toEqual({ output: "SIBLING_OK" });
});
