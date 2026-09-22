//! Walker filtering tests: skip-list membership and gitignore/hidden
//! filtering against a temp tree.

use jevr::walk::{is_skipped_dir, searchable_files};
use std::{env, fs, path::Path};

#[test]
fn generated_and_dependency_directories_are_skipped() {
    assert!(is_skipped_dir(Path::new("project/node_modules")));
    assert!(is_skipped_dir(Path::new("project/target")));
    assert!(!is_skipped_dir(Path::new("project/src")));
}

#[test]
fn gitignored_and_hidden_files_are_filtered() {
    let root = env::temp_dir().join(format!("jevr-walk-{}", std::process::id()));
    fs::create_dir_all(root.join("ignored")).unwrap();
    fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
    fs::write(root.join("visible.rs"), "fn visible() {}\n").unwrap();
    fs::write(root.join(".hidden.rs"), "fn hidden() {}\n").unwrap();
    fs::write(root.join("ignored/no.rs"), "fn ignored() {}\n").unwrap();
    fs::write(root.join("notes.md"), "# notes\n").unwrap();
    fs::write(root.join("report.pdf"), "%PDF-1.4\n").unwrap();

    let visible = searchable_files(&root, false).unwrap();
    assert_eq!(
        visible,
        vec![root.join("notes.md"), root.join("visible.rs")]
    );
    let with_hidden = searchable_files(&root, true).unwrap();
    assert_eq!(
        with_hidden,
        vec![
            root.join(".hidden.rs"),
            root.join("notes.md"),
            root.join("visible.rs")
        ]
    );

    fs::remove_dir_all(root).unwrap();
}
