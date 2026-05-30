//! Assembly aggregate: the recipe that `ailly assemble` expands into a set
//! of conversation files. Owns the matrix, the prefix block list, and the
//! templated conversation; round-trips through `from_yaml_str` /
//! `to_yaml_string`.
//!
//! See `docs/developer/2026-05-23-A-cli-assemble/plan.md` Step 1.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use serde::Deserialize;
use serde::Serialize;

use crate::content::conversation::BindingMap;
use crate::content::conversation::Content;
use crate::content::conversation::Message;
use crate::content::conversation::ModelId;
use crate::content::conversation::Rendered;
use crate::content::conversation::Template;
use crate::content::conversation::TurnBody;
use crate::content::repository::ContextRepository;
use crate::content::repository::RepositoryError;

/// Aggregate root for a single assembly recipe.
///
/// Invariants: `name` non-empty; matrix axis order is `BTreeMap` order;
/// `prefix` and `conversation` preserve declaration order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assembly {
    pub name: String,
    pub model: ModelId,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub matrix: BTreeMap<String, Vec<serde_yaml_ng::Value>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefix: Vec<PrefixBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversation: Vec<Message<Template>>,
}

/// One entry in the assembly's prefix list. Variants mirror DESIGN.md's
/// prefix-kind discriminator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrefixBlock {
    File {
        path: String,
        #[serde(default, skip_serializing_if = "is_false")]
        cache: bool,
    },
    System {
        path: String,
        #[serde(default, skip_serializing_if = "is_false")]
        cache: bool,
    },
    Tools {
        path: String,
        #[serde(default, skip_serializing_if = "is_false")]
        cache: bool,
    },
    Examples {
        path: String,
        #[serde(default, skip_serializing_if = "is_false")]
        cache: bool,
    },
    Context {
        source: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        glob: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<usize>,
        #[serde(default, skip_serializing_if = "is_false")]
        cache: bool,
    },
}

/// Parse and emit errors for [`Assembly`].
#[derive(Debug, thiserror::Error)]
pub enum AssemblyError {
    #[error("failed to parse assembly YAML: {0}")]
    Parse(#[from] serde_yaml_ng::Error),
}

/// Errors emitted when rendering templated turns or prefix blocks.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("unknown variable `{name}` in `{in_path}`")]
    UnknownVar { name: String, in_path: String },
    #[error("unterminated `{{{{` in `{in_path}`")]
    UnterminatedPlaceholder { in_path: String },
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

/// One point in the matrix cross-product. Values reuse [`BindingMap`] so the
/// rest of the crate sees a single binding type.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Binding {
    pub values: BindingMap,
}

impl Assembly {
    /// Parse an assembly YAML document.
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::Parse`] when the YAML is malformed or the
    /// document does not match the [`Assembly`] schema (including unknown
    /// prefix-block kinds, which surface through serde's default unknown-
    /// variant error).
    pub fn from_yaml_str(input: &str) -> Result<Self, AssemblyError> {
        serde_yaml_ng::from_str(input).map_err(AssemblyError::from)
    }

    /// Serialize this assembly back to YAML.
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::Parse`] when the YAML emitter rejects the
    /// underlying value.
    pub fn to_yaml_string(&self) -> Result<String, AssemblyError> {
        serde_yaml_ng::to_string(self).map_err(AssemblyError::from)
    }

    /// Expand `self.matrix` into the cross-product of axis values.
    ///
    /// Axes iterate in [`BTreeMap`] order; per-axis values iterate in
    /// declaration order. An empty matrix yields a single [`Binding`] with an
    /// empty values map so callers can treat the no-matrix case uniformly.
    #[must_use]
    pub fn expand_matrix(&self) -> Vec<Binding> {
        if self.matrix.is_empty() {
            return vec![Binding::default()];
        }
        let mut result: Vec<BindingMap> = vec![BindingMap::new()];
        for (axis, values) in &self.matrix {
            let mut next: Vec<BindingMap> = Vec::with_capacity(result.len() * values.len());
            for base in &result {
                for value in values {
                    let mut new = base.clone();
                    new.insert(axis.clone(), value.clone());
                    next.push(new);
                }
            }
            result = next;
        }
        result
            .into_iter()
            .map(|values| Binding { values })
            .collect()
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if requires &T"
)]
fn is_false(b: &bool) -> bool {
    !*b
}

