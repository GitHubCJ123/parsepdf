# PDF-Parser — TODO

Actionable backlog for the next round of work. Items are grounded in the current
codebase (Tauri 2 + React frontend in `src/`, Rust backend in `src-tauri/`).

Legend: `[ ]` not started · `[~]` in progress · `[x]` done
Each task lists **Why**, the **Files** to touch, concrete **Steps**, and
**Acceptance** criteria.

---

## 1. Explain the difference between the OCR models/engines in the app

**Why:** The OCR engine picker lets users choose between **Tesseract** (bundled,
default) and **RapidOCR** (opt-in PP-OCRv5 ONNX), but gives almost no guidance on
when to pick which. Settings → OCR shows only `{name} · {status}` in a dropdown
plus a single helper line ("Install RapidOCR here when higher-accuracy OCR is
needed"), and each engine card has a one-line blurb. Users can't see the
trade-offs (speed vs. accuracy, language/script coverage, handwriting and
low-quality scans, download size, offline behavior).

**Files:**
- `src/components/engine-selector.tsx` — OCR engine cards: intro copy (~line 135)
  and per-card description/size/estimate (`EngineCard`, ~line 191).
- `src/app/settings/page.tsx` — Settings → OCR section (`activeSection === "ocr"`,
  ~line 239): engine `<select>` (~line 244) and its helper text (~line 259).
- `src-tauri/src/commands/engines.rs` — backend `EngineInfo.description` strings
  (Tesseract ~line 60, RapidOCR card); the single source of truth for the blurbs.
- `MODELS.md` — currently the RapidOCR model manifest; add a short
  "Which OCR engine should I use?" comparison.

**Steps:**
- [ ] Write clear, accurate descriptions for each engine covering: typical speed,
      accuracy, language/script coverage, handwriting/low-quality scans, download
      size, and offline behavior. (Tesseract = fast, lightweight, English-first;
      RapidOCR = higher accuracy, multilingual, larger download.)
- [ ] Surface a side-by-side comparison in the OCR settings (small table or two
      labeled cards) instead of the single helper sentence.
- [ ] Add a "When to choose this" hint on each `EngineCard` and/or a tooltip /
      info popover next to the engine `<select>`.
- [ ] Keep the copy in one place — prefer enriching the backend `description`
      (and an optional `best_for`/`strengths` field on `EngineInfo`) so the UI
      and `MODELS.md` stay consistent.
- [ ] Document the comparison in `MODELS.md`.

**Acceptance:**
- [ ] In Settings → OCR, a user can clearly see how Tesseract and RapidOCR differ
      and decide which to use without external research.
- [ ] Descriptions match actual behavior and stay in sync with
      `commands/engines.rs` (names, sizes, statuses); extra/unknown engines
      render gracefully.

---

## 2. Make watched folders the main feature

**Why:** Today the headline experience is the Upload/Inbox queue, and watched
folders are buried in Settings. The product should lead with "drop PDFs in a
folder and they get processed automatically." `/` currently redirects to
`/inbox` (`src/router.tsx`).

**Files:**
- `src/router.tsx` — `indexRoute` redirect target; route registration.
- `src/components/sidebar.tsx` — nav order and default emphasis.
- `src/app/inbox/page.tsx` — hero copy (header h1 ~line 345) should defer to
  Folders as the primary flow.
- `README.md` — "Features" list ordering (currently OCR-first) and "Quick start"
  (step 3 says drag into Inbox).
- Depends on **Task 4** (Folders becomes its own route/page).

**Steps:**
- [ ] After Task 4 lands, change the `/` redirect to `/folders` so the app opens
      on the watched-folders experience.
- [ ] Reorder sidebar so **Folders** sits at/near the top.
- [ ] Rewrite the Folders page hero to frame automatic intake as the core
      workflow; reframe Upload as the "one-off / manual" path.
- [ ] Update `README.md`: lead the Features list with the folder watcher and
      revise Quick start to "Add a watched folder" first, manual upload second.

**Acceptance:**
- [ ] Launching the app lands on Folders.
- [ ] README and in-app copy present watched folders as the primary feature.

---

## 3. Rename "Inbox" to "Upload"

**Why:** "Inbox" is ambiguous; the panel is really a manual upload + processing
queue (it already uses an `UploadCloud` icon and "Choose files").

**Files:**
- `src/components/sidebar.tsx` — `SidebarItem` `to` union + `sidebarItems` entry
  (`{ to: "/inbox", label: "Inbox", icon: Inbox }`).
- `src/router.tsx` — `inboxRoute` path `/inbox`, lazy import of
  `@/app/inbox/page` / `InboxPage`, and the `/` redirect (see Task 2).
- `src/app/inbox/` — rename folder to `src/app/upload/`; rename `InboxPage` →
  `UploadPage`; update header/empty-state copy that says "Inbox".
- Grep the repo for `"/inbox"`, `Inbox`, `InboxPage`, `InboxIssueBanner` and any
  `navigate({ to: "/inbox" })` calls (e.g. in `inbox/page.tsx`, settings).

**Steps:**
- [ ] Change the route path to `/upload` (keep a redirect from `/inbox` →
      `/upload` for any saved deep links / muscle memory).
- [ ] Rename the directory, component (`UploadPage`), and exported symbols.
- [ ] Update sidebar label to **Upload** and the `to` type union.
- [ ] Update all `navigate`/`Link` targets and copy strings referencing Inbox.
- [ ] Pick the icon (`UploadCloud`) consistently for the nav item.

**Acceptance:**
- [ ] Nav shows **Upload**; `/upload` works; `/inbox` redirects to it.
- [ ] No remaining user-facing "Inbox" strings; `tsc` and build pass.

---

## 4. Add a top-level "Folders" menu option for watched folders

**Why:** Watched folders are currently only reachable via Settings →
sub-section. They deserve a first-class nav entry (precondition for Task 2).

**Files:**
- `src/app/settings/page.tsx` — extract the existing `FoldersSection` (~line 645)
  and its handlers: `chooseWatchedFolder`, `toggleWatchedFolder`,
  `setFolderRecursive`, `scanWatchedFolder`, `removeWatchedFolder`,
  `refreshFolders`, plus the `folders`/`foldersMessage` state.
- (New) `src/app/folders/page.tsx` — `FoldersPage` hosting the moved UI.
- `src/router.tsx` — add `foldersRoute` (`/folders`).
- `src/components/sidebar.tsx` — add `{ to: "/folders", label: "Folders",
  icon: FolderOpen }`.
- `src/app/settings/page.tsx` — remove `"folders"` from the `sections` array
  (~line 181) and the `activeSection === "folders"` block (~line 330), or leave
  a short "Manage watched folders →" link that routes to `/folders`.

**Steps:**
- [ ] Create `src/app/folders/page.tsx` and move the watched-folders UI + IPC
      wiring (`watcherListFolders`, `watcherAddFolder`, `watcherSetEnabled`,
      `watcherScanNow`, `watcherRemoveFolder` from `@/lib/ipc`) into it.
- [ ] Register `/folders` in the router and add the sidebar item.
- [ ] Remove the Folders sub-section from Settings (or replace with a link).
- [ ] Verify add/remove/enable/recursive/scan-now all work from the new page.

**Acceptance:**
- [ ] **Folders** appears in the sidebar and opens a dedicated page.
- [ ] All watched-folder actions function identically to the old Settings panel.

---

## 5. Make the folder-check interval configurable

**Why:** The periodic rescan cadence is hardcoded:
`const PERIODIC_RESCAN_INTERVAL: Duration = Duration::from_secs(5 * 60);`
(`src-tauri/src/watcher/mod.rs:28`), consumed by `spawn_periodic_rescan`
(~line 332). Users on network shares / slow disks want to tune it.

**Files:**
- `src-tauri/src/watcher/mod.rs` — read interval from settings instead of the
  const; clamp to a safe range; have `spawn_periodic_rescan` re-read it (or
  restart) when it changes.
- `src-tauri/src/commands/folders.rs` — add a `watcher_set_rescan_interval`
  (and/or include the value in the folder config payload).
- `src-tauri/src/lib.rs` — register any new command in `invoke_handler`.
- `src/lib/ipc.ts` — typed wrapper for the new command/value.
- `src/app/folders/page.tsx` (from Task 4) — UI control for the interval.

**Steps:**
- [ ] Add a setting key, e.g. `watcher.rescan_interval_secs`, with a default of
      `300` and a sane minimum (e.g. ≥ 30s) to avoid hammering the disk.
- [ ] Load it in the watcher; clamp out-of-range values; log the effective
      interval. Apply changes without requiring an app restart.
- [ ] Surface a control on the Folders page (number input + unit, or a select:
      1m / 5m / 15m / 30m / 1h, plus "Scan now" which already exists).
- [ ] Persist via settings and confirm the running watcher picks up the change.

**Acceptance:**
- [ ] Changing the interval in the UI changes how often folders are rescanned,
      with no restart, and survives app relaunch.
- [ ] Values below the minimum are rejected/clamped with clear feedback.

---

## 6. Automatically install Ollama

**Why:** Ollama must currently be installed by hand — settings copy literally
tells the user to run `ollama pull llama3.1` in a terminal. We can detect,
download, and install it in-app, then pull a default model. There is a proven
in-app installer pattern to copy: `commands::engines::ocr_install_engine`
(`src-tauri/src/commands/engines.rs`) + `ocr/rapidocr_install.rs`
(`install_rapidocr` with a progress callback) emitting
`engine:install:progress` events.

**Files:**
- (New) `src-tauri/src/ai/ollama_install.rs` — detect existing install / running
  server; download the official Windows installer; run it; poll until the local
  server (`http://localhost:11434`) responds; pull `DEFAULT_OLLAMA_MODEL`.
- `src-tauri/src/commands/ai.rs` — new commands: `ollama_status`,
  `ollama_install`, `ollama_pull_model`, emitting progress events (mirror the
  engine-install event shape).
- `src-tauri/src/lib.rs` — register the new commands.
- `src/lib/ipc.ts` — typed wrappers + progress event listener.
- `src/app/settings/page.tsx` — AI providers section: replace the "run it
  yourself" copy with an **Install Ollama** button + progress UI (reuse the
  `EngineSelector` progress pattern), then auto-detect once installed.

