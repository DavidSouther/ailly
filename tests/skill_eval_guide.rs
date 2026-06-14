//! Feature test for the `ailly-skill-eval` authoring skill and its guide.
//!
//! User story: a maintainer of an LLM-agent skillset wants to regression-test
//! an edit to one of their `SKILL.md` files. They are handed only the
//! `skills/ailly-skill-eval/` directory — `SKILL.md` plus
//! `references/method.md` — with no access to `e2e/patterns-eval/` to
//! reverse-engineer. From that directory alone they must be able to reconstruct
//! the method: what each part of a skill-eval project is for, the `assemble` ->
//! `run` -> `eval` -> `report` operator workflow, which assertions serve the
//! discovery and invocation axes, and the `improved > 0 && regressed == 0`
//! falsification gate (including how to read a deliberate null result). The
//! skill's own `description:` must route an agent to it for "build an eval for
//! my skills" / "regression-test a SKILL.md edit" situations.
//!
//! This test asserts the user-story outcome structurally over the produced
//! artifacts. Each block maps to a metric from
//! `.ailly/developer/2026-06-01-A-skill-testing-docs/design.md`:
//!
//!   M1  self-contained directory that names the project anatomy and the
//!       four-verb workflow (an agent can scaffold from the skill alone).
//!   M2  the `description:` is a discovery surface for skillset-eval authoring.
//!       The structural test confirms the surface exists and is non-trivial;
//!       the concrete positive/negative routing *boundary* is dogfooded by the
//!       discovery cases that `developer:plan` defines, per the design — not by
//!       this test.
//!   M3  no schema is duplicated; the guide links out to DESIGN.md.
//!   M4  the falsification gate and null-result reading are stated explicitly.
//!
//! Plus the design's Fidelity rule: the built falsification arm is
//! `baseline.yaml`, never the README's `invocation-baseline.yaml`.
//!
//! Fails until `skills/ailly-skill-eval/SKILL.md` and `references/method.md`
//! exist with the content the design specifies.

use std::path::Path;
use std::path::PathBuf;

/// Single knob: the design's Specification frontmatter pins `ailly-skill-eval`,
/// while the Summary line says `skill-eval`. The deferred "final name" decision
/// is settled during writing and validated against the discovery-dogfooding
/// metric. If writing settles a different slug, change this one constant.
const SKILL_NAME: &str = "ailly-skill-eval";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn skill_dir() -> PathBuf {
    repo_root().join("skills").join(SKILL_NAME)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Extract inline-markdown link targets (`[text](target)`), trimming any
/// `"title"` suffix and `#fragment`. Reference-style links are not used by the
/// design's artifacts, so inline parsing is sufficient.
fn link_targets(markdown: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let bytes = markdown.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']'
            && bytes[i + 1] == b'('
            && let Some(close) = markdown[i + 2..].find(')')
        {
            let raw = &markdown[i + 2..i + 2 + close];
            let target = raw.split_whitespace().next().unwrap_or("");
            let target = target.split('#').next().unwrap_or("");
            if !target.is_empty() {
                targets.push(target.to_string());
            }
            i += 2 + close + 1;
            continue;
        }
        i += 1;
    }
    targets
}

