# RepoQuest

RepoQuest is an experimental tool for interactive programming tutorials. Each lesson takes place in a Github repository, and RepoQuest uses the Github interface for issues and pull requests to provide starter code and explain programming concepts. You can see how RepoQuest works in this short video: <https://www.youtube.com/watch?v=MY75S1sEuc8>

## Installation

### Option A: Download prebuilt binary

The latest binary release is on our Github releases page: <https://github.com/cognitive-engineering-lab/repo-quest/releases/latest>

We have prebuilt binaries for MacOS (x86-64 and ARM64), Linux (x86-64), and Windows (x86-64 and ARM64).

#### MacOS

1. Download the `.dmg` for your platform (aarch64 for M-series Macs, x64 otherwise).
2. Drag `RepoQuest.app` into `/Applications`.
3. Configure your OS to allow the app by running at the terminal:
   ```console
   xattr -cr /Applications/RepoQuest.app
   ```

#### Linux

1. Download the relevant package file for your distro (`.deb` for Ubuntu, `.rpm` for Fedora, `.AppImage` otherwise).
2. Install it through your package manager (e.g., `apt install ./the-file.deb`).

#### Windows

1. Download the installer (either `.msi` or `.exe`) for your platform (arm64 for ARM chips, x64 otherwise).
2. Run the installer.

Note that Windows will aggressively prevent you from running the installer. For example, in Edge you will need to right click the download and select "Keep", then hit "Show More", then hit "Keep anyway". When you open the installer, Windows Defender will stop it from running &mdash; again, hit "More Info" and click "Run anyway".

### Option B: Build from source

You will need [Rust](https://rustup.rs/) and [pnpm](https://pnpm.io/installation). Then install `tauri-cli`:

```console
cargo install tauri-cli --version "^2.0.0-rc"
```

Then clone the repository and build it:

```console
git clone https://github.com/cognitive-engineering-lab/repo-quest
cd repo-quest/rs/crates/repo-quest
cargo run --bin export-bindings
cargo tauri build
```

Then install the generated bundle. (TODO: flesh this out)

## Setup

To setup RepoQuest, you first need to have Git installed on your computer with the `git` executable accessible on your PATH. You will need a Github account.

### Github Token

You need to generate a Github access token that allows RepoQuest to perform automatically Github actions (e.g., filing an issue). You can do this in one of two ways:

#### Option A: Generate a one-off token

Go to <https://github.com/settings/tokens/new>. Select the **repo** scope. Click "Generate Token" at the bottom. Copy the token into the file `~/.rqst-token`. On MacOS, you can run:

```console
pbpaste > ~/.rqst-token
```

*Note:* these tokens will expire after a few months. You will have to refresh the token if you want to use RepoQuest after its expiration.

#### Option B: Use the github CLI

Install the `gh` tool following these instructions: <https://github.com/cli/cli#installation>

Then login by running:

```console
gh auth login
```

Try running `gh auth token`. If that succeeds, then you're good.

### Launching RepoQuest

Next, you need to launch the RepoQuest app. This depends on which OS you're using.

#### MacOS

You can launch the app via the finder (Cmd+Space) by searching for "RepoQuest". Or you can add `/Applications/RepoQuest.app/Contents/MacOS` to your `PATH` and run `repo-quest` from the command line.

#### Linux

Run `repo-quest` from the command line.

#### Windows

Search for "RepoQuest" in your applications list and run it.