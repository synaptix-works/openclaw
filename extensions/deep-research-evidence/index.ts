import { definePluginEntry } from "openclaw/plugin-sdk/plugin-entry";

// Skills-only plugin: capability ships via the manifest `skills` pack plus the
// native `dr` binary (crates/dr) on PATH. No runtime registration needed.
export default definePluginEntry({
  id: "deep-research-evidence",
  name: "Deep Research Evidence",
  description: "Citation-tracked research pipeline with evidence/claim ledgers and verification, backed by the native `dr` CLI.",
  register() {},
});
