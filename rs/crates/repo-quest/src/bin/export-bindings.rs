use specta_typescript::Typescript;

fn main() {
  repo_quest::specta_builder()
    .export(
      Typescript::default(),
      "../../../js/packages/repo-quest/src/bindings/backend.ts",
    )
    .expect("Failed to export typescript bindings");
}
