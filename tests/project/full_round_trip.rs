//! Feature test for the Project / ConversationRoot / KnowledgeRoot slice.
//!
//! User story (narrative):
//!
//! A CLI bootstrap author runs `ailly --root <project> --knowledge <extra>`
//! against a project tree at `project/` that holds an `AGENTS.md`, two
//! skills (`local` and `shared`) under `skills/`, and a single
//! conversation turn at `01.toml`. The author also passes one extra
//! knowledge tree at `extra_kb/` that holds its own `AGENTS.md`, an
//! `extra` skill that the project does not carry, and a same-named
//! `shared` skill that the precedence rules must shadow.
//!
//! The author assembles a `Project` whose `root` is `project/`, whose
//! `conversations` is the project root in conversation mode, and whose
//! `knowledge` is `[project, extra_kb]` in argument order. The harness
//! wraps the assembled knowledge roots in an `FsKnowledgeBase` and
//! threads the project's `ConversationRoot` plus the `KnowledgeBase`
//! through `Conversation::load`. Six observable runtime properties of
//! the slice are then exercised end to end:
//!
//! 1. `Project` is built from a `ProjectRoot`, a `ConversationRoot`
//!    derived from the project root in conversation mode, and a
//!    `Vec<KnowledgeRoot>` whose first entry is the project root and
//!    whose second entry is the additional `--knowledge` directory in
//!    argument order.
//! 2. `KnowledgeBase::skill("shared")` returns the project-root copy
//!    of the body, not the extra root's same-named copy. First-wins
//!    precedence is observable across roots.
//! 3. `KnowledgeBase::skill("extra")` returns the extra root's body.
//!    Lower-precedence roots are still discoverable when no project
//!    copy exists.
//! 4. `KnowledgeBase::list_skills()` returns one `SkillSummary` per
//!    unique skill name across both roots (`extra`, `local`, `shared`),
//!    and the surviving `shared` summary's `KnowledgeSource` points at
//!    the project root.
//! 5. `KnowledgeBase::agents()` returns one `AgentsDoc` per root that
//!    holds an `AGENTS.md`, in root iteration order: project's
//!    `AGENTS.md` body first, extra root's `AGENTS.md` body second.
//! 6. `Conversation::load` over the conversation root excludes the
//!    `skills/` subtree from its recursive walk. The top-level
//!    `01.toml` is enumerated as a turn, but `skills/local/SKILL.md`
//!    is not. (`.ailly/` and `workflows/` are similarly excluded; this
//!    test covers `skills/` as the representative exclusion.)
//!
//! A separate property — that `FsEdit::new` accepts only a
//! `&ProjectRoot` and the Rust type system rejects a `ConversationRoot`
//! or `KnowledgeRoot` at compile time — is enforced by the design's
//! negative-build gate and is not exercised at runtime here.

use std::sync::Arc;

use ailly::content::Conversation;
use ailly::knowledge::base::{FsKnowledgeBase, KnowledgeBase};
use ailly::knowledge::skills::SkillName;
use ailly::mem_fs;
use ailly::project::{ConversationRoot, KnowledgeRoot, Project, ProjectRoot};

