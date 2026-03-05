# Add a chapter

To add a new chapter, create a new commit or sequence of commits in the
linear-history format and make a branch for each commit with the correct
structure.

First, ensure that `hist` is up-to-date with the directory representation.

> [!WARNING]
> Unlike with how the `dirs` command checks for changes before overwriting, the
> `hist` command will overwrite any pending changes in the `hist` directory.

```shell
repo-quest hist
```

Switch to the main branch and move it to the last commit of the last chapter.

```shell
git switch main
git reset --hard quest/chapter/first-chapter/solution/implement-add
```

## Create the scaffolding

The new chapter will have an additional task, asking the learner to implement
some simple functions. The scaffolding commits will add the tests and the
solution commits will add the implementations.

In order to reduce the chances of conflicts when incorporating the scaffolding
into the learner's repository, try to keep the changes made for scaffolding and the
changes made for the solution in separate files. If there is a conflict, the
generated pull request for the chapter will discard the learner's changes for
earlier chapters and replace them with the reference solutions.

In this quest, the learner will be modifying `src/operations.rs` and the scaffolding
changes will be to `src/main.rs`.

Modify the tests in `src/main.rs` to include the following tests.

```rust
#[test]
fn test_sub() {
    assert_eq!(sub(3,1), 2);
}

#[test]
fn test_mul() {
    assert_eq!(mul(2,3), 6);
}
```

Commit the changes and create a branch to indicate that the commit is a
scaffolding commit.

```shell
git add -u .
git commit -m "Add sub and mul tests"
git branch -c quest/chapter/sub-and-mul/scaffold/add-tests
```

When converting back to the directory format, `sub-and-mul` will become the
label and directory name for the chapter and `add-tests` will become the label
and directory name for the commit.

## Create the reference solution commits

Modify `src/operations.rs` to add the `sub` implementation.

```rust
pub fn sub(x: u32, y: u32) -> u32 { x - y }
```

Commit the changes and create a branch to indicate that the commit is a
solution commit.

```shell
git add -u .
git commit -m "Add sub implementation"
git branch -c quest/chapter/sub-and-mul/solution/add-sub
```

Modify `src/operations.rs` to add the `mul` implementation.

```rust
pub fn mul(x: u32, y: u32) -> u32 { x * y }
```

Commit the changes and create a branch to indicate that the commit is a
solution commit.

```shell
git add -u .
git commit -m "Add sub implementation"
git branch -c quest/chapter/sub-and-mul/solution/add-mul
```

## Define the task

To continue defining the chapter, we first convert back to the directory format.

```shell
repo-quest dirs
```

If you attempt to view the structure of the quest now with `repo-quest ls`, you
will see an error:

```plaintext
Error: Could not read issue file "/home/author/my-quest/chapters/sub-and-mul/issue.md"
```

This is because even though you can defined the commits for a chapter with the
linear-history format, you did not define the instructions for the user. To do
so, create the mentioned file and add the following content.

```markdown
+++
title = "Add sub and mul"
+++
Add `sub` and `mul` functions that take and return `u32` to `operations.rs`.
```

This file defines the Forgejo issue that will be created for the user when they
start the chapter. The metadata block delimited by the `+++` marks is TOML and
defines the title of the issue. The remainder of the file is Markdown and will
become the body of the issue.

Once you have saved the file, you can run `repo-quest ls` again and you will see
the structure of the quest with the new chapter and commits added.

```plaintext
Quest Title
├── main
│   └── initialize-project
├── first-chapter
│   ├── scaffold
│   │   └── add-test
│   └── solution
│       └── implement-add
└── sub-and-mul
    ├── scaffold
    │   └── add-tests
    └── solution
        ├── add-sub
        └── add-mul
```

Stage and commit the changes to the quest with git.

```sh
git add .
git commit -m "Add sub-and-mul chapter"
```