fn is_external(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://") || target.starts_with("mailto:")
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "single test covering the full user story; splitting would obscure the assertion-to-metric mapping"
)]
fn skill_eval_guide_is_reusable_from_its_directory_alone() {
    let dir = skill_dir();
    let skill_md = dir.join("SKILL.md");
    let method_md = dir.join("references").join("method.md");

    // --- M1: self-contained directory ---------------------------------------
    assert!(
        skill_md.exists(),
        "skills/{SKILL_NAME}/SKILL.md must exist (self-contained skill)",
    );
    assert!(
        method_md.exists(),
        "skills/{SKILL_NAME}/references/method.md must exist (the long-form guide)",
    );

    let skill = read(&skill_md);
    let method = read(&method_md);

    // --- Frontmatter: name + a discovery-surface description -----------------
    assert!(
        skill.starts_with("---"),
        "SKILL.md must open with YAML frontmatter",
    );
    assert!(
        skill.contains(&format!("name: {SKILL_NAME}")),
        "SKILL.md frontmatter must declare name: {SKILL_NAME}",
    );
    let description = frontmatter_description(&skill);
    assert!(
        description.len() >= 80,
        "description: must be a substantive discovery surface, got {} chars: {description:?}",
        description.len(),
    );

    // --- M2: the description routes on skillset-eval authoring ---------------
    // Property-level only. The discovery boundary (routes here, not to a single
    // ad-hoc eval) is exercised by plan's concrete discovery cases, per design.
    let desc = description.to_lowercase();
    assert!(
        desc.contains("skill"),
        "description must mention skills (the thing under test): {description:?}",
    );
    assert!(
        ["eval", "regression", "test", "assert"]
            .iter()
            .any(|kw| desc.contains(kw)),
        "description must mention evaluating / regression-testing: {description:?}",
    );

    // --- M1: project anatomy named in SKILL.md -------------------------------
    for token in [
        "context/skills/",
        "disclosure.md",
        "assemblies/",
        "prompts/",
        "evals/",
        "runs/",
    ] {
        assert!(
            skill.contains(token),
            "SKILL.md must name the `{token}` part of project anatomy",
        );
    }

    // --- M1: both axes named -------------------------------------------------
    for axis in ["discovery", "invocation"] {
        assert!(
            skill.to_lowercase().contains(axis),
            "SKILL.md must name the {axis} axis",
        );
    }

    // --- M1: the four-verb operator workflow ---------------------------------
    // `report` is a real subcommand the operator runs as the comparison step;
    // the guide must name it even though the top-level README still says
    // "three commands".
    let workflow_text = format!("{skill}\n{method}").to_lowercase();
    for verb in ["assemble", "run", "eval", "report"] {
        assert!(
            workflow_text.contains(verb),
            "the skill or guide must name the `{verb}` workflow step",
        );
    }

    // --- Pointers: worked example + depth ------------------------------------
    assert!(
        skill.contains("e2e/patterns-eval"),
        "SKILL.md must point to e2e/patterns-eval as the worked example",
    );
    assert!(
        skill.contains("references/method.md"),
        "SKILL.md must point to references/method.md for depth",
    );

    // --- M3: no schema duplication; the guide links out to DESIGN.md ---------
    assert!(
        links_to_design(&skill),
        "SKILL.md must link out to DESIGN.md for schema, not restate it",
    );
    assert!(
        links_to_design(&method),
        "method.md must link out to DESIGN.md for schema, not restate it",
    );

    // --- M4: falsification gate + null result stated explicitly --------------
    assert!(
        method.contains("improved > 0"),
        "method.md must state the `improved > 0` gate verbatim",
    );
    assert!(
        method.contains("regressed == 0"),
        "method.md must state the `regressed == 0` gate verbatim",
    );
    assert!(
        method.to_lowercase().contains("null result"),
        "method.md must explain how to read a deliberate null result",
    );

    // --- Fidelity rule: baseline.yaml, never invocation-baseline.yaml --------
    assert!(
        method.contains("baseline.yaml"),
        "method.md must name the built falsification arm `baseline.yaml`",
    );
    assert!(
        !method.contains("invocation-baseline.yaml"),
        "method.md must not use the README's stale `invocation-baseline.yaml` name",
    );

    // --- Every relative pointer in both files resolves -----------------------
    assert_links_resolve(&skill_md, &skill);
    assert_links_resolve(&method_md, &method);
}

/// Read the single-line `description:` field from the YAML frontmatter block.
fn frontmatter_description(skill: &str) -> String {
    let body = skill.strip_prefix("---").unwrap_or(skill);
    let end = body
        .find("\n---")
        .expect("SKILL.md frontmatter must be closed with ---");
    for line in body[..end].lines() {
        if let Some(rest) = line.trim_start().strip_prefix("description:") {
            return rest.trim().trim_matches('"').trim_matches('\'').to_string();
        }
    }
    panic!("SKILL.md frontmatter must declare a description:");
}

fn links_to_design(markdown: &str) -> bool {
    link_targets(markdown)
        .iter()
        .any(|t| Path::new(t).file_name().is_some_and(|f| f == "DESIGN.md"))
}

fn assert_links_resolve(file: &Path, markdown: &str) {
    let base = file.parent().expect("file has a parent directory");
    for target in link_targets(markdown) {
        if is_external(&target) {
            continue;
        }
        let resolved = base.join(&target);
        assert!(
            resolved.exists(),
            "{}: relative link `{target}` does not resolve ({})",
            file.display(),
            resolved.display(),
        );
    }
}
