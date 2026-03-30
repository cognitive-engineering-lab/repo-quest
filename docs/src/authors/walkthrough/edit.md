# Edit the initial commits

The `main` directory contains the commits that will already be present in the
repository when a learner starts the quest. All quests must have at least one
commit in defined in `main`. The generated quest contains a single commit,
labeled `initialize-project`. We will both the `README.md` file in this commit.

If we edited the `README.md` file directly, we would also have to edit it in
every later commit in every chapter. Instead, we will use the `repo-quest` tool
to create a linear-history repository representation of the quest and use
standard `git` operations to edit the history as a whole.

Run `repo-quest hist` in the root of the quest definition repository to create
the linear-history representation. The linear-history representation will be
created in a directory named `hist` in the root of the repository. The `hist`
directory is already included in the default `.gitignore` file.

The crated `hist` directory is a git repository with a branch defined for every
commit. The `main` branch is not associated with a specific chapter or commit
and instead is for your use in moving between commits to edit the repository.

```shell
git log --pretty=oneline --graph --decorate --abbrev-commit --all
```

```plaintext
* dbe6499 (HEAD -> main, quest/chapter/first-chapter/solution/implement-add) Implement a reference add function
* 2d5d518 (quest/chapter/first-chapter/scaffold/add-test) Add a test that needs to be implemented
* f9dc574 (quest/main/initialize-project) Initial commit
```

To edit the `README.md` and have it propagate through the later commits, use
an interactive git rebase and choose to edit the commit.

```shell
git rebase -i --update-refs --root
```

```plaintext
edit f9dc574 # Initial commit
update-ref refs/heads/quest/main/initialize-project

pick 2d5d518 # Add a test that needs to be implemented
update-ref refs/heads/quest/chapter/first-chapter/scaffold/add-test

pick dbe6499 # Implement a reference add function
update-ref refs/heads/quest/chapter/first-chapter/solution/implement-add
```

Change the content of `README.md`.

```markdown
# My Quest

This is an awesome quest where you will do cool things!
```

Amend the most recent commit with the changes.

```shell
git add -u .
git commit --amend --no-edit
```

Finish the rebase.

```shell
git rebase --continue
```

### Commit the changes

The changes to `README.md` have been propagated through all of the commits in
the linear history representation in `hist`. Now we just need to apply the
changes to the original quest directory so that we can commit them.

The command `repo-quest dirs` will apply the changes from the linear-history
representation to the quest definition and update `quest.toml`. Because the
command makes significant edits to the quest, it will only make the updates if
there are no uncommitted changes to `quest.toml`, `main`, or `chapters`.

> [!NOTE]
> The overwriting of `quest.toml` will also normalize the structure, so there
> may be some unexpected changes to the file.

Stage and commit the changes.

```shell
git add .
git commit -m "abc"
```

