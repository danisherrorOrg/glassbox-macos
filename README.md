# Process Network Inspector

A native macOS app (Tauri + Rust core, React frontend) for inspecting what a
running process can see and say over the network — process → connection →
domain → request/response, correlated in one view.

Start with `docs/[0] READING_ORDER.md` for the full design (data model,
architecture, permissions, testing strategy) and `docs/[9] TODO.md` for the
phase-by-phase build roadmap this project follows.

## Development

```
npm install
npm run tauri dev
```

Requires Rust (`cargo`/`rustc`) and Node.js. `npm run tauri dev` launches the
app with hot-reload on both the Rust core and the React frontend.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
