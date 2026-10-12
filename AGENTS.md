# Repository Guidelines

This file guides AI agents and contributors working in this repository. Follow the local architecture and existing patterns before introducing new abstractions.

## Project Overview

`lovelycarve` is a Tauri v2 desktop forensic disk-image tool. The backend is Rust with async tasks, memory-mapped raw image access, pure Rust EWF/VMDK/VHD/VHDX/BitLocker/ReFS readers. The frontend is Vue 3 (Composition API), TypeScript, and Vite. The app targets Windows, macOS, and Linux. UI copy is Chinese-first and written inline (there is no i18n layer).

Core workflow: open or create a case, load evidence, detect partitions, browse filesystem data, preview native file types, hash / export / carve, build volume snapshots, run global search and timeline projections, and run LPP plugins. Read `docs/ARCHITECTURE.md` before adding backend modules. Read `docs/FRONTEND.md` (the frontend spec: principles, tokens and contrast, type scale, spacing, primitives, states and feedback, tables, dialogs and layering, motion, copy, accessibility, review checklist) before changing UI; the design system itself is the CSS tokens in `src/styles/themes.css` plus the shared `ui-*` primitives in `src/styles/ui.css` and the motion layer in `src/styles/motion.css` — read them too.

## Project Structure

- `src/`: Vue 3 frontend.
- `src/app/`: bootstrap, main-shell components, workspace navigation/tabs, and cross-feature workflow wiring.
- `src/features/`: twelve business domains, each colocating components, composables, pure models, and tests. Feature collaboration uses small `api.ts` files, explicit component entries, or pure contracts; features must not import `app/`.
- `src/components/`: shared `ui/`, `feedback/`, `layout/`, and `display/` components; no business state dependencies.
- `src/composables/`: generic reactive capabilities such as theme, motion, persistence, notifications, and confirmation; business state lives in its feature's `composables/` (no global store).
- `src/lib/models/`: shared pure contracts such as file-location targets; transport types remain under IPC.
- `src/views/`: secondary app windows (the detached `FilePreviewWindow.vue`). The main shell is `src/AuthorizedApp.vue`.
- `src/lib/ipc/`: the typed IPC service layer — `client.ts` (sole `invoke` wrapper), `events.ts` (`onEvent`/`emitEvent`), and per-domain command modules. Components/composables call this layer, not Tauri `invoke`/`listen` directly.
- `src/styles/themes.css`: all theme tokens for `light`, `dark`, and `pink`.
- `src/styles/ui.css`: shared UI primitives (buttons, fields, tabs, badges, cards, modal shell, menus, empty states, panel headers), loaded globally through `global.css`.
- `src/styles/motion.css`: the motion layer (shared keyframes, primitive entrances, the `ui-stagger` utility, the theme reveal), loaded globally after `ui.css`.
- `src-tauri/src/`: Rust backend.
- `src-tauri/src/commands/`: per-domain `#[tauri::command]` handlers.
- `src-tauri/src/app/`: startup, shutdown, shared application access/error types, and cross-domain workflows.
- `src-tauri/src/core/`: forensic engines, parsers, indexing, filesystems, search, timeline, carving, and plugin runtime.
- `src-tauri/src/state.rs`: shared application state.
- `src-tauri/vendor/`: vendored filesystem and parser crates.
- `src-tauri/examples/`: standalone probes for partition and filesystem work.
- `plugins/`: Git submodule pointing to `LovelyTeams/LovelyCarve-Plugins`, with a separate Cargo workspace — `sdk` (crate `lovelycarve-plugin`: C ABI, LPP wire types, Rust plugin API; also a host dependency), `host` (`lc-plugin-host`, the process-isolation loader), and the single bundled analyzer (`linuxinfo`), a `cdylib` next to its `plugin.toml`. Initialize with `just bootstrap`; commit and push plugin changes before updating the main repository's submodule pointer.
- `docs/`: architecture, development, protocols, and verification guides.

## Build, Test, and Development Commands

