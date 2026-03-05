# Quest representations

A RepoQuest quest is a sequence of chapters, each of which builds on the
previous chapter. Each chapter includes (optional) scaffolding code, a reference
solution, and instructions for the learner.

While authoring, there are two useful formats for a quest.

The first represents chapters of the quest as a sequence of directories. This
flat representation makes it possible to track the development of the quest
using git.

The second represents the quest as a git repository with a linear history. Each
directory from the first representation becomes a commit in the second with a
branch identifying it. The additional metadata, such as the instructions to the
user for each chapter, are represented as files in the content of a single
commit disconnected from the linear history. This representation makes it easier
to make changes that must be propagated into other commits. It also can be used
for running any continuous integration workflows that are included in each
chapter.

The RepoQuest binary provides functionality for converting between the two
formats and for performing some common actions involved in quest authoring and
maintenance.
