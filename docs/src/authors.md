# Authoring quests

RepoQuest is useful for developing tutorials in which a learner progressively
extends a single program. Each chapter presents a learner with some scaffolding for
a problem and instructions. The learner implements a solution based on the
scaffolding, and then the subsequent chapter provides additional scaffolding and
instructions that build on the previous solution.

For example, a tutorial for building an interpreter might begin with providing
the learner with a datatype defining the syntax tree for a language and the
signature for an evaluation function and ask the learner to implement evaluation
the function. A subsequent chapter in the quest could add additional cases to
the syntax tree datatype and ask the learner to extend the evaluation function,
or could add signatures for additional functions over the syntax tree datatype
and ask the learner to implement those.

The learner completes the quest using a [forge-based interface](./learners.md)
where each chapter in the quest is completed by merging a branch with the
learner's solution into the main branch of a repository.

## Two quest representations

RepoQuest makes use of two quest formats. The first represents each scaffolding
or solution commit in a quest as a directory. This makes it possible to use git
to version control the whole quest sequence.

The second represents the quest as a git repository, where the quest sequence is
represented as a linear history of git commits. This representation makes it
easier to make edits to the content and structure of the quest.

The walk-through will demonstrate how to make use of each representation when
working on a quest.