Use the repository-root `Justfile` as the task entry point. Do not replace an existing recipe with a direct `bun`, `cargo`, or Tauri command. `bun install --frozen-lockfile` is the bootstrap exception because dependency installation has no recipe. Run `just --list` when the available tasks may have changed.

- `just dev-fe`: run the Vite frontend, including the file-viewer asset preparation step.
- `just dev`: run the Tauri desktop app without QR/OCR using the configured Visual Studio and OpenSSL environment; `just dev-full` also enables QR/OCR and configures OpenCV.
- `just typecheck`: run `vue-tsc --noEmit`.
- `just build-fe`: type-check and build the frontend production bundle.
- `just test-fe`: run the complete frontend Vitest suite once.
- `just ipc-check`: enforce the typed frontend IPC boundary.
- `just frontend-structure-check`: enforce feature/shared/model dependency boundaries, valid import/style paths, and absence of runtime dependency cycles.
- `just fmt`: format the main Rust crate, the plugin workspace and the maintained vendored reader crates.
- `just check`: compile-check the Rust backend without the optional OpenCV QR feature.
- `just check-examples`: compile-check every explicitly declared example without the optional OpenCV QR feature.
- `just clippy`: lint the Rust backend without the optional OpenCV QR feature.
- `just test`: run the Rust backend tests without the optional OpenCV QR feature.
- `just test <name>`: run a targeted Rust test by substring.
- `just test-ewf`: test the vendored pure Rust EWF reader and sample corpus.
- `just test-bitlocker`: test the vendored pure Rust BitLocker decryptor; set `BDE_ORACLE_IMAGE` to run the full external dfvfs oracle.
- `just test-vhd` / `just test-vhdx`: test the vendored Rust VHD/VHDX readers and committed image fixtures.
- `just test-vmdk`: test the vendored pure Rust VMDK reader and committed image fixtures.
- `just test-qcow2` / `just test-vdi`: test the standalone vendored image readers.
- `just build-plugins [--release]`: build the plugin workspace (`lc-plugin-host` and the plugin libraries) and copy each library next to its `plugin.toml`; `just dev`, `just build-release` and `just pack-cli` run it first.
- `just test-plugins` / `just clippy-plugins`: test / lint the plugin workspace.
- `just build` / `just build-full`: build the backend without / with QR/OCR. Cargo default features are empty.
- `just build-release` / `just build-release-full`: create the release desktop bundle without / with QR/OCR; the full entry merges `src-tauri/tauri.full.conf.json` for OpenCV and models.
- `just run-example probe_partitions <image-path>`: run the standalone partition probe.
- `just run-cli [args]`: run the window-free `lovelycarve-cli` shell / complete command surface (development build without QR/OCR). See `docs/CLI.md`.
- `just build-cli`: build the QR/OCR-free release CLI; `just build-cli-full` enables QR/OCR. `just pack-cli` / `just pack-cli-full` package the matching Windows CLI.
- `just test-cli`: build and smoke-test CLI navigation, JSONL sessions, encrypted-volume unlock/export, carving and clean shutdown.
- `just test-luks` / `just test-veracrypt`: test the standalone vendored encrypted-container readers.
- `just pack` / `just repack`: create or recreate the portable Windows archive without QR/OCR; use `just pack-full` / `just repack-full` for a full-featured build.
- `cd src-tauri; cargo sort -w`: sort Rust manifests after dependency edits; this is allowed directly because no `just` recipe currently wraps it.
- `just bench-filedb-store`: synthetic filedb metadata-index benchmark (`release-fast` profile).

The shipping `release` profile is size-first (`opt-level = "s"`, fat LTO). Byte-crunching crates (hashers, zstd / xz / bzip2 / zlib, SQLCipher, memchr / aho-corasick / regex-automata) get `opt-level = 3` through `[profile.release.package.*]` overrides in `src-tauri/Cargo.toml`. Add a new hot-path dependency there, and compare against `--profile release-fast` when tuning.

