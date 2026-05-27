# PDF-Parser

Windows-first Tauri 2 + React desktop foundation for a local-first PDF processing app.

## Phase 0

- React 19, Vite 6, TypeScript 5, Tailwind CSS v4, shadcn/ui, Geist fonts
- Dark Linear-style app shell with Inbox, Library, Search, Chat, and Settings routes
- Ctrl+K command palette skeleton
- SQLite app database at `%APPDATA%\PDF-Parser\db\app.db`
- FTS5 migration, sqlite-vec registration, and startup PRAGMAs

## Development

```powershell
pnpm install
pnpm tauri dev
```

## Verification

```powershell
pnpm build
cargo build --release --manifest-path src-tauri\Cargo.toml
pnpm tauri build --debug
```