#[tokio::test]
async fn project_with_two_knowledge_roots_threads_through_conversation_load() {
    let fs = mem_fs! {
        "project": {
            "AGENTS.md": "# project agents\n",
            "skills": {
                "local": {
                    "SKILL.md": "---\nname: local\ndescription: project-only skill\n---\nLOCAL BODY\n",
                },
                "shared": {
                    "SKILL.md": "---\nname: shared\ndescription: project shared skill\n---\nPROJECT-SHARED BODY\n",
                },
            },
            "01.toml": "prompt = 'hello'\n",
        },
        "extra_kb": {
            "AGENTS.md": "# extra agents\n",
            "skills": {
                "extra": {
                    "SKILL.md": "---\nname: extra\ndescription: extra-only skill\n---\nEXTRA BODY\n",
                },
                "shared": {
                    "SKILL.md": "---\nname: shared\ndescription: extra shared skill\n---\nEXTRA-SHARED BODY\n",
                },
            },
        },
    };

    // 1. Project assembles from a ProjectRoot, a derived ConversationRoot,
    //    and a Vec<KnowledgeRoot> whose first entry is the project root.
    let project_root = ProjectRoot::try_from(fs.join("project").expect("join project"))
        .expect("project root constructs from a valid directory");
    let extra_root = KnowledgeRoot::try_from(fs.join("extra_kb").expect("join extra_kb"))
        .expect("extra knowledge root constructs from a valid directory");

    let project = Project {
        root: project_root.clone(),
        conversations: ConversationRoot::from(project_root.clone()),
        knowledge: vec![KnowledgeRoot::from(project_root.clone()), extra_root],
        bash_cwd: std::path::PathBuf::from("."),
    };

    assert_eq!(
        project.knowledge.len(),
        2,
        "knowledge list carries project root then extra root in argument order"
    );
    assert_eq!(
        project.knowledge[0].as_path().as_str(),
        project.root.as_path().as_str(),
        "first knowledge root is the project root"
    );

    let kb: Arc<dyn KnowledgeBase> = Arc::new(FsKnowledgeBase::new(project.knowledge.clone()));

    // 2. Skill lookup is first-wins across roots: project copy of `shared` wins.
    let shared = kb
        .skill(&SkillName::try_from("shared").expect("valid skill name"))
        .expect("shared skill resolves from FsKnowledgeBase");
    assert!(
        shared.body.as_str().contains("PROJECT-SHARED BODY"),
        "first-wins: project root's `shared` body is returned, got: {:?}",
        shared.body.as_str()
    );

    // 3. Extra-root entries remain discoverable when no project copy exists.
    let extra = kb
        .skill(&SkillName::try_from("extra").expect("valid skill name"))
        .expect("extra skill resolves from FsKnowledgeBase");
    assert!(
        extra.body.as_str().contains("EXTRA BODY"),
        "extra root supplies the `extra` body, got: {:?}",
        extra.body.as_str()
    );

    // 4. list_skills dedupes by name; the surviving source is first-wins.
    let mut summaries = kb.list_skills().expect("list_skills succeeds");
    summaries.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
    let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["extra", "local", "shared"],
        "list_skills returns one summary per unique name across both roots"
    );
    let shared_summary = summaries
        .iter()
        .find(|s| s.name.as_str() == "shared")
        .expect("shared summary present in list_skills output");
    assert_eq!(
        shared_summary.source.root.as_path().as_str(),
        project.root.as_path().as_str(),
        "surviving `shared` summary is sourced from the project root"
    );

    // 5. agents() returns one AgentsDoc per root with an AGENTS.md, in
    //    root iteration order: project first, extra second.
    let agents = kb.agents().expect("agents() succeeds");
    assert_eq!(
        agents.len(),
        2,
        "both roots contributed an AGENTS.md, both are returned"
    );
    assert!(
        agents[0].body.as_str().contains("project agents"),
        "first agents doc is the project's, got: {:?}",
        agents[0].body.as_str()
    );
    assert!(
        agents[1].body.as_str().contains("extra agents"),
        "second agents doc is the extra root's, got: {:?}",
        agents[1].body.as_str()
    );

    // 6. Conversation::load excludes the knowledge subdirs from the walk.
    let conversation = Conversation::load(&project.conversations, kb.as_ref())
        .await
        .expect("conversation loads against the conversation root and KnowledgeBase");

    let turn_paths: Vec<String> = (0..conversation.turn_count())
        .map(|i| conversation.turn(i).path().as_str().to_string())
        .collect();

    assert!(
        turn_paths.iter().any(|p| p.ends_with("/01.toml")),
        "the top-level 01.toml is enumerated as a turn, got turn_paths={turn_paths:?}"
    );
    assert!(
        !turn_paths.iter().any(|p| p.contains("/skills/")),
        "skills/ subtree is excluded from the conversation walk, got turn_paths={turn_paths:?}"
    );
}
