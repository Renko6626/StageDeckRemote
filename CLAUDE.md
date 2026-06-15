# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

StageDeck Remote — a LAN PowerPoint remote for live stage events. A **director** runs the Tauri v2 desktop app (Rust core + Vue 3 GUI) on their PC; a **presenter** controls it from a phone/tablet browser (scan a QR, no install) served by an embedded Axum HTTP server. Runtime target is **Windows** (native Win32 screen capture / input / window enumeration, plus PowerPoint COM automation).

## Disciplines (non-negotiable)

1. **Tauri build environment.** This box *can* build the app and Windows installers — via the conda envs in **Build / test / run** below. NEVER run a bare `cargo build` / `tauri dev` / `tauri build`, and NEVER conclude "Tauri can't be built here": no system `webkit2gtk` + no sudo make the bare commands fail, but the `tauri-dev` (native) and `xwin` + cargo-xwin (Windows cross-compile) envs work. Always use the env-prefixed commands, and verify any Win32/COM/FFI change with the `xwin` cross-compile (0 errors) before claiming it builds.

2. **Version bump on every build.** Every time you produce an installer, first bump the **patch** version in all three of `tauri.conf.json`, `package.json`, and `src-tauri/Cargo.toml` (the build then syncs `Cargo.lock`), and commit it as `chore: bump version X → Y`. The version is embedded in the installer filename, so successive builds must be distinguishable.

## Build / test / run

> **This dev box is Linux with no system `webkit2gtk` and no sudo.** A bare `cargo build` / `tauri dev` / `tauri build` of the Tauri crate **fails** out of the box. Do NOT conclude "can't build here." Use the two pre-installed user-level conda envs (paths are this machine's; also documented in `~/.claude/CLAUDE.md`).

**Native Linux build + Rust tests** (service layer + pure-logic; native modules compile as `#[cfg(not(windows))]` stubs):
```bash
P=/data/sunyunbo/miniconda3/envs/tauri-dev
PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH \
  cargo test --manifest-path src-tauri/Cargo.toml
# single test:  ... cargo test --manifest-path src-tauri/Cargo.toml <test_name>
```

**Windows cross-compile** (the authoritative check that Win32/COM FFI compiles — no webkit2gtk needed, WebView2 target):
```bash
cd src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
  cargo xwin build --target x86_64-pc-windows-msvc
```

**Windows installer (NSIS)**:
```bash
XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
  pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
# output: src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/StageDeck Remote_<ver>_x64-setup.exe
```
Bump the version before building — see **Disciplines #2**.

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

**Presenter UI is vanilla JS, embedded at compile time.** `src/presenter/{presenter.html,presenter.css,presenter.js}` are served by Axum via `include_str!` in `routes.rs` → **editing them requires recompiling the Rust crate**; they are NOT part of the Vite build and NOT typechecked. Layout is phone-first one-handed: a bottom transport dock (giant full-width "下一页") pinned across both modes, 2-col macro buttons, and an ARM strip; client-side gating mirrors the backend (`canAct()` ⇔ `click_allowed`).

**Director GUI** (`src/`, Vue 3 + Pinia + Vue Router): `views/DirectorView.vue` composes `components/*`; `utils/api.ts` wraps IPC; `stores/` holds state; it polls `get_status` + `get_logs` every 1.5s. The module map in `docs/architecture.md §7` is still accurate for *what each file does*.

## Platform pattern & gotchas

- Native modules (`capture`, `capture_session`, `input`, `window_manager`, `ppt_com`) are `#[cfg(windows)]` real impl + `#[cfg(not(windows))]` stub. Keep this split when adding native code so the service layer keeps compiling on Linux.
- **COM VARIANT gotcha** (regressed twice): PowerPoint returns `VT_DISPATCH (9)`; always extract objects with `IDispatch::try_from(&VARIANT)`, never `IUnknown::try_from`.
- COM behavior cannot be unit-tested on Linux — verify it via **cross-compile (0 errors) + runtime on Windows**. Pure functions (`parse_macro_names`, `ppt_window_rank`, lock predicates, normalized→screen coord mapping) ARE Linux-unit-testable; prefer extracting logic into such functions.
- The process sets Per-Monitor-DPI-Aware v2 (`lib.rs`) so window rects, screen coords, and `SendInput` all share physical pixels.

## Workflow conventions (the user enforces these)

- **No one-shot generation.** Every change goes brainstorm → spec → plan → implement → review, in small auditable commits. Specs live in `docs/superpowers/specs/YYYY-MM-DD-*.md`, plans in `docs/superpowers/plans/`. Use the `superpowers` skills (brainstorming → writing-plans → subagent-driven-development) and the dated specs/plans as the source of truth for recent behavior.
- Per-feature branch off `master`, then local merge (no PR unless asked).

## Doc staleness

`README.md` and `docs/architecture.md` were written at the MVP-scaffold stage and are **partly outdated**: capture/input/window-enumeration, the PowerPoint COM toolchain, and the offline QR code are now **implemented** (no longer "TODO(windows)/占位"); and the lock model changed — `control_locked` now defaults **false** (emergency-only), and **all** presenter actions require `click_allowed` (architecture.md §5/§4.3 still say only preview-clicks need unlock and that force-lock defaults on). When in doubt, trust the code and the newest `docs/superpowers/specs/*`.
