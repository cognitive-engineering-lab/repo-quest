# Test the quest commits

In order to test each commit of a quest, you can define a test command in
`quest.toml`. The command defined when creating a quest with `repo-quest init`
is `cargo test`.

```toml
test-cmd = [
    "cargo",
    "test",
]
```

The command `repo-quest test` runs the command for each commit. The output of
`repo-quest test` indicates for each commit whether the result was the expected
one or not and whether the test passed or failed.

```plaintext
EXPECTED RESULT: PASSED "/home/theo/work/trust2/example/my-quest/main/initialize-project"
EXPECTED RESULT: FAILED "/home/theo/work/trust2/example/my-quest/chapters/first-chapter/scaffold/add-test"
EXPECTED RESULT: PASSED "/home/theo/work/trust2/example/my-quest/chapters/first-chapter/solution/implement-add"
UNEXPECTED RESULT: FAILED "/home/theo/work/trust2/example/my-quest/chapters/sub-and-mul/scaffold/add-tests"
UNEXPECTED RESULT: FAILED "/home/theo/work/trust2/example/my-quest/chapters/sub-and-mul/solution/add-sub"
EXPECTED RESULT: PASSED "/home/theo/work/trust2/example/my-quest/chapters/sub-and-mul/solution/add-mul"
Error: There were unexpected test failures.
```

Commits that should have the tests fail can be marked to expect failure. For
example, the first commit of first-chapter adds a test for which the called
functions are not implemented, so the registered test command `cargo test`
should fail. The scaffold commit for `first-chapter` is marked for expecting
failure, but the scaffold commit and one of the solution commits for the
`sub-and-mul` chapter are not.

To mark them as expecting to fail, change the chapter definition to the following.

```toml
[[chapters]]
label = "sub-and-mul"
scaffold = [
    {label = "add-tests", expected = "fail"}
]
solution = [
    {label = "add-sub", expected = "fail"},
    "add-mul",
]
```

> [!NOTE]
> The above format is not the format that `repo-quest dirs` will produce. If you
> want to match that format, use the following instead:
> ```toml
> [[chapters]]
> label = "sub-and-mul"
> solution = [
>     { label = "add-sub", expected = "fail" },
>     "add-mul",
> ]
>
> [[chapters.scaffold]]
> label = "add-tests"
> expected = "fail"
> ```

Stage and commit the changes with git.

```shell
git add .
git commit -m "Fix test expectations"
```
