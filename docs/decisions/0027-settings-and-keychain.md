# 0027. Settings in the app, keys in the keychain

Status: accepted

## Context

Version 1 needed a hand-written `kirpitch.toml` to use a model, and the file
could not hold keys (ADR 0026). People expect to choose providers, models,
endpoints, keys and folders in the app.

## Decision

- The settings screen edits the same `kirpitch.toml` that the command line reads.
  The file in use is `kirpitch.toml` in the current folder when there is one,
  else `%APPDATA%\Kirpitch\kirpitch.toml` (Windows),
  `~/Library/Application Support/Kirpitch` (macOS) or
  `$XDG_CONFIG_HOME/kirpitch` (Linux). `KIRPITCH_CONFIG_DIR` replaces the folder.
  The file is written whole to a side file and renamed, and comments of a
  hand-written file are not kept; the screen says where the file is.
- API keys are kept in the operating system keychain (Windows Credential
  Manager, macOS Keychain, the Linux kernel keyring), under the name of the
  model, through the `keyring` crate (MIT OR Apache-2.0) with its native
  backends only. The Linux secret-service backend is left out because it pulls
  in `zbus` crates that cargo-deny flags as unmaintained. A model marks
  `keychain = true`; the key itself is never in the file. An environment
  variable named in `api_key_env` still wins when it is set, so the command line
  and CI work as before. The command line reads keychain keys too.
- A new `[folders]` section holds the default folders for parts, KiCad output
  and datasheets, the KiCad version, a fab profile file and the pdfium folder.
  The command line ignores it, except that the fab profile and pdfium folder
  are used by the app.
- A model can be tested from the screen (reachable, structured JSON, images,
  long text) after it is saved. The screen shows for each task whether its data
  stays local, and `local_only` is a switch there.

## Consequences

Keys cannot be read back by the screen (it only knows that one is stored). A
keychain that is locked or missing is reported as an error text. Settings are not
shared between machines, by design.
