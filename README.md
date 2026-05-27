# PDF-Parser

PDF-Parser is a Windows-first, local-first desktop app for turning folders of scanned and downloaded PDFs into a searchable, AI-named document library. It is built with Tauri 2, React 19, Tailwind v4, shadcn/ui, and a bundled SQLite database with FTS5 + sqlite-vec.

![PDF-Parser screenshot placeholder](docs/screenshot.png)

## Phase 0 foundation

- React 19, Vite 6, TypeScript 5, Tailwind CSS v4, shadcn/ui, Geist fonts
- Dark Linear-style app shell with Inbox, Library, Search, Chat, and Settings routes
- Ctrl+K command palette skeleton
- SQLite app database at `%APPDATA%\PDF-Parser\db\app.db`
- FTS5 migration, sqlite-vec registration, and startup PRAGMAs

## Building from source

### Prerequisites

- Windows 10/11
- Node.js 20+
- pnpm 9+
- Rust stable with the `x86_64-pc-windows-msvc` target
- Tauri prerequisites for Windows, including Microsoft C++ Build Tools and WebView2

### Development

```powershell
pnpm install
pnpm tauri dev
```

### Production bundle

```powershell
pnpm install
pnpm tauri build
```

For a faster local verification build:

```powershell
pnpm tauri build --debug
```

## Releasing

The release pipeline is tag-driven. Before the first release, replace the `<OWNER>` placeholder in `src-tauri\tauri.conf.json` under `plugins.updater.endpoints` with the GitHub user or organization that owns the `PDF-Parser` repository.

1. Bump the app version in `src-tauri\tauri.conf.json` (`version`). Tauri reads this value when `pnpm tauri build` creates installers and updater metadata.
2. Confirm the Ed25519 updater public key is embedded in `src-tauri\tauri.conf.json` at `plugins.updater.pubkey`.
3. Copy the private updater key contents from `C:\Users\jacob\.copilot\session-state\0a7757fe-40fc-4764-892e-0085b8c8387d\files\tauri_updater.key` into a GitHub Actions secret named `TAURI_SIGNING_PRIVATE_KEY`. Never commit this key.
4. If a password-protected key is generated in the future, add `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The auto-generated v1 key has no password, so leave this secret empty or unset.
5. Tag and push the release:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

The `Release` GitHub Actions workflow builds on `windows-latest`, produces MSI and NSIS installers, signs updater artifacts, generates `latest.json`, attaches SHA256 checksums, and publishes the GitHub Release with release notes.

## First install on Windows

The v1 installer is intentionally unsigned, so Windows SmartScreen may show an "Unknown publisher" warning. Click **More info → Run anyway** only if you downloaded the installer from this repository's GitHub Release.

To verify an installer manually:

```powershell
Get-FileHash -Algorithm SHA256 .\PDF-Parser_0.1.0_x64_en-US.msi
```

Compare the hash with the `checksums.txt` file attached to the same GitHub Release.

## Auto-updates

PDF-Parser uses the Tauri updater plugin. Each release publishes an Ed25519-signed `latest.json` feed to GitHub Releases, and the app checks that feed on launch and every 6 hours while running. When an update is available, the app shows release notes and an **Install and restart** action. The updater verifies the signature with the public key embedded in `tauri.conf.json` before installing.

## Verification

```powershell
pnpm build
cargo build --release --manifest-path src-tauri\Cargo.toml
pnpm tauri build --debug
```
