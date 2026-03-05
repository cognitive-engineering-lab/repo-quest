# Quest repository format

The linear-history repository format for a quest makes it easier to make edits
to the content and structure of the quest. This format does not include all of
the information required to define a quest: for example, it omits task
instructions. It does include the chapter structure, commits, and commit
messages.

The repository format consists of a git repository with a linear history of
commits. Each commit has a corresponding branch whose name has one of the
following formats:

- `quest/main/commit-label`,
- `quest/chapter/chapter-label/scaffold/commit-label`, or
- `quest/chapter/chapter-label/solution/commit-label`,

where each of the `commit-label` and `chapter-label` components are replaced by
concrete commit and chapter labels.

The commits with branches of the form `quest/main/commit-label` correspond to
the commits in the directory structure in the `main` directory. The commits the
other format correspond to the scaffold or reference solution commits for a
chapter with the given label.

All commits for a chapter must be adjacent, all commits for `quests/main`
must come before any chapter commits, and all commits must have exactly one
branch with the given format.
