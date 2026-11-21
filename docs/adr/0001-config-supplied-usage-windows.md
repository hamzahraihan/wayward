# Local AI providers report usage windows, not vendor quota

Claude Code and OpenCode local logs contain token counts but no limit or reset time. Instead of inventing a quota shape or deferring every provider, local providers report a usage window: real token consumption compared against a user-defined limit and reset interval in `wayward.toml`. Vendor-reported quota remains a separate concept for future official API providers. This keeps the bars honest without credentials or fabricated data.
