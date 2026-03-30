# Recommendations for quest development

- Only edit branches for commits as part of a `rebase --update-refs` command, to
  ensure that the changes propagate forward. This means using `git reset` to
  move the `main` branch around to make edits and using `git branch -c` to name
  label the commits with branch names that RepoQuest knows how to interpret.
