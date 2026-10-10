# 🎭 Prismic Avatar Search - Tauri Edition

A modern, high-performance desktop application for searching, browsing, and equipping VRChat avatars, powered by **Tauri v2**, **Rust**, **React 19**, **Tailwind CSS**, and **SQLite**.

---

## ✨ Features

- **🚀 Ultra-Fast Avatar Search**: Indexed SQLite queries with sub-millisecond full-text and tag searches across tens of thousands of avatars.
- **🔐 VRChat Authentication & 2FA**:
  - Secure login with official VRChat API.
  - Full Two-Factor Authentication support (Email OTP & TOTP Authenticator apps).
  - Secure cookie storage and session persistence.
- **👕 1-Click Avatar Equipping**: Wear any public avatar directly in VRChat from the application.
- **🖼️ Authenticated Image Proxy**: Server-side proxying and caching of avatar thumbnails and preview images with authenticated headers to bypass CORS restrictions.
- **🗄️ Smart Multi-Source Database Sync**:
  - Automatically downloads and decrypts Main, Quest, and iOS avatar databases from GitHub Gists.
  - Multi-stage background synchronization with live progress feedback.
  - Bulk SQLite transactions with composite indexes.
- **🎨 Sleek Dark Cyberpunk Web Interface**:
  - Responsive grid layout with instant debounced search.
  - Filtering by platform (PC, Quest, iOS), performance ratings, and marketplace items.
  - Modal with avatar details, high-res preview, tags, and one-click ID copying.

---

## 📁 Project Architecture

```
prismic_tauri/
├── src-tauri/                 # Rust Tauri v2 Backend
│   ├── src/
│   │   ├── main.rs            # Desktop window entrypoint
│   │   ├── lib.rs             # Tauri initialization & plugin registration
│   │   ├── models.rs          # Data structures & IPC types
│   │   ├── config.rs          # Persistent user configuration
│   │   ├── db.rs              # SQLite connection, schema & search queries
│   │   ├── downloader.rs      # Decryption, parsing & bulk sync pipeline
│   │   ├── vrc_api.rs         # VRChat API, Auth, 2FA, avatar equip & image proxy
│   │   └── commands.rs        # Tauri IPC commands
│   ├── Cargo.toml             # Rust dependencies
│   └── tauri.conf.json        # Tauri v2 application configuration
├── src/                       # React Web Frontend
│   ├── components/
│   │   ├── Header.tsx         # Global search, platform pills, DB sync & profile badge
│   │   ├── SidebarFilters.tsx # Author, description, platform & rating filters
│   │   ├── AvatarGrid.tsx     # Responsive grid & pagination
│   │   ├── AvatarCard.tsx     # Avatar card with badges & quick wear action
│   │   ├── AvatarModal.tsx    # Detailed preview modal with metadata & tags
│   │   ├── LoginModal.tsx     # VRChat credentials & 2FA verification dialog
│   │   ├── DatabaseModal.tsx  # Database sync management & progress monitor
│   │   └── Toast.tsx          # Action notification toasts
│   ├── api.ts                 # Typed Tauri IPC wrapper with web fallback
│   ├── types.ts               # Shared TypeScript definitions
│   ├── App.tsx                # Main view & state coordination
│   └── index.css              # Tailwind base styling & scrollbars
├── package.json
├── tailwind.config.js
└── vite.config.ts
```

---

## 🛠️ Quick Start & Development

### Prerequisites
- [Node.js](https://nodejs.org/) (v18+) and `npm`
- [Rust toolchain](https://rustup.rs/) (v1.77+)
- `cargo tauri` CLI:
  ```bash
  cargo install tauri-cli --version "^2.0"
  ```

### Run in Development Mode
To launch the Tauri desktop application with Hot Module Replacement (HMR):
```bash
cd prismic_tauri
cargo tauri dev
```

Or run the web interface in a browser for rapid UI iteration:
```bash
cd prismic_tauri
npm run dev
```

### Build for Production
To build the optimized native desktop executable:
```bash
cd prismic_tauri
cargo tauri build
```
The compiled executable will be located in:
`src-tauri/target/release/`

### Standalone Binaries (no dev server required)
The `tauri/custom-protocol` feature is enabled in `Cargo.toml`, so plain cargo
builds also produce fully standalone executables with the frontend embedded:
```bash
cd prismic_tauri/src-tauri
cargo build            # debug:   target/debug/app.exe
cargo build --release  # release: target/release/app.exe
```
You can double-click either binary directly — no `npm run dev` needed.

> ⚠️ Note: because `custom-protocol` is enabled, `cargo tauri dev` will load the
> embedded (pre-built) frontend instead of the live Vite server. Run
> `npm run build` first, or use `npm run dev` in a browser for frontend HMR.
