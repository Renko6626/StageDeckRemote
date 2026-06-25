# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

StageDeck Remote — a LAN PowerPoint remote for live stage events. A **director** runs the Tauri v2 desktop app (Rust core + Vue 3 GUI) on their PC; a **presenter** controls it from a phone/tablet browser (scan a QR, no install) served by an embedded Axum HTTP server. Runtime target is **Windows** (native Win32 screen capture / input / window enumeration, plus PowerPoint COM automation).

## Disciplines (non-negotiable)

1. **Tauri build environment.** This box *can* build the app and Windows installers — via the conda envs in **Build / test / run** below. NEVER run a bare `cargo build` / `tauri dev` / `tauri build`, and NEVER conclude "Tauri can't be built here": no system `webkit2gtk` + no sudo make the bare commands fail, but the `tauri-dev` (native) and `xwin` + cargo-xwin (Windows cross-compile) envs work. Always use the env-prefixed commands, and verify any Win32/COM/FFI change with the `xwin` cross-compile (0 errors) before claiming it builds.

2. **Version bump on every build.** Every time you produce an installer, first bump the **patch** version in all three of `tauri.conf.json`, `package.json`, and `src-tauri/Cargo.toml` (the build then syncs `Cargo.lock`), and commit it as `chore: bump version X → Y`. The version is embedded in the installer filename, so successive builds must be distinguishable.

## Build / test / run

> **This dev box is Linux with no system `webkit2gtk` and no sudo.** A bare `cargo build` / `tauri dev` / `tauri build` of the Tauri crate **fails** out of the box. Do NOT conclude "can't build here." Use the two pre-installed user-level conda envs (paths are this machine's; also documented in `~/.claude/CLAUDE.md`).

**Repo is a Cargo workspace** (v0.2.0+): root `Cargo.toml` with members `["src-tauri", "crates/protocol", "crates/relay"]`. Workspace `Cargo.lock` at the root is authoritative; the `src-tauri/Cargo.lock` left over from before workspace-ification is dead, kept around for `pnpm tauri build` convention but Cargo ignores it.

**Native Linux build + Rust tests** (whole workspace; native modules compile as `#[cfg(not(windows))]` stubs):
```bash
P=/data/sunyunbo/miniconda3/envs/tauri-dev
PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH \
  cargo test --workspace
# single test:  ... cargo test --workspace <test_name>
# relay only:   ... cargo test -p relay
# relay e2e integration test (Linux loopback, no Windows/Tauri needed):
#                ... cargo test --workspace --test end_to_end -- --nocapture
```

**Windows cross-compile** (the authoritative check that Win32/COM FFI compiles — no webkit2gtk needed, WebView2 target). Always run from `src-tauri/` (Tauri CLI conventions):
```bash
cd src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
  cargo xwin build --target x86_64-pc-windows-msvc
```
`relay` is **Linux-only** and is NOT cross-compiled to Windows.

**Windows installer (NSIS)**:
```bash
XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
  pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
# output: target/x86_64-pc-windows-msvc/release/bundle/nsis/StageDeck Remote_<ver>_x64-setup.exe
# (workspace target/ at repo root since v0.2.0; pre-workspace builds went to src-tauri/target/)
```
Bump the version before building — see **Disciplines #2**.

**Linux relay binary** (for CLAB deployment; v0.2.0+):
```bash
cargo build --release -p relay
# output: target/release/relay  (Linux x86_64; ~3 MB)
```
Both installer + relay binary should land in top-level `out/` per the user's pickup convention.

**Frontend** (package manager is **pnpm**, not npm — lockfile is `pnpm-lock.yaml`):
```bash
pnpm install
pnpm build      # vue-tsc --noEmit && vite build  (typechecks the Vue/TS director GUI)
pnpm typecheck  # vue-tsc only
```

**Running the real app** requires a Windows host (`pnpm tauri dev`); native capture/input/window-enum/COM only do real work on Windows. On Linux you can build/typecheck/unit-test only.

## Architecture (the parts that span files)

**Two channels, two trust levels, one shared state.** The process exposes the director and the presenter through different surfaces, both sharing one `Arc<AppState>` (`app_state.rs`, `Mutex<Inner>`):
- **Director = privileged, via Tauri IPC** (`commands.rs`, registered in `lib.rs`): enumerate/select window, remote on/off, emergency lock, local page-turn. These are **never bound to a network port** — unreachable from the LAN by construction.
- **Presenter = restricted, via Axum HTTP** on `0.0.0.0:8787` (`web_server.rs` + `routes.rs`): view snapshot, click, page-turn, lock, trigger macro. Every endpoint is token-gated. `web_server.rs`/`routes.rs` depend only on `Arc<AppState>`, **not on `tauri`** — that's why the service layer compiles without webkit2gtk.

**Three-layer lock** (in `app_state.rs`; semantics are CURRENT — see Doc staleness):
- `remote_enabled` (director master switch, default **OFF**) → `control_locked` (director **emergency** lock, default **OFF**) → `presenter_unlocked` (presenter "ARM", default OFF).
- `button_allowed = remote_enabled && !control_locked` — now gates **only** `/api/unlock` (the precondition for the presenter to ARM).
- `click_allowed = remote_enabled && !control_locked && presenter_unlocked` — gates **all** presenter action endpoints (`next`/`prev`/`black`/`restore`/`action`/`click`). So once ARMed-off the presenter can do nothing; the director strictly dominates, and `force_lock(true)` / `set_remote_enabled(false)` both clear `presenter_unlocked`.

**PowerPoint control = raw IDispatch COM** (`ppt_com.rs`): connects to the running instance via `GetActiveObject("PowerPoint.Application")` (window-agnostic — independent of the captured window). `slide_state()` enumerates the *current* slide's macro-bound shapes (`Shape.ActionSettings(ppMouseClick).Action == 8 /*ppActionRunMacro*/ → .Run`). `trigger_action()` takes a **shape name**, re-reads the bound macro server-side (don't trust a client-supplied macro name), and calls `Application.Run(macro, shape)` **with a no-arg fallback** — many quiz templates share one handler `Sub Foo(shp As Shape)` across buttons and need the clicked shape passed. Page-turn/black go through `SlideShowWindows(1).View`.

