# Quest definition structure overview and glossary

This page give an overview of the structure of a quest definition and defines
some terms used in this documentation by means of an example quest.

```
├── .git
├── .gitignore
├── quest.toml
├── main
│   ├── initialize-project
│   │   ├── Cargo.lock
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   └── src
│   │       └── main.rs
│   └── initialize-project.txt
└── chapters
    └── first-chapter
        ├── issue
        │   ├── 01-comment.md
        │   └── 02-comment.md
        ├── issue.md
        ├── pr
        │   └── 01-comment.md
        ├── pr.md
        ├── scaffold
        │   ├── add-test
        │   │   ├── Cargo.lock
        │   │   ├── Cargo.toml
        │   │   ├── README.md
        │   │   └── src
        │   │       └── main.rs
        │   └── add-test.txt
        └── solution
            ├── implement-add
            │   ├── Cargo.lock
            │   ├── Cargo.toml
            │   ├── README.md
            │   └── src
            │       ├── main.rs
            │       └── operations.rs
            └── implement-add.txt
```

The `quest.toml` contains the *quest metadata*, such as the title and author of
the quest. It also contains the chapter listing for the quest, which defines the
order of the chapters and of the commits within a chapter.

The `main` directory contains several directories and text files. Each directory
and text file represents a commit and commit message. The commits defined within
the `main` directory will be included in the learner's quest repository at the
start of the quest, before any tasks have been completed.

The `chapters` directory contains a sub-directory defining each *chapter* in the
quest. Each chapter may contain a `scaffold` directory containing directories
and text files representing *commits* and *commit messages* that represent the
*scaffolding code* provided to the learner upon starting a chapter. The chapter
must contain a `solution` directory which contains the commits and commit
messages representing a *reference solution* which builds on the scaffolding
code.

> [!NOTE]
> The word "commit" is used to describe a few different things for the various
> repository representations. The documentation here will attempt to distinguish
> between them as clearly as possible by specifying "chapter commit",
> "scaffolding commit", or "solution commit" when referring to one of the
> commits within a chapter or specifically one of the scaffold or solution
> commits. Commits within the `main` directory will also be referred to a
> "chapter commits" even though the content of `main` comes before any chapter.

The folder names are not required to be prefixed with a number. The name of the
folder will sometimes be referred to as the *chapter label* and will be the
basis of the *chapter branch name* when converting to the linear history format
of the quest.

Each chapter must have a *task description* contained in an `issue.md` file. The
content of that issue will be presented to the learner as an issue within
Forgejo. Each chapter may additionally have a `pr.md` file which contains
additional task information specifically pertaining to the scaffolding code for
the task. The content of the file will be included in the initial pull request
generated for the task.

The `issue` and `pr` folders contain additional file which will be presented as
comments on the issue or pull request. See the
[reference](../reference/directory-format.md) for more information about the
additional metadata that can be included with those files.

This `hist` directory contains a *linear-history representation* of the quest,
which may be easier to edit than the *directory representation* for some quest
creation or maintenance tasks. The RepoQuest binary includes utilities for
converting between the directory representation and linear-history
representation.