Rust recipes can hold Cargo's global lock for a long time. Be patient with `just build`, `just check`, `just test`, and `just clippy`; do not kill them merely because they appear idle.

Local `just` recipes are the verification source of truth. This repository has no GitHub Actions workflows; contributors and maintainers run checks and release builds locally.

ReFS uses the vendored pure Rust `refs-reader`; production builds do not load or ship `librefs-13.dll`. The explicit `refs-dll-oracle` feature and `just probe-refs` / `just test-refs-oracle` recipes retain the legacy DLL for comparison only. Compression does not require separate bzip2 or zlib runtime DLLs.

## Backend Architecture

`src-tauri/src/lib.rs` owns the Tauri builder, plugins, shared state registration, and the `invoke_handler!` list. Every new command must be registered there.

`src-tauri/src/commands.rs` is the command facade and transport helper location. Command implementations live under the ten domain modules in `src-tauri/src/commands/`. Cross-domain orchestration lives in `src-tauri/src/app/workflows/`; application workflows must not depend on commands. Startup and shared shutdown helpers live in `app/setup.rs` and `app/lifecycle.rs`.

`src-tauri/src/state.rs` owns long-lived runtime state; auxiliary types live in `state/`. Commands should receive state through `tauri::State` and avoid hidden globals. Immediate tasks live in `task_manager`; persistent plugin tasks remain in `core/extensions/task_scheduler`.

`AppState.images` stores `Arc<core::filesystem::evidence_session::EvidenceSession>`. A session owns lazy Docker and deleted-NTFS projections and external volume handles; `core::evidence::image::DiskImage` owns source bytes, read caches and decryptors. Byte-only operations borrow `session.disk_image()`. Native filesystem readers accept the byte reader; filesystem traversal accepts the session. External providers implement `filesystem::external_volume::ExternalVolume`; its registry must be drained before asynchronous close. Keep concrete plugin handles out of the byte reader.

Plugin execution, analysis publication, timeline projection and result cleanup are wired through `app/workflows/scheduler.rs`. The persistent scheduler calls the existing `TaskRunner` contract; it does not open analysis or timeline stores. Validated JSON/IPC analysis models live under `core/analysis/results/models.rs`; LPP and ABI definitions remain in the plugin SDK.

`src-tauri/src/core/` contains twelve domain engines: evidence, filesystem, catalog, casework, analysis, carving, search, timeline, preview, extensions, platform, and common. Place new code in its owning domain; core must not depend on app, commands, state, cli, or Tauri. Keep each file focused on one concern. Add new parsers, filesystems, or scanners as explicit modules or trait implementations instead of growing broad switch statements. The current `mod.rs` files are existing exceptions; do not create new `mod.rs` paths for new modules.

Long tasks should return a task identifier or useful result and propagate errors to the UI. Progress or completion events must be intentional and documented by the command that emits them.

### Backend invariants

These are easy to break silently; check them when touching the related code.

