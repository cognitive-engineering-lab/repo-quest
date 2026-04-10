use std::borrow::Cow;

use crate::dir::QuestDefinition;

use anyhow::Result;
use termtree::Tree;

pub fn quest_tree(quest: &'_ QuestDefinition) -> Result<Tree<Cow<'_, str>>> {
    let mut quest_tree = Tree::new(Cow::Borrowed(quest.title.as_str()));
    let mut main_tree = Tree::new(Cow::Borrowed("main"));
    for commit in &quest.main {
        let leaf = Tree::new(commit.path.file_name().unwrap().to_string_lossy());
        main_tree.push(leaf);
    }
    quest_tree.push(main_tree);

    for chapter in &quest.chapters {
        let mut chapter_tree = Tree::new(Cow::Borrowed(chapter.label.as_str()));
        if let Some(scaffold) = &chapter.scaffold {
            let mut scaffold_tree = Tree::new(Cow::Borrowed("scaffold"));
            for commit in scaffold {
                let leaf = Tree::new(commit.path.file_name().unwrap().to_string_lossy());
                scaffold_tree.push(leaf);
            }
            chapter_tree.push(scaffold_tree);
        }

        let mut solution_tree = Tree::new(Cow::Borrowed("solution"));
        for commit in &chapter.solution {
            let leaf = Tree::new(commit.path.file_name().unwrap().to_string_lossy());
            solution_tree.push(leaf);
        }
        chapter_tree.push(solution_tree);
        quest_tree.push(chapter_tree);
    }
    Ok(quest_tree)
}
