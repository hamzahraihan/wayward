# Official and experimental AI providers are deferred

M6 named official usage/cost APIs and experimental endpoints, but no endpoint paths, response shapes, or terms-of-service approval were supplied. Rather than invent an API or issue an unapproved request, only local-log and local-database providers were implemented; the config keeps an `experimental` flag and provider ids available, but no network request is ever made. The deferred work is tracked in a follow-up issue so M6 stays open until real providers exist.

## Status update

The user subsequently approved provider terms and supplied `Javis603/token-monitor` as a source of real endpoint paths and response shapes. Official providers for Claude, Codex, and OpenCode are now implemented against those observed shapes, with credentials stored in the Windows Credential Manager via `wayward --set-credential <id>` and never written to TOML or logs.

The **experimental endpoint** half remains deferred. No undocumented endpoint, real response sample, or explicit per-endpoint approval was supplied, and inventing one is forbidden by the M6 spec. The `experimental` config flag continues to gate code paths that do not exist yet. That is the only remaining blocker to closing the follow-up issue.