- **Cancellation.** Long work registers through `state.tasks` (`TaskManager`) and receives a `tokio_util::sync::CancellationToken`. Pass `&CancellationToken` straight into blocking code and poll `is_cancelled()` — it is `Sync` and needs no runtime. Do not add a polling thread that mirrors the token into an `AtomicBool`. (`bde_vmk` / BitLocker unlock still take `&AtomicBool` on purpose to stay tokio-agnostic.)
- **Whole-image readers.** Anything that streams the full image (hash, RAW export, decrypted export) must fail when it ends with `offset != total`. A short read must never produce a "complete" digest or image.
- **Evidence path identity.** Compare evidence paths only through `core::casework::case::evidence_path_key` (case-insensitive, separator-normalised on Windows). This covers the `image_load` single-flight lock, runtime-id reuse, and case evidence lookup.
- **Image unload.** `app::workflows::evidence::remove_image_runtime` is best-effort: it keeps cleaning after a failure and returns the first error. Every per-image cache in `AppState` (preview sessions, `flat_walk_cache`, media temp, session snapshots, plugin registries, Dokan mounts) must be purged there. Add new per-image state to that function in the same change.
- **Ranged file reads.** `fs_dispatch::Volume::read_range` is the seekable read path. NTFS and ext4 implement it (ext4 maps the range through the same extent piece plan as its stream, so both paths return identical bytes), and `FsSession::read_range_direct` also serves contiguous image spans (`unalloc:` / `diskgap:` / `sysfile:` / `deleted:run:`). Other filesystems return `Ok(None)` and `FsSession::read_file_range` / `FsSession::stream_file_range` fall back to streaming, which costs O(offset). Implement `read_range` when adding or touching a filesystem whose extent / run / cluster map allows seeking.
- **Host file names.** Names from evidence written to the host go through `fs_tree_export::sanitize_export_component` (illegal characters, trailing dots / spaces, Win32 device names such as `aux.c` / `CON`). Create export files with `create_new`, never truncate an existing host file.
- **Partial outputs.** Snapshot builders declare `volume_snapshot::PartialOutput` before creating the output file and call `keep()` only after the encoder finishes, so errors and cancellation never leave a truncated `.dirz`.
- **Large reads bypass the block cache.** `DiskImage::read_into` requests of at least 1 MiB skip the 64 KiB located cache for FFI-backed images, so bulk scans do not evict hot metadata blocks. Keep small, random metadata reads below that size.
- **Plugins.** Every discovered plugin is trusted and dispatchable; there is no approval or trust store. The directory fingerprint is display-only and computed lazily (`PluginDefinition::fingerprint`, blocking). Never hash plugin directories during startup. Plugins are libraries behind the C ABI in `plugins/sdk/src/abi.rs`; both isolation modes (`lc-plugin-host` over LPP frames, or in-process) go through `PluginConnection` and must carry the same methods and limits. A loaded library is never unloaded, and an in-process instance is destroyed only after its last call returns.

## IPC Contract

A feature crossing the frontend/backend boundary normally requires all of these in the same change:

1. Rust core logic in `src-tauri/src/core/`.
2. A Tauri command wrapper under `src-tauri/src/commands/`.
3. Registration in `src-tauri/src/lib.rs`.
4. A typed command function and matching types in the relevant `src/lib/ipc/<domain>.ts`.
5. A composable or component that consumes the wrapper.

Skipping one link often fails only at runtime. After IPC changes, run `just ipc-check`, `just typecheck`, and the relevant frontend/backend build or test recipes.

## Rust Coding Guidelines

- Prioritize correctness and clarity over speed unless performance is the task.
- Prefer existing files unless the change is a new logical component.
- Avoid `unwrap()` and `expect()` in production code; propagate with `?` or handle explicitly.
- Be careful with indexing and slicing that can panic.
- Never discard fallible results with `let _ =`. Propagate, log visibly, or handle with `match`.
- Ensure async failures reach the UI layer with meaningful context.
- Inline variables in format strings: use `format!("hello {name}")`. `clippy::uninlined_format_args` is enabled in `[lints.clippy]` in `src-tauri/Cargo.toml`. Existing violations are warnings; `cargo clippy --fix` rewrites them.
- Collapse nested `if` statements when practical.
- Prefer method references over redundant closures.
- Avoid ambiguous bool or `Option` parameters at call sites; prefer enums, named methods, or newtypes.
- Prefer exhaustive `match` arms over wildcard `_` arms.
- Newly added traits need doc comments explaining their role and implementer expectations.
- Use full words for variable names; avoid abbreviations such as `q` for `queue`.
- Do not create single-use helper functions unless they clarify nontrivial logic.
- Use variable shadowing to scope clones for async moves and minimize borrowed lifetimes.

## Module Size and Structure

Target Rust modules under roughly 500 lines excluding tests. If a file approaches 800 lines, add new functionality in a sibling module unless there is a strong reason not to. When extracting code, move related tests and type/module docs with the implementation. Avoid turning `core` into a catch-all; consider whether an existing crate or a new crate boundary is more appropriate.

## Frontend Guidelines