impl Message<Template> {
    /// Resolve `self` against a [`Binding`] over a [`Project`],
    /// producing the corresponding `Message<Rendered>`. A `User` turn
    /// resolves the templated path via [`Project::resolve`] and reads it
    /// through [`Project::context`]; an `Assistant` turn emits
    /// `body: None`.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::UnknownVar`] if the path references a variable
    /// not bound by `binding`, [`RenderError::UnterminatedPlaceholder`] if
    /// the path has an unclosed `{{`, or [`RenderError::Repository`] if the
    /// underlying file read fails.
    pub fn render(
        &self,
        project: &crate::content::project::Project,
        binding: &Binding,
    ) -> Result<Message<Rendered>, RenderError> {
        let body = match &self.body {
            TurnBody::UserPath { path } => {
                let resolved = project.resolve(path, binding)?;
                let text = ContextRepository::read_file(&project.context(), resolved.relative())?;
                Some(Content::Text(text))
            }
            TurnBody::AssistantBlank => None,
        };
        Ok(Message {
            role: self.role,
            body,
            cache: self.cache,
            trace: None,
            _phase: PhantomData,
        })
    }
}

/// Cache flag carried by every [`PrefixBlock`] variant.
#[must_use]
pub fn prefix_cache(block: &PrefixBlock) -> bool {
    match *block {
        PrefixBlock::File { cache, .. }
        | PrefixBlock::System { cache, .. }
        | PrefixBlock::Tools { cache, .. }
        | PrefixBlock::Examples { cache, .. }
        | PrefixBlock::Context { cache, .. } => cache,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::conversation::Role;
    use crate::content::conversation::TurnBody;

    const CLAIM_HANDLER_FIXTURE: &str = "\
name: claim-handler
model: claude-opus-4-7

matrix:
  case: [default, missing-fields, ambiguous, over-limit]

prefix:
  - { kind: file,     path: ./AGENTS.md,                          cache: true }
  - { kind: system,   path: context/system/*.md,                  cache: true }
  - { kind: tools,    path: context/tools/*.json,                 cache: true }
  - { kind: examples, path: context/examples/classification/*.md }

conversation:
  - { role: user, path: \"prompts/{{ case }}.md\" }
  - { role: assistant }
";

    #[test]
    fn parses_canonical_insurance_claim_fixture() {
        let assembly =
            Assembly::from_yaml_str(CLAIM_HANDLER_FIXTURE).expect("canonical fixture parses");

        assert_eq!(assembly.name, "claim-handler");
        assert_eq!(assembly.model.as_ref(), "claude-opus-4-7");
        assert_eq!(assembly.matrix.len(), 1);
        let axis = assembly.matrix.get("case").expect("case axis present");
        assert_eq!(axis.len(), 4);

        assert_eq!(assembly.prefix.len(), 4);
        assert!(matches!(
            assembly.prefix[0],
            PrefixBlock::File { ref path, cache: true } if path == "./AGENTS.md"
        ));
        assert!(matches!(
            assembly.prefix[3],
            PrefixBlock::Examples { cache: false, .. }
        ));

        assert_eq!(assembly.conversation.len(), 2);
        assert!(matches!(assembly.conversation[0].role, Role::User));
        match &assembly.conversation[0].body {
            TurnBody::UserPath { path } => assert_eq!(path, "prompts/{{ case }}.md"),
            TurnBody::AssistantBlank => panic!("first turn is a user turn"),
        }
        assert!(matches!(assembly.conversation[1].role, Role::Assistant));
        assert!(matches!(
            assembly.conversation[1].body,
            TurnBody::AssistantBlank
        ));
    }

    #[test]
    fn round_trip_through_yaml_is_a_fixed_point() {
        let assembly =
            Assembly::from_yaml_str(CLAIM_HANDLER_FIXTURE).expect("canonical fixture parses");
        let emitted = assembly.to_yaml_string().expect("emit");
        let reparsed = Assembly::from_yaml_str(&emitted).expect("emitted re-parses");
        assert_eq!(reparsed, assembly);
    }

    #[test]
    fn empty_matrix_yields_single_empty_binding() {
        let yaml = "name: x\nmodel: m\n";
        let assembly = Assembly::from_yaml_str(yaml).expect("parses");
        let bindings = assembly.expand_matrix();
        assert_eq!(bindings.len(), 1);
        assert!(bindings[0].values.is_empty());
    }

    #[test]
    fn single_axis_matrix_expands_in_declaration_order() {
        let yaml = "name: x\nmodel: m\nmatrix:\n  case: [a, b, c]\n";
        let assembly = Assembly::from_yaml_str(yaml).expect("parses");
        let bindings = assembly.expand_matrix();
        assert_eq!(bindings.len(), 3);
        let cases: Vec<String> = bindings
            .iter()
            .map(|b| {
                b.values
                    .get("case")
                    .and_then(serde_yaml_ng::Value::as_str)
                    .expect("case value")
                    .to_owned()
            })
            .collect();
        assert_eq!(cases, vec!["a", "b", "c"]);
    }

    #[test]
    fn two_axis_matrix_yields_cross_product() {
        let yaml =
            "name: x\nmodel: m\nmatrix:\n  alpha: [a1, a2]\n  beta: [b1, b2, b3]\n".to_string();
        let assembly = Assembly::from_yaml_str(&yaml).expect("parses");
        let bindings = assembly.expand_matrix();
        assert_eq!(bindings.len(), 6, "2 x 3 cross-product");

        let pairs: Vec<(String, String)> = bindings
            .iter()
            .map(|b| {
                let a = b
                    .values
                    .get("alpha")
                    .and_then(serde_yaml_ng::Value::as_str)
                    .expect("alpha")
                    .to_owned();
                let bv = b
                    .values
                    .get("beta")
                    .and_then(serde_yaml_ng::Value::as_str)
                    .expect("beta")
                    .to_owned();
                (a, bv)
            })
            .collect();
        // BTreeMap order puts alpha before beta; alpha is the outer axis.
        assert_eq!(
            pairs,
            vec![
                ("a1".into(), "b1".into()),
                ("a1".into(), "b2".into()),
                ("a1".into(), "b3".into()),
                ("a2".into(), "b1".into()),
                ("a2".into(), "b2".into()),
                ("a2".into(), "b3".into()),
            ]
        );
    }

    #[test]
    fn unknown_prefix_kind_surfaces_serde_error() {
        let yaml = "name: x\nmodel: m\nprefix:\n  - { kind: seed, value: hello }\n";
        let err = Assembly::from_yaml_str(yaml).expect_err("unknown kind rejected");
        let msg = format!("{err}");
        assert!(msg.contains("seed"), "got {msg}");
        assert!(msg.contains("context"), "got {msg}");
    }

    fn binding_with_case(case: &str) -> Binding {
        let mut b = Binding::default();
        b.values
            .insert(String::from("case"), serde_yaml_ng::Value::from(case));
        b
    }

    fn seed_prompt(project: &crate::content::project::Project, rel: &str, body: &str) {
        use std::io::Write;
        if let Some((parent, _)) = rel.rsplit_once('/') {
            project
                .root()
                .join(parent)
                .expect("join parent")
                .create_dir_all()
                .expect("mkdir parent");
        }
        let path = project.root().join(rel).expect("join rel");
        let mut f = path.create_file().expect("create file");
        f.write_all(body.as_bytes()).expect("write");
    }

    #[test]
    fn message_template_render_resolves_path_against_binding_and_reads_body() {
        let project = crate::content::project::Project::open_memory();
        seed_prompt(&project, "prompts/missing-fields.md", "known body");

        let template: Message<Template> = Message {
            role: Role::User,
            body: TurnBody::UserPath {
                path: String::from("prompts/{{ case }}.md"),
            },
            cache: false,
            trace: None,
            _phase: PhantomData,
        };
        let binding = binding_with_case("missing-fields");

        let rendered: Message<Rendered> = template.render(&project, &binding).expect("render");

        assert!(matches!(rendered.role, Role::User));
        match rendered.body {
            Some(Content::Text(ref t)) => assert_eq!(t, "known body"),
            other => panic!("expected Content::Text(\"known body\"), got {other:?}"),
        }
        assert!(!rendered.cache);
        assert!(rendered.trace.is_none());
    }

    #[test]
    fn message_template_render_returns_unknown_var_before_reading_file() {
        // No prompt files seeded — if render attempted a read after a
        // successful substitution, the missing file would surface as a
        // `RenderError::Repository`. The expected behaviour is the unknown-
        // variable error to fire first.
        let project = crate::content::project::Project::open_memory();

        let template: Message<Template> = Message {
            role: Role::User,
            body: TurnBody::UserPath {
                path: String::from("prompts/{{ unbound }}.md"),
            },
            cache: false,
            trace: None,
            _phase: PhantomData,
        };
        let binding = Binding::default();

        let err = template
            .render(&project, &binding)
            .expect_err("unbound variable should fail before read");
        match err {
            RenderError::UnknownVar { name, in_path } => {
                assert_eq!(name, "unbound");
                assert_eq!(in_path, "prompts/{{ unbound }}.md");
            }
            other => panic!("expected RenderError::UnknownVar, got {other:?}"),
        }
    }
}
