# Official and experimental AI providers are deferred

M6 named official usage/cost APIs and experimental endpoints, but no endpoint paths, response shapes, or terms-of-service approval were supplied. Rather than invent an API or issue an unapproved request, only local-log and local-database providers were implemented; the config keeps an `experimental` flag and provider ids available, but no network request is ever made. The deferred work is tracked in a follow-up issue so M6 stays open until real providers exist.

## Status update

The user subsequently approved provider terms and supplied `Javis603/token-monitor` as a source of real endpoint paths and response shapes. Official providers for Claude, Codex, and OpenCode are now implemented against those observed shapes, with credentials stored in the Windows Credential Manager via `wayward --set-credential <id>` and never written to TOML or logs.

The **experimental endpoint** half was initially deferred because no undocumented endpoint with a real response sample was identified. On clarification, the supplied reference repo does contain such endpoints: Manus `POST https://api.manus.im/user.v1.UserService/GetAvailableCredits` (Connect RPC, field names and types confirmed against a live response) and Hark `GET https://hark.com/api/billing/summary`. Manus is implemented as `manus-experimental`, gated by BOTH `ai.experimental = true` and an enabled provider entry, and fails closed on unknown response shapes. The flag and its warning comment are documented in the config template. Hark remains unimplemented; it is an optional follow-up, not a blocker for the two M6 issues.