**Presenter UI is vanilla JS, embedded at compile time.** `src/presenter/{presenter.html,presenter.css,presenter.js,favicon.svg}` are served by Axum via `include_str!` in `routes.rs` **and** by Relay via `include_str!` in `crates/relay/src/phone_routes.rs` → **editing them requires recompiling BOTH binaries** for the change to propagate. They are NOT part of the Vite build and NOT typechecked. The HTML has `<base href="__BASE__" />` placeholder; LAN injects `/presenter/`, Relay injects `/r/<sid>/presenter/`; all JS API calls are relative (`fetch("api/status?token=...")`). Layout is phone-first one-handed: a bottom transport dock (giant full-width "下一页") pinned across both modes, 2-col macro buttons, and an ARM strip; client-side gating mirrors the backend (`canAct()` ⇔ `click_allowed`).

**Capture/stream pipeline** (v0.1.16+): WGC background session writes raw BGRA → encoder thread does sampled-hash change detection → on change, encodes JPEG q82 once and publishes to `frame_broker` (`watch::Sender<Arc<EncodedFrame>>`) → two fan-outs: presenter `GET /api/stream.mjpg` (multipart) and director `subscribe_preview` IPC (Tauri `Channel<tauri::ipc::Response>` → canvas). Static slides → ≈ zero idle CPU. `snapshot.jpg` + `get_snapshot` IPC kept as one-shot fallbacks. See `docs/adr/0006-capture-stream-broker.md`.

**Relay architecture** (v0.2.0+): the same `Arc<axum::Router>` is reused two ways. LAN path: `web_server::run_with_router` binds `0.0.0.0:8787`. Relay path: `relay_client::spawn` opens two outbound WS to a CLAB-hosted `relay` binary, control WS receives `TunnelReq` from relay and feeds the router via `tower::ServiceExt::oneshot`, stream WS subscribes to broker and pushes JPEG bytes upward. **routes.rs business logic is unchanged**. Relay re-fans the stream as `multipart/x-mixed-replace` to phones; phones never know the difference. See `docs/adr/0007-relay-hybrid-tunnel.md`.

**Director GUI** (`src/`, Vue 3 + Pinia + Vue Router): `views/DirectorView.vue` composes `components/*`; `utils/api.ts` wraps IPC; `stores/` holds state; it polls `get_status` + `get_logs` every 1.5s. **Preview frames are NOT in that poll** — they come via `subscribe_preview` Tauri Channel push and render to canvas. The module map in `docs/architecture.md §7` is the authoritative source for *what each file does*.

## Platform pattern & gotchas

- Native modules (`capture`, `capture_session`, `input`, `window_manager`, `ppt_com`) are `#[cfg(windows)]` real impl + `#[cfg(not(windows))]` stub. Keep this split when adding native code so the service layer keeps compiling on Linux.
- **COM VARIANT gotcha** (regressed twice): PowerPoint returns `VT_DISPATCH (9)`; always extract objects with `IDispatch::try_from(&VARIANT)`, never `IUnknown::try_from`.
- COM behavior cannot be unit-tested on Linux — verify it via **cross-compile (0 errors) + runtime on Windows**. Pure functions (`parse_macro_names`, `ppt_window_rank`, lock predicates, normalized→screen coord mapping) ARE Linux-unit-testable; prefer extracting logic into such functions.
- The process sets Per-Monitor-DPI-Aware v2 (`lib.rs`) so window rects, screen coords, and `SendInput` all share physical pixels.

## Workflow conventions (the user enforces these)

- **No one-shot generation.** Every change goes brainstorm → spec → plan → implement → review, in small auditable commits. Specs live in `docs/superpowers/specs/YYYY-MM-DD-*.md`, plans in `docs/superpowers/plans/`. Use the `superpowers` skills (brainstorming → writing-plans → subagent-driven-development) and the dated specs/plans as the source of truth for recent behavior.
- Per-feature branch off `master`, then local merge (no PR unless asked).

## Doc currency

As of v0.2.0 + the 2026-06-26 doc refresh, the long-form docs are caught up to current code:

- `README.md`, `docs/architecture.md`, `docs/protocol.md`, `docs/network.md` cover both LAN and Relay modes, the change-driven capture pipeline, and the workspace structure.
- ADRs include the two new architectural decisions: `docs/adr/0006-capture-stream-broker.md` (capture/stream rework, supersedes 0005) and `docs/adr/0007-relay-hybrid-tunnel.md` (relay).
- `docs/superpowers/specs/*` and `docs/superpowers/plans/*` remain the brainstorm→spec→plan→implement record per dated sub-project; they are NOT edited after landing — they're history.
- When in doubt, trust the code first, then the newest spec under `docs/superpowers/specs/`, then the long-form docs.