Vue 3 single-file components with `<script setup lang="ts">` and the Composition API. The design system is the CSS tokens in `src/styles/themes.css`, the primitives in `src/styles/ui.css` and the motion layer in `src/styles/motion.css` — read them before editing UI. The rules below summarize `docs/FRONTEND.md` (Chinese, authoritative for frontend conventions); change both together when a rule changes.

- Component state is `ref` / `computed` / `watch`. Business state lives in `features/<domain>/composables/useXxx.ts`; generic capabilities live in root `composables/`. Do not add Vuex, Pinia, or any global store without explicit direction.
- Keep pure models free of reactive state, host globals, and IPC calls. Feature internals must not import their own API barrel; API barrels must not eagerly export heavy viewer components. Cross-feature dependencies are declared in `scripts/lib/frontend-structure.mjs`; run `just frontend-structure-check` after structural changes.
- Main-window wiring owns workspace singleton disposers; instance scopes own subscriptions, timers, media leases, and Blob URLs. Deactivation releases visible-panel keyboard/menu resources without cancelling backend work. Guard asynchronous publication by context generation, immediately release late handles, and flush tag/derived writes before switching cases. See `docs/FRONTEND_STRUCTURE.md` for the ownership matrix.
- Tauri access goes through `src/lib/ipc/`; never call `invoke`/`listen` inline in a component or composable.
- Every UI must work in `light`, `dark`, and `pink`. Consume colors, radii, shadows, and transitions through existing `var(--*)` tokens; each token is defined in all three theme blocks (`:root`, `[data-theme='dark']`, `[data-theme='pink']`), so a new token must be added to all three.
- Do not use hex colors, literal `white`/`black`, or hard-coded `rgba(...)` outside `themes.css`, except translucent overlays where no token fits. Component CSS is `<style scoped>`.
- Use the tokens that exist; do not invent names. Notably: surfaces are `--bg-primary`/`--bg-secondary`/`--surface-primary`/`--surface-secondary`/`--surface-tertiary`; text is `--text-primary`/`--text-secondary` (no `--muted`); lines are `--border-color` (no `--border`); radii `--radius-sm|md|lg`; shadows `--shadow-sm`/`--card-shadow`/`--card-shadow-hover`/`--modal-shadow`; severity chips `--sev-{critical,warn,success,info}-*`.
- Fonts are local only. The app CSP (`src-tauri/tauri.conf.json`) allows no remote style or font hosts, so bundle font files under `src/assets/fonts/` instead of linking a font CDN. `--font-ui` (Inter) is the default body/UI font; `--font-mono` (Cascadia Mono) for offsets, hashes, sectors, IDs, recovery keys, and pill/status labels; `--font-brand` (Quicksand) is brand/wordmark only. Only Quicksand is bundled: `--font-ui` and `--font-mono` are system stacks that fall back to the platform UI / CJK and monospace fonts. There is no serif font and no `--font-sans`.
- Build controls from the `ui-*` primitives in `src/styles/ui.css` instead of re-declaring button, input, badge, or modal chrome in scoped CSS: `ui-btn` (`primary` / `danger` / `ghost` / `link` / `sm` / `lg`), `ui-icon-btn`, `ui-input` / `ui-select` / `ui-textarea` / `ui-search`, `ui-check` / `ui-switch`, `ui-seg`, `ui-tabs`, `ui-badge`, `ui-card`, `ui-alert`, `ui-progress`, `ui-empty`, `ui-toolbar` / `ui-toolbar-actions`, `ui-footbar` / `ui-footbar-stat`, `ui-menu`, `ui-choice`, `ui-disclosure`. Keep scoped CSS for layout and genuinely component-specific visuals. Each primitive is a single-class selector with its modifiers and states inside `:where()`, so any scoped rule overrides it without `!important`; keep that property when adding one. Card = `--surface-secondary` + 1px `--border-color` + `var(--radius-lg)` + `var(--card-shadow)`.
- Dropdowns use the `UiSelect` component (`src/components/ui/UiSelect.vue`: `v-model` plus `options: { value, label, disabled? }[]`), never a native `<select>`, whose system popup ignores the theme. The trigger keeps the `ui-select` look and the width of the longest option; the list is a `ui-menu` in the top layer, teleported to `body`, so floating layers that close on an outside pointer or scroll must let events inside `.ui-select-list` through.
- No coloured stripe on the left edge of cards, list rows, table rows, toasts or panels (`border-left`, a `::before` bar or an `inset Npx 0 0` shadow alike). Show status with a `ui-badge`, an icon or a status banner, categories with a legend swatch, and selection with the tint plus primary text; selected table rows get the 1px frame from `data-table.css` (70% `--primary-color`, 3:1 on the canvas in every theme), and snapshot rows keep their tint when selected. Deleted rows use ordinary row backgrounds, hover and selection feedback, with red text, a struck-through name and a deletion badge. Neutral 1px dividers, tree guides and the Markdown blockquote rule are fine.
- Modals are `.ui-modal-overlay` > `.ui-modal` with `ui-modal-header` / `ui-modal-body` / `ui-modal-footer`, normally inside `<Transition name="ui-modal">`. The structured file viewers (PE / ELF / Mach-O / ar / GGUF) share `src/styles/report-viewer.css` and the SQLite / Realm browsers share `src/styles/db-browser.css`, both included through `<style scoped src>` like `data-table.css`.
- Filled controls (primary / danger buttons, switches) use `--primary-solid` / `--danger-solid` and their `-hover` tokens with `--accent-contrast` labels; `--primary-color` stays for text, icons, borders, focus rings, and tints. Keep those labels at 4.5:1 contrast or better in every theme. `--primary-color` and `--text-secondary` are text colors too: keep them at 4.5:1 on the surfaces, `--sidebar-bg` and `--sidebar-active-bg` in every theme (the softer brand tones live in gradients, glows and tints). Status bar labels use `--statusbar-fg`.
- Validation: mark the failing `ui-input` / `ui-select` / `ui-textarea` (or the input inside `ui-search`) with `aria-invalid="true"` and state the problem in a `ui-field-error` linked through `aria-describedby`; a toast alone is not field feedback. Font sizes are whole pixels; primitive spacing follows the 4px grid.
- Do not add new top-level chrome outside `src/AuthorizedApp.vue` (the main shell).
- Workspace pages carry no title bar: they open on a `ui-toolbar` (search and filters on the left, page actions in `ui-toolbar-actions` on the right), then the work surface, then a `ui-footbar` that holds the page context (current task, state) and counts. The sidebar already names the page.
- `lucide-vue-next` is the icon system (`import { Icon } from 'lucide-vue-next'`, `<Icon :size="16" />`). Do not add other icon libraries or emoji in the UI.
- The spin indicator is lucide `Loader2` with the `.spin` class (exempt from the reduced-motion freeze in `base.css`; with motion off it turns at half speed); `AsyncPanelLoading.vue` covers lazy panels. Prefer skeletons or segmented progress for long tasks.
- Motion is switchable (设置 → 界面 → 界面动画: 跟随系统 / 开启 / 关闭). `useUiMotion.ts` resolves the preference and the OS reduced-motion setting into `data-motion="full" | "reduced"` on `<html>` before mount; that attribute is the only switch, so never query `prefers-reduced-motion` in CSS (it would defeat an explicit 开启). With `reduced`, `base.css` settles every transition and animation instantly (spinners excepted); give an indeterminate bar a static `:root[data-motion='reduced']` fallback so it does not freeze mid-slide.
- Timing comes from tokens: `--transition-fast` (0.15s ease) / `--transition-normal` (0.25s ease) for plain state changes, and `--motion-fast|normal|slow` (150 / 250 / 400ms) with `--ease-out` (entrances), `--ease-in` (exits) and `--ease-spring` (small pops) for motion. Entrances take the longer duration, exits the shorter.
- Decorative motion lives under `:root[data-motion='full']` (use `:where(:root[data-motion='full'])` in the global sheets to keep single-class specificity) and reuses the keyframes in `motion.css` (`ui-rise-in`, `ui-pop-in`, `ui-zoom-in`, `ui-bounce-in`, `ui-grow-x|y`, `ui-bump`, idle `ui-float` / `ui-twinkle` / `ui-breathe`). They animate the individual `translate` / `scale` / `rotate` properties, declare only the start state and run with `backwards` fill, so they compose with a component's own `transform`. `ui-menu`, `ui-alert` and `ui-empty` animate in on their own; add `ui-stagger` to short, non-virtualized lists and card grids (never virtual lists: recycled or keyed-moved rows restart the entrance) and `ui-bump` plus a value `:key` to counters. Idle loops are for illustrations and drop targets only.
- JS-driven motion goes through `animateUi` / `motionTiming` (`useUiMotion.ts`), `UiTransition` (`collapse` / `horizontal` / `fade` / `pop`), `v-content-motion` (light fade; `.rise` for page and tab switches) and `v-disclosure-motion`; each settles immediately with motion off. The theme switch passes the clicked control to `setTheme(theme, from)` for the circular view-transition reveal.
- UI copy is Chinese-first and written inline; there is no i18n layer.

