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
///
/// `Seed` and `Retrieval` are intentionally absent: an assembly carrying
/// either kind returns [`AssemblyError::UnsupportedKind`] so the failure is
/// loud, not silent.
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
    #[error("unsupported prefix kind: {kind}")]
    UnsupportedKind { kind: &'static str },
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
    /// Returns [`AssemblyError::UnsupportedKind`] when any prefix block uses
    /// `kind: seed` or `kind: retrieval`; otherwise propagates serde errors
    /// through [`AssemblyError::Parse`].
    pub fn from_yaml_str(input: &str) -> Result<Self, AssemblyError> {
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(input)?;
        if let Some(prefix) = value
            .get("prefix")
            .and_then(serde_yaml_ng::Value::as_sequence)
        {
            for block in prefix {
                if let Some(kind) = block.get("kind").and_then(serde_yaml_ng::Value::as_str) {
                    match kind {
                        "seed" => return Err(AssemblyError::UnsupportedKind { kind: "seed" }),
                        "retrieval" => {
                            return Err(AssemblyError::UnsupportedKind { kind: "retrieval" });
                        }
                        _ => {}
                    }
                }
            }
        }
        let assembly: Self = serde_yaml_ng::from_value(value)?;
        Ok(assembly)
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
    /// Resolve `self` against a [`Binding`] and a [`ContextRepository`],
    /// producing the corresponding `Message<Rendered>`. A `User` turn reads
    /// the templated path from `ctx` and emits `Content::Text`; an
    /// `Assistant` turn emits `body: None`.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::UnknownVar`] if the path references a variable
    /// not bound by `binding`, [`RenderError::UnterminatedPlaceholder`] if
    /// the path has an unclosed `{{`, or [`RenderError::Repository`] if the
    /// underlying file read fails.
    pub fn render(
        &self,
        binding: &Binding,
        ctx: &dyn ContextRepository,
    ) -> Result<Message<Rendered>, RenderError> {
        let body = match &self.body {
            TurnBody::UserPath { path } => {
                let resolved = substitute(path, binding)?;
                let text = ctx.read_file(&resolved)?;
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

/// Replace every `{{ name }}` placeholder in `template` with the
/// YAML-stringified value bound to `name`. Whitespace inside the braces is
/// tolerated. No conditionals, escaping, or nesting.
///
/// # Errors
///
/// Returns [`RenderError::UnknownVar`] when `binding` does not bind a name
/// referenced by the template, or [`RenderError::UnterminatedPlaceholder`]
/// when an open `{{` has no matching `}}`.
pub fn substitute(template: &str, binding: &Binding) -> Result<String, RenderError> {
    let mut result = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        let end = after_open
            .find("}}")
            .ok_or_else(|| RenderError::UnterminatedPlaceholder {
                in_path: template.to_string(),
            })?;
        let name = after_open[..end].trim();
        let value = binding
            .values
            .get(name)
            .ok_or_else(|| RenderError::UnknownVar {
                name: name.to_string(),
                in_path: template.to_string(),
            })?;
        result.push_str(&stringify_value(value));
        rest = &after_open[end + 2..];
    }
    result.push_str(rest);
    Ok(result)
}

fn stringify_value(value: &serde_yaml_ng::Value) -> String {
    match value {
        serde_yaml_ng::Value::String(s) => s.clone(),
        serde_yaml_ng::Value::Bool(b) => b.to_string(),
        serde_yaml_ng::Value::Number(n) => n.to_string(),
        serde_yaml_ng::Value::Null => String::new(),
        other => {
            let raw = serde_yaml_ng::to_string(other).unwrap_or_default();
            raw.trim().trim_matches('"').trim_matches('\'').to_string()
        }
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
    fn seed_prefix_kind_returns_unsupported_kind() {
        let yaml = "name: x\nmodel: m\nprefix:\n  - { kind: seed, value: hello }\n";
        let err = Assembly::from_yaml_str(yaml).expect_err("seed rejected");
        assert!(
            matches!(err, AssemblyError::UnsupportedKind { kind: "seed" }),
            "got {err:?}"
        );
    }

    #[test]
    fn retrieval_prefix_kind_returns_unsupported_kind() {
        let yaml = "name: x\nmodel: m\nprefix:\n  - { kind: retrieval, source: docs/, query: q }\n";
        let err = Assembly::from_yaml_str(yaml).expect_err("retrieval rejected");
        assert!(
            matches!(err, AssemblyError::UnsupportedKind { kind: "retrieval" }),
            "got {err:?}"
        );
    }

    fn binding_with_case(case: &str) -> Binding {
        let mut b = Binding::default();
        b.values
            .insert(String::from("case"), serde_yaml_ng::Value::from(case));
        b
    }

    #[test]
    fn substitute_replaces_a_simple_placeholder() {
        let binding = binding_with_case("missing-fields");
        let resolved = substitute("prompts/{{ case }}.md", &binding).expect("substitute");
        assert_eq!(resolved, "prompts/missing-fields.md");
    }

    #[test]
    fn substitute_tolerates_whitespace_inside_braces() {
        let binding = binding_with_case("a");
        let resolved = substitute("x/{{case}}.y", &binding).expect("substitute");
        assert_eq!(resolved, "x/a.y");
    }

    #[test]
    fn substitute_unknown_variable_returns_unknown_var() {
        let binding = Binding::default();
        let err = substitute("prompts/{{ missing }}.md", &binding).expect_err("missing var");
        match err {
            RenderError::UnknownVar { name, in_path } => {
                assert_eq!(name, "missing");
                assert_eq!(in_path, "prompts/{{ missing }}.md");
            }
            other => panic!("expected UnknownVar, got {other:?}"),
        }
    }

    #[test]
    fn substitute_unterminated_placeholder_is_caught() {
        let binding = Binding::default();
        let err = substitute("a/{{ case", &binding).expect_err("unterminated");
        assert!(
            matches!(err, RenderError::UnterminatedPlaceholder { .. }),
            "got {err:?}"
        );
    }

    /// Tiny test double for [`ContextRepository`]: returns `body` when
    /// `read_file` is called with `expected_path`. Any other call panics.
    struct StubContextRepository {
        expected_path: String,
        body: String,
    }

    impl ContextRepository for StubContextRepository {
        fn read_file(&self, path: &str) -> Result<String, RepositoryError> {
            assert_eq!(path, self.expected_path, "unexpected read_file path");
            Ok(self.body.clone())
        }

        fn glob_concat(
            &self,
            _pattern: &str,
            _limit: Option<usize>,
        ) -> Result<crate::content::repository::GlobResult, RepositoryError> {
            panic!("glob_concat should not be called by these tests");
        }
    }

    /// Test double whose `read_file` panics; used to prove that
    /// `Message<Template>::render` fails on an unknown variable before any
    /// filesystem read is attempted.
    struct PanicOnReadContextRepository;

    impl ContextRepository for PanicOnReadContextRepository {
        fn read_file(&self, _path: &str) -> Result<String, RepositoryError> {
            panic!("read_file should not be called when the path has an unbound variable");
        }

        fn glob_concat(
            &self,
            _pattern: &str,
            _limit: Option<usize>,
        ) -> Result<crate::content::repository::GlobResult, RepositoryError> {
            panic!("glob_concat should not be called by these tests");
        }
    }

    #[test]
    fn message_template_render_resolves_path_against_binding_and_reads_body() {
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
        let ctx = StubContextRepository {
            expected_path: String::from("prompts/missing-fields.md"),
            body: String::from("known body"),
        };

        let rendered: Message<Rendered> = template.render(&binding, &ctx).expect("render");

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
        let ctx = PanicOnReadContextRepository;

        let err = template
            .render(&binding, &ctx)
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