**Steps:**
- [ ] Detection: probe `GET /api/tags`; if it fails, check whether the Ollama
      binary exists on PATH / default install dir.
- [ ] Download + verify the official installer (pin URL/version; verify size or
      checksum where possible — same rigor as RapidOCR's SHA256 checks).
- [ ] Run a silent/unattended install; surface download + install progress via
      events; handle "already installed" and "needs elevation" cases.
- [ ] After install, start/await the server, then `ollama pull` the default
      model with streamed progress.
- [ ] Wire the Settings UI: Install button → progress → "connected" state, and
      flip the provider to `ollama` automatically on success.

**Acceptance:**
- [ ] From a clean machine, a user can install Ollama + a default model entirely
      from Settings, with visible progress and clear error handling.
- [ ] If Ollama is already present/running, the UI reflects that and skips
      installation.

---

## 7. Add llama.cpp as an AI backend

**Why:** Some users prefer `llama.cpp` (`llama-server`) over Ollama, or already
run it. The AI layer is already abstracted behind the `AiProvider` trait
(`src-tauri/src/ai/mod.rs:82`) and selected in `configured_provider_with_model`
(~line 201), so adding a provider is well-scoped.

**Files:**
- (New) `src-tauri/src/ai/llamacpp.rs` — implement `AiProvider`
  (`propose_name`, `stream_chat`, `health_check`, `list_models`) against
  `llama-server`'s OpenAI-compatible endpoint (`/v1/chat/completions`,
  `/v1/models`). The existing `ollama.rs` and `openrouter.rs` are close
  templates (streaming + non-streaming).
