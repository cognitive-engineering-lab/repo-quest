# Create a quest

To create a new quest in a directory `my-quest`, run the following.

```shell
repo-quest init my-quest
```

The `init` command creates a quest directory `my-quest` that contains a basic
quest definition.

```plaintext
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

Edit `quest.toml` to set the title, author, and description of the quest. Also
set the `repo` value to a name that will be used as the name for the repository
created when a learner starts the quest.

Then, initialize the directory as a git repository and commit everything.

```shell
cd my-quest
git init .
git add .
git commit -m "Initial commit"
```

You can view the current state of the quest (chapters and commits) using
`repo-quest tree`.

```shell
repo-quest ls
```

```plaintext
Quest Title
├── main
│   └── initialize-project
└── first-chapter
    ├── scaffold
    │   └── add-test
    └── solution
        └── implement-add
```
