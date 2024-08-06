# RepoQuest

RepoQuest is an experimental tool for interactive programming tutorials.

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
cargo install dioxus-cli --version 0.5.6 --locked
```

Then clone the repository and build it:

```console
git clone https://github.com/cognitive-engineering-lab/repo-quest
cd repo-quest
dx bundle --release
```

Then install the application in `dist/bundle`. (TODO: flesh out this part)
