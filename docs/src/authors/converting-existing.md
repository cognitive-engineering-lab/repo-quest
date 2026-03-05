# Converting an existing tutorial to a quest

The process for converting an existing tutorial to a quest depends on the
structure of the tutorial.

If it is already represented as a sequence of scaffolding and reference solution
directories, where each directory is a complete snapshot of the state of the
quest, then the easiest way is probably to manually adapt the directory
structure to match the [directory-based format](../reference/directory-format.md)
and add the additional required metadata.

If the quest is not represented like that, then the easiest way is probably to
do the tutorial within a git repository, committing each scaffolding and
solution step as you go. Then add the metadata required to match the
[repository-based format](../reference/repository-format.md) and convert the
repository to the directory based structure [using the `repoquest`
binary](./hist-to-dirs.md).
