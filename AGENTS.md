# AGENTS.md: wayward

Instructions for AI coding agents working on **wayward**. Read this whole file before writing code.

## 1. What we are building

**wayward** is a native status bar for **Windows**, written in **Rust** with **iced**. It imitates the look and feel of Omarchy's Waybar setup: blocky, minimal, straightforward, unbloated.

Hard constraints:

- Windows only (Windows 10 and 11). Do not add Linux or macOS code or `cfg` branches for them.
- Native only. **No webview, no Electron, no Tauri, no embedded browser.**
- Lightweight. Low idle CPU, small binary, few dependencies. This runs all day.
- Real bar behavior: it reserves screen space like the taskbar (AppBar API), not just a floating window.

Non-goals (do not build unless asked): tray icon hosting, replacing the Windows taskbar, plugin system, scripting language, cross-platform support.

## 2. Tech stack

| Concern | Choice |
|---|---|
| Language | Rust (stable, edition 2021 or later) |
| UI | `iced` with the `tiny-skia` renderer by default (CPU, no GPU context). `wgpu` stays optional behind a feature flag. |
| Win32 | `windows` crate (official bindings) |
| System stats | `sysinfo` |
| Battery | `starship-battery` or `GetSystemPowerStatus` |
| Time | `jiff` or `chrono` |
| Config | `serde` + `toml` to read, `toml_edit` to write (preserves comments), `dirs` for the home folder, hot reload with `notify` |
| Secrets | `keyring` (Windows Credential Manager) |
| Async | `tokio` (only what iced and the providers need) |
| Logging | `tracing` + `tracing-subscriber` |

Before using any crate, **check its latest version and docs**. iced's API changes between releases (multi-window, window handles, subscriptions), so do not write code from memory. Read the docs of the version you install.

Ask before adding any dependency not listed here. Prefer fewer dependencies.

## 3. Workspace layout

```
wayward/
├─ Cargo.toml                 # workspace
├─ AGENTS.md
├─ config/
│  └─ wayward.toml            # bundled default config, copied to the user config dir on first run
└─ crates/
   ├─ wayward-core/           # Module trait, config, theme, shared types (no Win32, no iced)
   ├─ wayward-modules/        # clock, cpu, ram, battery, volume, network, window title
   ├─ wayward-ai/             # AI usage: UsageProvider trait + providers
   ├─ wayward-platform/       # ALL Win32 code: AppBar, window styles, DPI, hooks
   └─ wayward-app/            # iced app, views, popups, main()
```

Rules:

- `wayward-core` and `wayward-ai` must not depend on `iced` or `windows`.
- All `unsafe` and all Win32 calls live in `wayward-platform`. Every `unsafe` block gets a `// SAFETY:` comment.
- UI code only consumes plain data structs from modules; it never does I/O.

## 4. Platform behavior (the hard part, do it right)

Register the bar as a Windows AppBar using `SHAppBarMessage`:

1. Create a frameless, always-on-top window with `WS_EX_TOOLWINDOW` (no taskbar button, no Alt+Tab entry).
2. `ABM_NEW` to register.
3. `ABM_QUERYPOS` then `ABM_SETPOS` with `ABE_TOP` (configurable: top or bottom) and the bar height in **physical pixels**.
4. Handle the AppBar callback message: `ABN_POSCHANGED`, `ABN_FULLSCREENAPP` (hide when a fullscreen app is active, restore after).
5. `ABM_REMOVE` on exit **and on panic/crash paths where possible**. Otherwise the user's work area stays shrunk. Add a drop guard.
6. Per-monitor DPI: enable `PerMonitorV2` via manifest. Recompute height on `WM_DPICHANGED`.
7. Handle display changes (`WM_DISPLAYCHANGE`, `WM_SETTINGCHANGE`) by re-running `QUERYPOS/SETPOS`.

Get the HWND through iced's raw window handle support. Check the docs for the installed iced version to see how to access it and how to run code with the handle.

Start with the primary monitor only. Multi-monitor comes later.

## 5. Design rules (Omarchy look)

- Blocky: rectangles, square corners, 1px borders, no shadows, no gradients, no blur.
- Monospace font, with a Nerd Font for icons. Bundle the font or document the install step.
- Colors come only from the theme: `background`, `foreground`, `accent`, `muted`, `warn`, `urgent` (plus optional `surface`, `success`). Never hardcode a color in a view. The default theme is **Catppuccin Mocha** (see section 5.1).
- Modules are flat cells separated by spacing or a thin divider.
- Straightforward: no animations beyond hover state, no decorative elements, no tooltips that repeat what is already visible.
- Everything configurable lives in `wayward.toml` (see section 5.1): bar position, height, font, module order (left / center / right), theme. The same options are editable from the settings GUI.

## 5.1 Configuration and settings GUI

Users can change the bar's style in two ways, and both edit the **same file**:

1. Edit TOML by hand.
2. Use a settings window (GUI) inside wayward.

