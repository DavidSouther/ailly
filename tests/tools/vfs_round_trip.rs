//! Feature test for the built-in vfs file system tools slice.
//!
//! User story (narrative):
//!
//! An agent harness composes the four built-in vfs file system tools
//! (`fs.list`, `fs.read`, `fs.grep`, `fs.edit`) against a captured `VfsPath`
//! root that holds a small project tree (`README.md`, `src/lib.rs`,
//! `src/main.rs`, `docs/design.md`). The harness wraps each tool as
//! `Arc<dyn ToolDyn>`, registers them in a `HashMapRegistry` under their
//! static `NAME` constants, resolves them back through
//! `ToolRegistry::resolve`, and walks the full tool surface in sequence:
//!
//! 1. `fs.list { "path": "src" }` returns the immediate children of `src/`
//!    as a JSON array. Within the array, entries are sorted directories
//!    first then files, lexicographic within each group.
//! 2. `fs.list { "path": "src", "glob": "**/*.rs" }` recurses (because the
//!    glob's leading segment is `**`) and surfaces only the `.rs` entries
//!    discovered under `src/`.
//! 3. `fs.grep { "path": "src", "pattern": "TODO", "glob": "**/*.rs" }`
//!    finds the single `TODO` in `src/lib.rs` line 1 and returns a
//!    `{"results":[{"path":"...","matches":[{"line":1,"text":"..."}]}],"total":1}`
//!    envelope.
//! 4. `fs.read { "path": "src/lib.rs", "range": {"start":1,"end":1} }`
//!    returns the matched line verbatim with no line numbers added.
//! 5. `fs.edit { "path": "src/lib.rs", "range": {"start":1,"end":1},
//!    "replacement": "// done" }` rewrites line 1, returning a
//!    confirmation string that names the path and the original 1-indexed
//!    range. The on-disk body shows the edit landed and the rest of the
//!    file is preserved.
//! 6. `fs.grep` re-run with the same args reports `total = 0`, proving the
//!    edit took effect on the underlying VFS that all four tools share.
//! 7. `fs.list { "path": "../oops" }` is rejected by the captured-root
//!    guard inherited from the `FsAbsent` shape (`905918d`/`aa2c25a`) and
//!    surfaces as an `Err` from `ToolDyn::call` rather than letting the
//!    walk escape the captured root.

use std::sync::Arc;

use rig::tool::ToolDyn;

use ailly::engine::{HashMapRegistry, ToolRegistry};
use ailly::mem_fs;
use ailly::tools::{FsEdit, FsGrep, FsList, FsRead};