After frontend changes, always run `just build-fe` (or at least `just typecheck` while iterating). Use `just test-fe` whenever behavior or state logic changes.

## Testing Guidelines

Rust tests are colocated in `#[cfg(test)]` modules. Use focused tests for parser behavior, filesystem traversal, indexing, search, timeline, and command-level changes. Prefer whole-object `assert_eq!` comparisons instead of field-by-field assertions. Use `pretty_assertions::assert_eq` in tests when available. Avoid mutating process environment in tests; pass dependencies or flags from the caller instead.

The frontend uses Vitest with colocated `*.test.ts` files (e.g. `src/composables/*.test.ts`, `src/utils/*.test.ts`). Run it through `just test-fe`; do not invoke Vitest directly. Also run `just typecheck` and `just build-fe` for frontend work.

Use this minimum local verification matrix:

- Frontend-only changes: `just typecheck`, `just test-fe`, and `just build-fe`; structural changes also need `just frontend-structure-check` and `just ipc-check`.
- Rust-only changes: `just fmt`, `just check`, `just clippy`, and `just test`.
- IPC changes: all relevant frontend and Rust checks above, plus `just ipc-check`.
- Plugin workspace changes (`plugins/`): `just fmt`, `just clippy-plugins`, `just test-plugins`; SDK changes also need the Rust checks above, since the host depends on the SDK.
- Vendored QCOW2/VDI changes: `just fmt` plus `just test-qcow2` and/or `just test-vdi`.