- `src-tauri/src/ai/mod.rs` — `pub mod llamacpp;`; add `DEFAULT_LLAMACPP_BASE_URL`
  (e.g. `http://localhost:8080`) + default model; extend the provider `match`
  (~line 217) with a `"llamacpp"` arm reading `llamacpp.base_url` /
  `llamacpp.model`.
- `src-tauri/src/commands/ai.rs` — extend `ai_list_models` (~line 25) and health
  check to handle `"llamacpp"`.
- `src/app/settings/page.tsx` — `Provider` type (`"none" | "ollama"`) → add
  `"llamacpp"`; add a llama.cpp `ProviderCard` (base URL + model, Save / Test).
- `src/app/chat/page.tsx` — let chat target llama.cpp (provider id + model
  override, mirroring the `ollama:<model>` convention).
- `src/lib/ipc.ts` — any new typed params.

**Steps:**
- [ ] Implement the provider against `llama-server`'s OpenAI-compatible API,
      including SSE token streaming for chat and JSON naming proposals (reuse
      `proposal_from_model_response`).
- [ ] Register it in `configured_provider_with_model`, `ai_list_models`, and
      health checks.
- [ ] Add the Settings UI (provider option + config card + Test connection).
- [ ] Make it selectable in Chat alongside Ollama.
- [ ] (Optional, later) consider an auto-download of `llama-server` mirroring
      Task 6 — track separately if pursued.

**Acceptance:**
- [ ] With a running `llama-server`, naming and chat work end-to-end through the
      llama.cpp provider, including streaming and citations.
- [ ] Switching providers (none / Ollama / llama.cpp) persists and behaves
      correctly; unconfigured providers fail gracefully.

---

## Cross-cutting / done-when

- [ ] Update `README.md` (features, quick start, architecture diagram) to reflect
      Folders-first, Upload rename, Ollama auto-install, and llama.cpp support.
- [ ] Add/adjust Rust tests for the watcher interval setting and the new
      provider (`cargo test --workspace`).
- [ ] Frontend typecheck/build clean (`pnpm build`).
- [ ] Run `cargo clippy` and a Tauri build when UI/packaging changes land
      (`pnpm tauri build --debug` for a fast check), per CONTRIBUTING.

## Suggested order

4 (Folders route) → 3 (Upload rename) → 2 (Folders as main) → 5 (interval
config) → 1 (OCR engine explanations) → 6 (auto-install Ollama) → 7 (llama.cpp).