#[tokio::test]
async fn vfs_tools_round_trip_through_registry() {
    let fs = mem_fs! {
        "root": {
            "README.md": "# project\n\nsee docs/\n",
            "src": {
                "lib.rs": "// TODO: implement\npub fn run() {}\n",
                "main.rs": "fn main() {\n    println!(\"hi\");\n}\n",
            },
            "docs": {
                "design.md": "design body\n",
            },
        }
    };
    let root = fs.join("root").expect("join root");

    let mut registry = HashMapRegistry::default();
    registry.insert(
        FsList::NAME,
        Arc::new(FsList::new(root.clone())) as Arc<dyn ToolDyn>,
    );
    registry.insert(
        FsRead::NAME,
        Arc::new(FsRead::new(root.clone())) as Arc<dyn ToolDyn>,
    );
    registry.insert(
        FsGrep::NAME,
        Arc::new(FsGrep::new(root.clone())) as Arc<dyn ToolDyn>,
    );
    registry.insert(
        FsEdit::NAME,
        Arc::new(FsEdit::new(root.clone())) as Arc<dyn ToolDyn>,
    );
    let registry: Arc<dyn ToolRegistry> = Arc::new(registry);

    // 1. fs.list non-recursive lists the immediate children of src/.
    let list_tool = registry.resolve(FsList::NAME).expect("resolve fs.list");
    let list_src_raw = list_tool
        .call(serde_json::json!({"path": "src"}).to_string())
        .await
        .expect("fs.list src succeeds");
    let list_src: serde_json::Value =
        serde_json::from_str(&list_src_raw).expect("fs.list returns JSON");
    let list_src_entries = list_src.as_array().expect("fs.list returns a JSON array");
    let list_src_names: Vec<&str> = list_src_entries
        .iter()
        .map(|entry| entry["name"].as_str().expect("entry has a name field"))
        .collect();
    assert_eq!(
        list_src_names,
        ["lib.rs", "main.rs"],
        "src/ has the two .rs files in lexicographic order"
    );
    for entry in list_src_entries {
        assert_eq!(
            entry["kind"].as_str(),
            Some("file"),
            "every entry in src/ is a file: {entry}"
        );
    }

    // 2. fs.list recursive with **/*.rs surfaces only the .rs entries.
    let list_rec_raw = list_tool
        .call(serde_json::json!({"path": "src", "glob": "**/*.rs"}).to_string())
        .await
        .expect("fs.list recursive succeeds");
    let list_rec: serde_json::Value =
        serde_json::from_str(&list_rec_raw).expect("recursive fs.list returns JSON");
    let list_rec_entries = list_rec
        .as_array()
        .expect("recursive fs.list returns a JSON array");
    let list_rec_names: Vec<&str> = list_rec_entries
        .iter()
        .map(|entry| entry["name"].as_str().expect("entry has a name field"))
        .collect();
    assert!(
        list_rec_names.contains(&"lib.rs"),
        "recursive **/*.rs walk includes lib.rs: {list_rec_names:?}"
    );
    assert!(
        list_rec_names.contains(&"main.rs"),
        "recursive **/*.rs walk includes main.rs: {list_rec_names:?}"
    );
    assert!(
        !list_rec_names.iter().any(|n| n.ends_with(".md")),
        "recursive **/*.rs walk excludes markdown entries: {list_rec_names:?}"
    );

    // 3. fs.grep finds the single TODO in src/lib.rs at line 1.
    let grep_tool = registry.resolve(FsGrep::NAME).expect("resolve fs.grep");
    let grep_args = serde_json::json!({
        "path": "src",
        "pattern": "TODO",
        "glob": "**/*.rs",
    })
    .to_string();
    let grep_raw = grep_tool
        .call(grep_args.clone())
        .await
        .expect("fs.grep succeeds");
    let grep_envelope: serde_json::Value =
        serde_json::from_str(&grep_raw).expect("fs.grep returns a JSON envelope");
    assert_eq!(
        grep_envelope["total"].as_u64(),
        Some(1),
        "fs.grep total counts every match across every file"
    );
    let grep_results = grep_envelope["results"]
        .as_array()
        .expect("envelope has a results array");
    let lib_result = grep_results
        .iter()
        .find(|r| {
            r["path"]
                .as_str()
                .map(|path| path.ends_with("lib.rs"))
                .unwrap_or(false)
        })
        .expect("results include an entry for src/lib.rs");
    let lib_matches = lib_result["matches"]
        .as_array()
        .expect("lib.rs result carries a matches array");
    assert_eq!(
        lib_matches.len(),
        1,
        "lib.rs has exactly one TODO match: {lib_matches:?}"
    );
    assert_eq!(
        lib_matches[0]["line"].as_u64(),
        Some(1),
        "TODO is reported on the 1-indexed first line"
    );
    assert_eq!(
        lib_matches[0]["text"].as_str(),
        Some("// TODO: implement"),
        "match text is the full source line as read from disk"
    );

    // 4. fs.read with a 1..=1 LineRange returns the first line verbatim.
    let read_tool = registry.resolve(FsRead::NAME).expect("resolve fs.read");
    let first_line = read_tool
        .call(
            serde_json::json!({
                "path": "src/lib.rs",
                "range": {"start": 1, "end": 1},
            })
            .to_string(),
        )
        .await
        .expect("fs.read of a single line succeeds");
    assert_eq!(
        first_line, "// TODO: implement",
        "fs.read returns the matched line with no numbering or framing"
    );

    // 5. fs.edit rewrites line 1 in place.
    let edit_tool = registry.resolve(FsEdit::NAME).expect("resolve fs.edit");
    let edit_confirmation = edit_tool
        .call(
            serde_json::json!({
                "path": "src/lib.rs",
                "range": {"start": 1, "end": 1},
                "replacement": "// done",
            })
            .to_string(),
        )
        .await
        .expect("fs.edit succeeds");
    assert!(
        edit_confirmation.contains("src/lib.rs"),
        "edit confirmation names the path: {edit_confirmation}"
    );
    assert!(
        edit_confirmation.contains("1-1"),
        "edit confirmation names the original 1-indexed range: {edit_confirmation}"
    );

    let body_after_edit = root
        .join("src/lib.rs")
        .expect("join src/lib.rs")
        .read_to_string()
        .expect("read src/lib.rs after edit");
    assert_eq!(
        body_after_edit, "// done\npub fn run() {}\n",
        "fs.edit rewrote line 1 and preserved the trailing lines and final newline"
    );

    // 6. fs.grep on the same args now reports zero matches.
    let grep_post_raw = grep_tool
        .call(grep_args)
        .await
        .expect("fs.grep succeeds after edit");
    let grep_post: serde_json::Value =
        serde_json::from_str(&grep_post_raw).expect("post-edit fs.grep returns JSON");
    assert_eq!(
        grep_post["total"].as_u64(),
        Some(0),
        "the edit removed the only TODO; post-edit grep total is zero"
    );

    // 7. fs.list rejects a path that would escape the captured root.
    let escape_attempt = list_tool
        .call(serde_json::json!({"path": "../oops"}).to_string())
        .await;
    assert!(
        escape_attempt.is_err(),
        "OutsideRoot resolution must surface as an Err from ToolDyn::call, not as an Ok payload: {escape_attempt:?}"
    );

    // 8. fs.edit creates a missing file with the replacement as body.
    let create_confirmation = edit_tool
        .call(
            serde_json::json!({
                "path": "src/notes.txt",
                "range": {"start": 1, "end": 1},
                "replacement": "first note\nsecond note\n",
            })
            .to_string(),
        )
        .await
        .expect("fs.edit creates missing file");
    assert!(
        create_confirmation.contains("created"),
        "creation confirmation announces creation: {create_confirmation}"
    );
    let created_body = root
        .join("src/notes.txt")
        .expect("join created path")
        .read_to_string()
        .expect("read created file");
    assert_eq!(created_body, "first note\nsecond note\n");
}