**Config location**

```
C:\Users\<user>\.config\.wayward\
├─ wayward.toml        # main config: bar, layout, theme, modules, providers
└─ themes\             # optional named themes (e.g. tokyo-night.toml)
```

- Resolve with `dirs::home_dir()` joined with `.config/.wayward`. Do **not** use `%APPDATA%` or `%LOCALAPPDATA%`.
- Allow an override with the `WAYWARD_CONFIG_DIR` environment variable (useful for testing).
- On first run, create the folder and write a commented default `wayward.toml` (the bundled copy in `config/`). **Never overwrite an existing file.**
- Add CLI helpers: `wayward --config-path` (print the folder) and `wayward --open-config` (open it in Explorer).

**What is configurable** (all of it, from TOML or GUI)

- Bar: position (top/bottom), height, padding, monitor
- Font: family and size
- Theme: `background`, `foreground`, `accent`, `muted`, `warn`, `urgent` as hex colors, or `theme = "name"` to load from `themes\`
- Layout: which modules appear and their order in left / center / right
- Module options: clock format, stat intervals, and AI provider enable/disable and poll intervals
- Secrets (API keys, tokens) are **never** in TOML. They go through `keyring`.

**Default theme: Catppuccin Mocha**

The bundled default `wayward.toml` uses `theme = "catppuccin-mocha"`, and the same palette is shipped as `themes\catppuccin-mocha.toml`. Palette mapping:

```toml
[theme]
name       = "catppuccin-mocha"
background = "#1e1e2e"   # base
foreground = "#cdd6f4"   # text
accent     = "#cba6f7"   # mauve
muted      = "#6c7086"   # overlay0
warn       = "#f9e2af"   # yellow
urgent     = "#f38ba8"   # red

# optional extras the popup and settings window may use
surface    = "#313244"   # surface0, for borders and empty progress cells
success    = "#a6e3a1"   # green, for healthy usage levels
```

Rules:

- Use these values exactly. If a different shade is needed, add a new named key to the theme instead of hardcoding a color in a view.
- If the user's `theme` name cannot be found, or a key is missing, fall back to Catppuccin Mocha values, not to arbitrary colors.
- Other flavors (Latte, Frappé, Macchiato) and other palettes (Tokyo Night, etc.) can be added later as files in `themes\`. Do not build them now unless asked.

**Behavior rules**

- **TOML is the single source of truth.** The GUI reads from and writes to `wayward.toml`. It keeps no separate state.
- Write with `toml_edit` so user comments, ordering, and formatting survive GUI edits. Do not re-serialize the whole file with `toml`.
- Hot reload with `notify`, debounced. Ignore changes caused by wayward's own writes so the GUI and watcher don't loop.
- If the file is invalid, keep the **last good config**, log the error, and show a small error state (for example, the settings window shows the parse error with line number). The bar must never crash or go blank because of a bad edit.
- Unknown keys are warnings, not errors. Missing keys fall back to defaults.
- Validate values (hex colors, ranges for height and font size) and report clear messages.

**Settings window**

- A separate borderless iced window styled with the same blocky theme as the bar (flat rectangles, 1px borders, monospace, no shadows).
- Opened by right-clicking the bar (menu item "Settings") and by `wayward --settings`.
- Sections: Bar, Theme, Layout, Modules, AI providers.
- Theme editing: hex input with a swatch preview; changes apply live to the bar, and are saved on Apply (or immediately, but be consistent and document which).
- Layout editing: enable/disable modules and reorder them. Keep the UI simple; buttons for move up/down are enough, drag and drop is not required.
- Include "Open config file" and "Reset to defaults" (with confirmation).
- Keyboard accessible: `Tab` navigation, `Esc` closes.

## 6. Modules

Each module is a plain struct implementing:

```rust
pub trait Module: Send + 'static {
    fn id(&self) -> &'static str;
    fn interval(&self) -> Option<std::time::Duration>;
    fn update(&mut self) -> ModuleState;          // text, icon, level (normal/warn/urgent)
    fn on_click(&mut self, _button: MouseButton) {}
}
```

Initial modules, in this order: `clock`, `cpu`, `ram`, `battery`, `volume`, `network`, `active_window`, `ai`.

Rules:

- A failing module shows `--` and logs a warning. It must never crash or freeze the bar.
- No module polls faster than it needs to. The clock ticks once per second at most; stats every 2 to 5 seconds.
- Idle CPU target: effectively 0% between ticks. Use iced subscriptions and timers, not busy loops.

## 7. AI usage module

This is the signature feature.

**In the bar:** a single AI icon, styled like every other module. No numbers. The icon's color reflects the worst state across providers: normal, warn (any provider under ~20% left), urgent (any provider at 0 or limit hit). Alert coloring is configurable.

**On click:** toggle a popup. Right click refreshes. `Esc` or clicking outside closes it.

**Popup:** a separate borderless, topmost tool window anchored under (or above, if the bar is at the bottom) the icon, clamped to the monitor work area, closing on focus loss. It lists one flat block per provider:

```
┌─ AI USAGE ─────────────────── [left|used] ┐
│ CLAUDE CODE                          MAX  │
│ session  ██████░░░░  62%    reset 1h51m   │
│ weekly   ███░░░░░░░  36%    reset 2d18h   │
├───────────────────────────────────────────┤
│ CODEX                            PRO 200  │
│ session  █████████░  88%    reset 3h40m   │
└───────────────────────────────────────────┘
 updated 4s ago                    r refresh
