# RepoQuest Guide

First, follow the [Installation] and [Setup] instructions in the RepoQuest README.

## Launching RepoQuest

Next, you need to launch the RepoQuest app. This depends on which OS you're using.

### MacOS

You can launch the app via the finder (Cmd+Space) by searching for "RepoQuest". Or you can add `/Applications/RepoQuest.app/Contents/MacOS` to your `PATH` and run `repo-quest` from the command line.

### Linux

Run `repo-quest` from the command line.

### Windows

Search for "RepoQuest" in your applications list and run it.

## Starting a Quest

1. Select "Start a new quest".
2. Select your desired quest.
3. Select a directory. RepoQuest will clone the quest repository as a subdirectory of your selected directory.
4. Click "Create".

## Doing a Quest

A quest is a series of programming challenges, or chapters. At each step, you will be given an issue describing the challenge and relevant background. You may also be given starter code that is part of the challenge.

To start a chapter, click the "File issue" button. If a starter PR is filed, then review it and merge it. Then read the filed issue to understand your task. Then try to complete the task. Once you think you're done, then close the issue to start the next chapter.

If you need help, you can review our reference solution for a given chapter. Click the "Help" button for a link. If you're still lost, you can replace your code with the reference solution by clicking "File reference solution" under "Help". This will create a solution PR that you can review and merge.

### ⚠️ Pitfalls 🕳️

RepoQuest has some sharp edges. Some are inherent to the quest concept, and some are just because RepoQuest is under development. Below are some pitfalls to know.

* Unlike a normal textbook, a quest is highly stateful. Once you go to a new chapter, you can't go back to the previous one, or undo changes (without deleting the repo and starting again).

* A quest is setup such that you should write code in one set of files, and the starter code is provided in a different set of files. That way, the starter code should never cause a merge conflict with your changes. However, if you commit changes outside the "game area" (so to speak), you will probably cause a merge conflict.

  In these cases, RepoQuest's fallback behavior is to create a PR that hard resets your repo to the reference repo. This lets you proceed with the quest, but it replaces your running solution with the reference solution. These PRs will be tagged with a `reset` label.

  The goal of RepoQuest is to avoid hard resets at all costs (except when you explicitly ask for the reference solution). If you encounter a hard reset, please let us know!

* The RepoQuest UI infrequently polls Github for the state of your repo. If you perform an action within Github (lke merging a PR) and the UI doesn't seem to update, try clicking the "Refresh" button in the control panel.

[Installation]: https://github.com/cognitive-engineering-lab/repo-quest?tab=readme-ov-file#installation
[Setup]: https://github.com/cognitive-engineering-lab/repo-quest?tab=readme-ov-file#setup