## Logging and Output

Keep CLI/user output separate from diagnostics. Use `println!` or `eprintln!` only for requested data, copyable/actionable content, hard top-level errors, or interactive prompts. Everything else belongs in `tracing`.

Default behavior should be quiet on success. The desktop app's default filter is `warn`; set `RUST_LOG` (for example `RUST_LOG=lovelycarve_lib=info`) to see more. Lifecycle messages are `tracing::info!`, recoverable fallbacks are `tracing::warn!`, and module-level detail is `tracing::debug!`. Do not print success confirmations when the exit code already communicates success.

## Changelog and Documentation

Update `CHANGELOG.md` under `[Unreleased]` for meaningful user-visible features, fixes, breaking changes, deprecations, or removals. Follow Keep a Changelog 1.1.0 and keep entries concise.

Update `docs/` when changing user-facing behavior, configuration, architecture, commands, or workflow assumptions.

## Commit and Pull Request Guidelines

Recent history uses Conventional Commits with optional scopes, for example `feat(index): ...`, `fix(fstree): ...`, `perf(index): ...`, and `chore: ...`. Keep subjects imperative and specific.

Pull requests should describe user-visible behavior, list the local `just` recipes used for verification, link related issues, and include screenshots or recordings for UI changes. Claim hosted CI coverage only when the relevant workflow run has succeeded.
