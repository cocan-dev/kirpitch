# Install and run Kirpitch

This page is for people who want to **use** Kirpitch, not change its code.

## The short version

There is no ready-made installer yet (no release has been published). Until
there is one, you build the app once on your own computer. It takes about
15 minutes, most of it waiting, and you do not need to know Rust. You copy
a few commands.

You need:

- a computer with Windows 10 or 11, macOS or Linux (64-bit),
- an internet connection for the first setup,
- a few GB of free disk space (the build files are large).

KiCad is **not** needed to run Kirpitch. Install it if you want to open the
parts it makes.

## Windows

1. **Install Rust.** Go to <https://rustup.rs>, download `rustup-init.exe` and
   run it. When it says it needs the *Visual Studio C++ Build Tools*, let it
   install them (choose "Desktop development with C++" if it asks). Accept the
   default options for everything else. Close and reopen any terminal window
   when it is done.
2. **Get Kirpitch.** Open <https://github.com/cocan-dev/kirpitch>, press
   **Code**, then **Download ZIP**, and unpack it somewhere you will find it
   again, for example `C:\Kirpitch`. (If you have Git: `git clone
   https://github.com/cocan-dev/kirpitch`.)
3. **Open a terminal in that folder.** In File Explorer, open the folder that
   holds `README.md`, click the address bar, type `powershell` and press Enter.
4. **Download the PDF reader library** (once; it is what shows and reads
   datasheet pages):

   ```sh
   cargo xtask fetch-pdfium
   ```

5. **Start the app:**

   ```sh
   cargo run -p kirpitch-app --release
   ```

   The first time this compiles everything, which takes 5 to 15 minutes and
   prints a lot of text. That is normal. When it finishes, the Kirpitch window
   opens.

To start it again later, open a terminal in the same folder and run the last
command again; now it starts in a few seconds.

Windows 11 already has the web view the window needs (WebView2). On Windows 10
install the "Evergreen" WebView2 runtime from Microsoft if the window stays
blank.

## macOS

1. Open Terminal and run `xcode-select --install` (installs Apple's command
   line tools; skip it if it says they are already installed).
2. Install Rust: run the command shown at <https://rustup.rs>.
3. Download the ZIP from <https://github.com/cocan-dev/kirpitch> (**Code**,
   **Download ZIP**), unpack it, and in Terminal go to the folder
   (`cd ~/Downloads/kirpitch-main`).
4. Run `cargo xtask fetch-pdfium`, then `cargo run -p kirpitch-app --release`.

## Linux

1. Install the build tools and the web view libraries. On Debian or Ubuntu:

   ```sh
   sudo apt install build-essential curl git libwebkit2gtk-4.1-dev libgtk-3-dev libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev
   ```

   For other distributions see the
   [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux).
2. Install Rust: run the command shown at <https://rustup.rs>.
3. `git clone https://github.com/cocan-dev/kirpitch`, then `cd kirpitch`.
4. Run `cargo xtask fetch-pdfium`, then `cargo run -p kirpitch-app --release`.

## Your first part

The start screen has four buttons:

- **New part…** makes a part from a form (package type, pin count, pitch). No
  model and no datasheet needed. This is the quickest way to see how Kirpitch
  works.
- **From datasheet…** reads a PDF datasheet step by step: you point at the part
  number, the pin table and the package drawing, a language model reads each,
  and you check and correct every result before the next step. It needs a model
  (see below).
- **Open…** opens a part you made before (a `.partspec.json` file).
- **My parts** lists every part you made, with its footprint drawing, to search,
  open, rename or delete (right-click a card, or use its **⋯** button).

Then work through the four review steps (Pinout, Footprint, 3D model, Export),
approve each, and **Export** writes the KiCad files (`.kicad_sym`,
`.kicad_mod` and the 3D model).

## Setting up a model (only for "From datasheet…")

Press the gear icon (**Settings**) and add a model:

- **Free and private: Ollama on your own computer.** Install Ollama from
  <https://ollama.com>, run `ollama pull qwen3-vl:7b` (or another model), then
  in Settings add a model of kind Ollama. Nothing leaves your computer.
- **A cloud model** (Claude, OpenAI, Gemini, Mistral, OpenRouter, Ollama
  cloud...): pick it from the presets, paste your API key. The key is stored in
  your system keychain, never in a file. Datasheet text is sent to that
  provider, so switch **local only** on if that must not happen.

Use **Test** on the model card to check that it answers. Hints on which models
read datasheets well are in [model-recommendations.md](model-recommendations.md).

If Settings says **pdfium not found**, run `cargo xtask fetch-pdfium` again in
the Kirpitch folder, or choose the folder it printed under **Folders** in
Settings.

## Where things are kept

- **Settings** (`kirpitch.toml`): `%APPDATA%\Kirpitch` on Windows,
  `~/Library/Application Support/Kirpitch` on macOS, `~/.config/kirpitch` on
  Linux. Settings shows the exact file.
- **Where your datasheets are** (`datasheets.json`, in the same folder): a part
  file keeps the hash of its datasheet, not its path, so that the file can be
  shared. This list is how a part finds its PDF again on your computer, also
  after it is renamed. If a datasheet was moved to another folder, type its path
  once.
- **API keys:** the system keychain (Windows Credential Manager, macOS
  Keychain, Linux keyring).
- **Your parts:** where you save them; **My parts** looks in the folders you
  opened or built into, in the parts folder set under Settings, and in folders
  you add there.
- Kirpitch never changes the example parts: they open as working copies.

## Update

Download the newest ZIP and repeat step 5, or, if you used Git, run `git pull`
in the folder and then `cargo run -p kirpitch-app --release`. Your settings and
parts are not in that folder, so they are kept.

## Remove

Delete the Kirpitch folder. To also remove settings, delete the settings folder
above and the keychain entries named after your models.

## Problems

| What you see | What to do |
| --- | --- |
| `cargo` is not found | Close and reopen the terminal after installing Rust. |
| `linker link.exe not found` (Windows) | Install the Visual Studio C++ Build Tools (step 1), then try again. |
| The window opens blank (Windows 10) | Install the WebView2 Evergreen runtime. |
| `pdfium not found`, or datasheet pages do not show | Run `cargo xtask fetch-pdfium` in the Kirpitch folder (needs `curl` and `tar`, which Windows 10 and later, macOS and Linux have). |
| "empty answer" or a very long wait with a model | Some models think for a long time and ignore "thinking off". Try another model, or raise the answer limit and time limit under Settings, Extraction. |
| A scanned PDF is refused | Kirpitch reads text-based PDFs only; scanned pages (images) are not supported yet. |

Something else? Open an [issue](https://github.com/cocan-dev/kirpitch/issues)
and say your system and what the terminal printed.

For the command-line tool (`kirpitch`), see the [README](../README.md).