```

Progress bars are rows of filled and empty rectangles, not gradients. Keyboard: `r` refresh, `l` / `u` toggle left/used, `Esc` close.

**Provider interface:**

```rust
#[async_trait::async_trait]
pub trait UsageProvider: Send {
    fn id(&self) -> &'static str;
    async fn fetch(&mut self) -> anyhow::Result<Usage>;
}

pub struct Usage {
    pub plan: Option<String>,
    pub meters: Vec<Meter>,    // label, used, limit, reset_at
    pub spend: Option<Spend>,  // used, limit, currency
}
```

Rules for providers:

- Each provider has its own poll interval (default 5 minutes) with exponential backoff on failure. Show cached data instantly when the popup opens, plus an "updated Ns ago" line.
- Prefer **local data** (log and session files) over network calls. Start with Claude Code and OpenCode local logs.
- Then official usage/cost APIs (they need user-supplied admin keys).
- Undocumented endpoints go last, behind an `experimental` config flag, with a clear comment that they can break. Do not invent endpoints or response shapes. If you don't know a format, inspect real data or ask me.
- Never store credentials in the config file. Use `keyring`. Never log tokens or keys.
- Respect each provider's terms. If something looks questionable, stop and ask.

## 8. Milestones (work in this order, one per PR/commit series)

1. **Skeleton.** Workspace, CI-ready `cargo fmt` / `clippy`, iced window with a clock, config loading.
   *Done when:* `cargo run` shows a frameless bar with a live clock.
2. **AppBar.** Reserve screen space, DPI handling, fullscreen hiding, clean `ABM_REMOVE` on exit.
   *Done when:* maximized windows stop below the bar, and exiting restores the work area, including after Ctrl+C.
3. **Module system.** `Module` trait, subscriptions, `cpu`, `ram`, `battery`.
   *Done when:* modules update on their intervals and a failing module shows `--`.
4. **Theme and config.** Config folder at `~\.config\.wayward\` (created with defaults on first run), TOML theme, module layout (left/center/right), debounced hot reload, last-good-config fallback on invalid TOML.
   *Done when:* a fresh install renders with the Catppuccin Mocha theme, editing `wayward.toml` updates the bar without restart, and a broken edit leaves the bar running with the previous config.
5. **AI icon and popup with fake data.** Icon module, popup window, positioning, close-on-blur, keyboard shortcuts.
   *Done when:* the popup matches section 7 using mock providers.
6. **Real providers.** Local-log providers first, then official APIs, then experimental ones.
7. **Remaining modules.** `volume` (WASAPI), `network`, `active_window` (`SetWinEventHook`).
8. **Settings GUI.** Settings window (Bar, Theme, Layout, Modules, AI providers) that edits `wayward.toml` through `toml_edit`, opened from the bar's right-click menu and `wayward --settings`.
   *Done when:* a change made in the GUI appears in the TOML file with the user's comments intact, and a change made in the TOML file appears in the GUI, with no feedback loop.
9. **Polish.** Multi-monitor, autostart option, installer or portable zip, release build size check.

## 9. How to work

- Work on **one milestone at a time**. Do not start the next until the current one meets its "done when".
- Small, reviewable commits with clear messages.
- Before finishing any task run: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and fix everything.
- Add unit tests for pure logic (config parsing, usage math, time formatting). Win32 code gets a short manual test note in the PR description.
- When something is ambiguous or requires a decision (new dependency, design deviation, provider terms), **ask instead of guessing**.
- Do not refactor unrelated code, rename things for style, or add features not in this document.
- Keep the release binary small: `lto = "thin"` or `"fat"`, `codegen-units = 1`, `strip = true`, `panic = "abort"` if compatible with the AppBar cleanup guard (verify before enabling).
- Report at the end of each task: what changed, what you tested, what is left, and any assumptions.

## 10. Definition of done (whole project, v0.1)

- Runs on Windows 10 and 11 with correct DPI scaling.
- Reserves screen space and releases it cleanly on exit.
- Idle CPU near 0%, memory in the low tens of MB.
- Single AI icon with a working popup showing at least two real providers.
- Theme and layout fully driven by `~\.config\.wayward\wayward.toml` with hot reload.
- Every style option is editable from the settings GUI or by hand in the TOML, with both staying in sync.
