# RepoQuest

RepoQuest is an experimental tool for interactive programming tutorials. Each lesson takes place in a Github repository, and RepoQuest uses the Github interface for issues and pull requests to provide starter code and explain programming concepts.

## Installation

### From binaries

We have pre-built binaries for MacOS (ARM and x86_64) and Linux (x86_64).

#### MacOS (ARM)

```console
wget https://github.com/cognitive-engineering-lab/repo-quest/releases/latest/download/repo-quest_aarch64-apple-darwin.tar.gz
tar -xf repo-quest_aarch64-apple-darwin.tar.gz
xattr -cr RepoQuest.app
rm -rf /Applications/RepoQuest.app && mv RepoQuest.app /Applications
```

#### MacOS (x86_64)

```console
wget https://github.com/cognitive-engineering-lab/repo-quest/releases/latest/download/repo-quest_x86_64-apple-darwin.tar.gz
tar -xf repo-quest_x86_64-apple-darwin.tar.gz
xattr -cr RepoQuest.app
rm -rf /Applications/RepoQuest.app && mv RepoQuest.app /Applications
```

#### Linux (x86_64)

```console
wget https://github.com/cognitive-engineering-lab/repo-quest/releases/latest/download/repo-quest_x86_64-unknown-linux-gnu.deb
sudo apt update -y
sudo apt install -y libgtk-3-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev libwebkit2gtk-4.1-dev libxdo-dev
sudo apt install ./repo-quest_x86_64-unknown-linux-gnu.deb
```

### From source

You will need [Rust](https://rustup.rs/). Then install the `dioxus-cli`:

```console
cargo install dioxus-cli --version 0.5.7 --locked
```

Then clone the repository and build it:

```console
git clone https://github.com/cognitive-engineering-lab/repo-quest
cd repo-quest
dx bundle --release
```

Then install the application in `dist/bundle`. (TODO: flesh out this part)

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
