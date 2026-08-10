# RepoQuest — agent notes

Read the README first: it covers the crate layout, design rationale (two
quest-definition formats, git/rsync as subprocesses, single-user locking
model), the compose/port layout, Forgejo UI customization, and the template
hot-reload loop.

Facts not in the README:

- `references/forgejo` is an untracked checkout of upstream Forgejo. Grep it
  for template/CSS-variable internals when editing `docker/custom/`; never
  edit it.
- Before `8dc0c02` ("Swap to Forgejo-based RepoQuest") this repo was a Tauri
  app (`rs/`, `js/`); ignore pre-rewrite paths when mining history.
- CI runs `cargo build && cargo test && cargo clippy`. Clippy pedantic is
  warn-level but must stay clean. rustfmt uses `tab_spaces = 4`.
- The only unit tests are in `crates/repo-quest/src/dir/parse.rs`; everything
  else is verified manually (README §Testing, `assets/skel`).
- Issue/PR text is mustache-templated: `Template` newtype in
  `repo-quest-core/src/quest/template.rs`; template variables are injected via
  `insert_str` in `repo-quest-bot/src/forgejo.rs`.
