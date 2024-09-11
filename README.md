# RepoQuest

RepoQuest is an experimental tool for interactive programming tutorials. Each lesson takes place in a Github repository, and RepoQuest uses the Github interface for issues and pull requests to provide starter code and explain programming concepts.

## Installation

### From binaries

The latest binary release is on our Github releases page: <https://github.com/cognitive-engineering-lab/repo-quest/releases/latest>

#### MacOS

1. Download the `.dmg` for your platform (aarch64 for M-series Macs, x84 otherwise).
2. Drag `RepoQuest.app` into `/Applications`.
3. In the console, run:
   ```console
   xattr -cr /Applications/RepoQuest.app
   ```

#### Linux

1. Download the relevant package file for your distro (`.deb` for Ubuntu, `.rpm` for Fedora, `.AppImage` otherwise).
2. Install it through your package manager (e.g., `apt install ./the-file.deb`).
3. Install additional dependencies. On Ubuntu, this is:
   ```console
   sudo apt install -y libgtk-3-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev libwebkit2gtk-4.1-dev libxdo-dev
   ```

### From source

You will need [Rust](https://rustup.rs/). Then install the `tauri-cli`:

```console
cargo install tauri-cli --version "^2.0.0-rc"
```

Then clone the repository and build it:

```console
git clone https://github.com/cognitive-engineering-lab/repo-quest
cd repo-quest/rs/crates/repo-quest
cargo tauri build
```

Then install the generated bundle. (TODO: flesh this out)

## Setup

To setup RepoQuest, you need a Github account. 

### Github Token

You need to generate a Github access token that allows RepoQuest to perform automatically Github actions (e.g., filing an issue). You can do this in one of two ways:

#### Generate a one-off token

Go to <https://github.com/settings/tokens/new>. Select the **repo** scope. Click "Generate Token" at the bottom. Copy the token into the file `~/.rqst-token`. On MacOS, you can run:

```console
pbpaste > ~/.rqst-token
```

#### Use the github CLI

Install the `gh` tool following these instructions: <https://github.com/cli/cli#installation>

Then login by running:

```console
gh auth login
```

Try running `gh auth token`. If that succeeds, then you're good.
