# RepoQuest Guide

This is a brief guide to how to use RepoQuest.

## Starting a Quest

1. Select "Start a new quest".
2. Select your desired quest.
3. Select a directory. RepoQuest will clone the quest repository as a subdirectory of your selected directory.
4. Click "Create" and wait a few seconds.
5. Open the "Quest directory" in your code editor.
6. Follow the directions below to do the first chapter of the quest.

## Doing a Quest

1. Click "File Issue" to start the chapter.
2. Click the "Issue" link and read the issue.
3. If the chapter has a "Starter PR", click that link. Read and merge the PR. **Make sure to pull the changes!**
4. Follow the instructions in the issue to complete the chapter.
5. Commit and push your local changes.
6. Once you're done, close the issue.
7. After a few seconds (or click "Refresh UI state"), then the next chapter should appear.

## If You Get Stuck

If you need help, you can review our reference solution for a given chapter. Click the "Help" button next to the chapter title. If you're still lost, you can replace your code with the reference solution by clicking "File reference solution". This will create a solution PR that you can review and merge.

## ⚠️ Pitfalls 🕳️

RepoQuest has some sharp edges. Some are inherent to the quest concept, and some are just because RepoQuest is under development. Below are some pitfalls to know.

* Unlike a normal textbook, a quest is highly stateful. Once you go to a new chapter, you can't go back to the previous one, or undo changes (without deleting the repo and starting again).

* A quest is setup such that you should write code in one place (file, block of text, etc.), and starter code is provided in a different place. That way, the starter code should never cause a merge conflict with your changes. However, if you commit changes outside the "game area" (so to speak), you will probably cause a merge conflict.

  In these cases, RepoQuest's fallback behavior is to create a PR that hard resets your repo to a known good state. This lets you proceed with the quest, but it replaces your running solution with the reference solution. These PRs will be tagged with a `reset` label.

  The goal of RepoQuest is to avoid hard resets at all costs (except when you explicitly ask for the reference solution). If you encounter a hard reset, please let us know!

* The RepoQuest UI infrequently polls Github for the state of your repo. If you perform an action within Github (lke merging a PR) and the UI doesn't seem to update, try clicking the "Refresh" button in the control panel.