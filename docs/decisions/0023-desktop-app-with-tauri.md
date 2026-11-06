# 0023. The desktop app: Tauri with a plain HTML and JavaScript frontend

- **Status:** accepted (6 November 2026)

## Context

The requirements name a Tauri desktop app as the main interface (milestone 3).
The maintainer asked to start it. Tauri shows web pages in the operating
system's own web view (WebView2 on Windows, WebKitGTK on Linux, WKWebView on
macOS), so there is no bundled browser.

## Decision

- Crate `kirpitch-app`: `tauri` 2 (Apache-2.0 OR MIT, released October 2026,
  actively maintained). The window logic lives in `lib.rs` as plain functions
  that return serde structs (`SpecView`); `main.rs` only connects them to the
  web view as commands (`open`, `approve`, `export`). The functions are tested
  without a window.
- The frontend is plain HTML, CSS and JavaScript in `ui/`, without a build step
  or Node toolchain. It uses the global `window.__TAURI__` object. The content
  security policy allows only the app's own files.
- No server and no network: files are read and written on this machine only.
- The first version shows the two gates (pinout, footprint) with their status,
  errors, warnings and approval; the pin and pad tables with source pages and low
  confidence marks; the findings; a drawing of the footprint made from the
  PartSpec; and writes the KiCad files for a chosen version.
- Paths are typed in. A file dialog needs the Tauri dialog plugin and comes
  later.

## Consequences

- `cargo deny` reports one unmaintained advisory, `proc-macro-error`
  (RUSTSEC-2024-0370), a compile-time macro used by glib-macros in Tauri's GTK
  backend. It is ignored in `deny.toml` with that reason; check again at each
  Tauri update.
- CI installs the WebKitGTK development packages on Linux, because the app crate
  is part of the workspace.
- Later screens (side-by-side datasheet pages, region pick, 3D view, library)
  build on the same commands.
