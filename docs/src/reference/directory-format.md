# Quest directory format

The following diagram attempts to capture the general "grammar" of the quest
directory format. The individual parts are described below. For a concrete
example, see [the authoring walkthrough](../authors/walkthrough/create.md).

```
├── quest.toml (Required)
├── main (Required, and at least one entry required)
│   ├── commit/
│   ├── commit.txt
│   └── ...
└── chapters (Required, and at least one entry required)
    ├── chapter
    │   ├── issue (Optional)
    │   │   ├── issue-comment.md
    │   │   └── ...
    │   ├── issue.md (Required)
    │   ├── pr (Optional)
    │   │   ├── pr-comment.md
    │   │   └── ...
    │   ├── pr.md (Optional)
    │   ├── scaffold (Optional)
    │   │   ├── commit/
    │   │   ├── commit.txt
    │   │   └── ...
    │   └── solution (Required, and at least one entry required)
    │       ├── commit/
    │       ├── commit.txt
    │       └── ...
    └── ...
```

- `quest.toml`: Contains the metadata about the quest definition. See below for
  the definition of the keys in the file.
- `main/`: Directory containing commits that will be included in the learner's
  repository upon starting the quest, before any chapter has been completed.
  This must contain at least one entry.
- `chapters/`: Directory containing directories defining the chapters of the
  quest. This must contain at least one entry.

- `chapter/`: Directory defining a chapter. The name of the directory is the
  chapter label, which must appear in `quest.toml`.

- `commit/`: Directory containing the contents of a commit. The name of the
  directory is the commit label, which must appear in `quest.toml`.
- `commit.txt`: Plain-text file containing the commit message of a commit. The
  name of the file must match the name of the corresponding commit directory.

- `issue.md`: Markdown file with TOML frontmatter delimited by `+++` fences.
  This defines the Forgejo issue that will be created for the learner when the
  learner starts a chapter. The frontmatter must define exactly the key `title`
  which is a string that will be used for the title of the issue.
- `issue/`: Directory containing files defining comments that will be created on
  the generated issue when a chapter is started. The comment files are
  interpreted in lexical order of filename, and so the convention is to prefix
  them with a numeric value.
- `issue-comment.md`: Markdown file with no frontmatter. The content of this
  file will be the content of a comment on the generated Forgejo issue for the
  chapter.

- `pr.md`: Markdown file with TOML frontmatter delimited by `+++` fences. This
  defines the Forgejo pull request that will be created for the learner when the
  learner starts a chapter. The frontmatter must define exactly the key `title`
  which is a string that will be used for the title of the issue.

  If omitted, the generated pull request will have a standard body that links to
  the issue and will use the same title as the issue.
- `pr/`: Directory containing files defining comments that will be created on
  the generated pull request when a chapter is started. The comment files are
  interpreted in lexical order of filename, and so the convention is to prefix
  them with a numeric value.

  The comments can be defined even if `pr.md` is omitted.
- `pr-comment.md`: Markdown file with TOML frontmatter delimited by `+++`
  fences. The content of this file will be the content of a comment on the
  generated Forgejo pull request for the chapter.

  The frontmatter, if provided, defines a quote of the code in the pull request.
  The frontmatter, if provided, must contain exactly the keys `file`,
  `end-line-side`, and `end-line`. The value for `end-line-side` must either be
  the string `"right"` or `"left"`, where `"right"` means the result of the pull
  request and `"left"` means the base of the pull request. The value for `file`
  must be a string that names a file (in the result or base of the pull request,
  depending on `end-line-side`), relative to the root of the commit directory.
  The value for `end-line` must be an integer that is the last line number of
  the code to quote.


## `quest.toml`

The `quest.toml` file defines metadata about and the structure of the quest. The
following keys are supported:

- `title`: Required. A string defining the quest title.
- `author`: Required. A string defining the quest author.
- `repo`: Required. A string defining the name to use for the Forgejo repository
  generated for the learner when they start the quest.
- `rq-version`: Required. A string defining the compatible version of
  repo-quest. Not currently checked.
- `description`: Require.d A string defining a short description of the quest to
  be presented to the learner when they install the quest.
- `test-cmd`: Optional. An array of strings giving a command to execute in each
  commit directory when `repo-quest test` is invoked.
- `main`: Required. A non-empty array of commit information. See below for the
  structure of the commit information.
- `chapters`: Required. A non-empty array of tables defining the chapters in a
  quest. The order of the chapters in this array is the order of the in which
  the chapters are interpreted.

  Each chapter table must have the key `label` whose value is the chapter label
  as a string. Each chapter may optionally have the key `scaffold` whose value
  is an array of commit information. Each chapter must have the key `solution`
  whose value is a non-empty array of commit information.

Commit information is given as either a string or table. If a string, then the
string is interpreted as the label of the commit. The label must exist in the
corresponding chapter or `main/` directory. If a table, then the table has two
keys. The key `label` is required. The value for `label` is a string that gives
the chapter label. The key `expected` is optional. The value for `expected` may
be the string `"pass"` or the string `"fail"`. If omitted or if a just string is
given for the chapter information, `"pass"` is assumed. The value indicates
whether the test command is expected to pass or fail for that commit.

## `.git`

Some `repo-quest` operations assume that the quest directory is committed to a
Git repository. If the quest is not committed, those commands will produce an
error.
