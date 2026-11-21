# Official and experimental AI providers are deferred

M6 named official usage/cost APIs and experimental endpoints, but no endpoint paths, response shapes, or terms-of-service approval were supplied. Rather than invent an API or issue an unapproved request, only local-log and local-database providers were implemented; the config keeps an `experimental` flag and provider ids available, but no network request is ever made. The deferred work is tracked in a follow-up issue so M6 stays open until real providers exist.
