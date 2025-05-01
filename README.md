# RepoQuest

RepoQuest is an experimental tool for interactive programming tutorials. Each lesson takes place in a Github repository, and RepoQuest uses the Github interface for issues and pull requests to provide starter code and explain programming concepts.

## Setup

Before starting, you will need Git and Rust installed on your computer. You will also need a Github account.

### 1. Install RepoQuest

You need [Rust](https://www.rust-lang.org/tools/install) installed. Then run:

```
cargo install repo-quest --locked
```

### 2. Generate a Github token

You need to generate a Github access token that allows RepoQuest to perform automatically Github actions (e.g., filing an issue). You have two options:

#### Option A: Use the Github CLI (Recommended)

Install the `gh` tool following these instructions: <https://github.com/cli/cli#installation>

Then login by running:

```console
gh auth login
```

Try running `gh auth token`. If that succeeds, then you're good.

#### Option B: Generate a one-off token

Go to <https://github.com/settings/tokens/new>. Select the **repo** scope. Click "Generate Token" at the bottom. Copy the token into the file `~/.rqst-token`. On MacOS, you can run:

```console
pbpaste > ~/.rqst-token
```

*Note:* these tokens will expire after a few months. You will have to refresh the token if you want to use RepoQuest after its expiration.

### 3. Run RepoQuest

Finally, pick a directory where you want to create a new quest folder. In that directory, run this command:

```
repo-quest
```

Then follow the directions in the terminal.
