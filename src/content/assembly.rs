//! Assembly aggregate: the recipe that `ailly assemble` expands into a set
//! of conversation files. Owns the matrix, the prefix block list, and the
//! templated conversation; round-trips through `from_yaml_str` /
//! `to_yaml_string`.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use serde::Deserialize;
use serde::Serialize;

use crate::content::conversation::BindingMap;
use crate::content::conversation::Content;
use crate::content::conversation::Conversation;
use crate::content::conversation::Message;
use crate::content::conversation::Meta;
use crate::content::conversation::ModelId;
use crate::content::conversation::Rendered;
use crate::content::conversation::Role;
use crate::content::conversation::Template;
use crate::content::conversation::TurnBody;
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

/// One entry in the assembly's prefix list.
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
    /// Read a single prefix file from OUTSIDE the project root, named relative
    /// to the project's canonical host root with `..` permitted. Mirrors
    /// `File` (one file, not a glob). Absolute `path` is rejected so
    /// committed assemblies stay portable across side-by-side checkouts.
    /// Requires a host-anchored project (`Project::open`); `open_memory` /
    /// `from_root` projects error at resolve time.
    External {
        path: String,
        #[serde(default, skip_serializing_if = "is_false")]
        cache: bool,
    },
}

/// Parse and emit errors for [`Assembly`].
#[derive(Debug, thiserror::Error)]
pub enum AssemblyError {
    #[error("failed to parse assembly YAML: {0}")]
    Parse(#[from] serde_yaml_ng::Error),
    /// A map-valued matrix axis entry lacked the required `name:` key. Names
    /// the offending axis so the operator can fix the assembly. Surfaced
    /// instead of silently serializing the map into a filename stem.
    #[error("matrix axis `{axis}` has a map value missing required `name:` key")]
    MatrixMapMissingName { axis: String },
}

/// Errors emitted when rendering templated turns or prefix blocks.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("unknown variable `{name}` in `{in_path}`")]
    UnknownVar { name: String, in_path: String },
    #[error("unterminated `{{{{` in `{in_path}`")]
    UnterminatedPlaceholder { in_path: String },
    /// An `external` prefix `path` was absolute. External paths are
    /// repo-relative with `..` permitted; an absolute path is
    /// machine-specific and defeats the portable sibling-repo use case. The
    /// file is never read.
    #[error(
        "external path `{path}` is absolute; external paths must be repo-relative (`..` permitted)"
    )]
    ExternalAbsolute { path: String },
    /// An `external` prefix block was used in a project with no host root
    /// (`open_memory` / `from_root`). `external` needs a real on-disk anchor to
    /// resolve `..` against; without one it errors instead of reading the host
    /// FS.
    #[error("external path `{path}` requires a host-anchored project (opened via Project::open)")]
    ExternalNoHostRoot { path: String },
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

/// One point in the matrix cross-product. `values` holds only scalar axis
/// values (a map axis is unwrapped to its `name` before it lands here, so
/// [`filename_for`](crate::content::repository), `substitute_template`, and
/// eval `when:` keep seeing a scalar [`BindingMap`]). `model` is the
/// per-binding override of the assembly-level default `model:`, set only when
/// a map axis carried a `model:` field; `None` for every scalar binding.
///
/// Invariant: `values` never retains a map [`serde_yaml_ng::Value`]; the map's
/// identity in `values` is the scalar `name` string. The map's `model` lives
/// here, not in `values`, so it is excluded from binding identity (eval `when:`
/// and the filename both read `values` only).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Binding {
    pub values: BindingMap,
    pub model: Option<ModelId>,
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
    ///
    /// A *map-valued* axis entry (e.g. `{ name: anthropic, model: ... }`) is
    /// projected before it lands in a binding: its `name` becomes the scalar
    /// `values` entry (so the filename and eval `when:` see a string, never a
    /// serialized map), and its optional `model` becomes the binding's
    /// per-binding [`Binding::model`] override. A *scalar* axis value is cloned
    /// into `values` unchanged.
    ///
    /// When two map axes each carry a `model:`, the later axis in [`BTreeMap`]
    /// order wins (last write into the candidate binding's `model`).
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::MatrixMapMissingName`] when a map-valued axis
    /// entry omits the required `name:` key, naming the offending axis.
    pub fn expand_matrix(&self) -> Result<Vec<Binding>, AssemblyError> {
        if self.matrix.is_empty() {
            return Ok(vec![Binding::default()]);
        }
        let mut result: Vec<Binding> = vec![Binding::default()];
        for (axis, values) in &self.matrix {
            let mut next: Vec<Binding> = Vec::with_capacity(result.len() * values.len());
            for base in &result {
                for value in values {
                    let mut new = base.clone();
                    project_axis_value(axis, value, &mut new)?;
                    next.push(new);
                }
            }
            result = next;
        }
        Ok(result)
    }

    /// Render `self` against one [`Binding`] into a full [`Conversation`]:
    /// prefix system messages followed by the rendered template turns, ending
    /// in a blank assistant slot for `ailly run` to fill.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Repository`] when a prefix block read fails or
    /// [`RenderError::UnknownVar`] / [`RenderError::UnterminatedPlaceholder`]
    /// when a template references an unknown variable or has a malformed
    /// placeholder.
    pub fn render(
        &self,
        project: &crate::content::project::Project,
        binding: &Binding,
    ) -> Result<Conversation, RenderError> {
        let mut session: Vec<Message<Rendered>> = Vec::new();
        for block in &self.prefix {
            let body = resolve_prefix_block(project, block, binding)?;
            session.push(Message {
                role: Role::System,
                body: Some(Content::Text(body)),
                cache: prefix_cache(block),
                trace: None,
                _phase: PhantomData,
            });
        }
        for turn in &self.conversation {
            session.push(turn.render(project, binding)?);
        }
        Ok(Conversation {
            meta: Meta {
                model: binding.model.clone().unwrap_or_else(|| self.model.clone()),
                debug: false,
                assembly: Some(self.name.clone()),
                binding: binding.values.clone(),
                tools: Vec::new(),
            },
            session,
        })
    }
}

fn resolve_prefix_block(
    project: &crate::content::project::Project,
    block: &PrefixBlock,
    binding: &Binding,
) -> Result<String, RenderError> {
    let context = project.context();
    match block {
        PrefixBlock::File { path, .. } => {
            let resolved = project.resolve(path, binding)?;
            Ok(context.read_file(&resolved)?)
        }
        PrefixBlock::System { path, .. }
        | PrefixBlock::Tools { path, .. }
        | PrefixBlock::Examples { path, .. } => {
            let resolved = project.resolve(path, binding)?;
            Ok(context.glob_concat(&resolved, None)?.body)
        }
        PrefixBlock::Context {
            source,
            glob,
            count,
            ..
        } => {
            let pattern = match glob {
                Some(g) => format!("{source}/{g}"),
                None => source.clone(),
            };
            let resolved = project.resolve(&pattern, binding)?;
            Ok(context.glob_concat(&resolved, *count)?.body)
        }
        PrefixBlock::External { path, .. } => {
            let resolved = project.resolve_external(path, binding)?;
            resolved
                .read_to_string()
                .map_err(|source| RepositoryError::Vfs {
                    path: resolved.as_str().to_string(),
                    source,
                })
                .map_err(RenderError::from)
        }
    }
}

/// Project one raw matrix axis value into a candidate [`Binding`].
///
/// Scalar values clone straight into `values`. A map value is unwrapped:
/// its required `name` becomes the scalar
/// `values` entry for `axis`, and its optional `model` sets the binding's
/// per-binding override.
fn project_axis_value(
    axis: &str,
    value: &serde_yaml_ng::Value,
    binding: &mut Binding,
) -> Result<(), AssemblyError> {
    match value {
        serde_yaml_ng::Value::Mapping(map) => {
            let name = map
                .get(serde_yaml_ng::Value::from("name"))
                .and_then(serde_yaml_ng::Value::as_str)
                .ok_or_else(|| AssemblyError::MatrixMapMissingName {
                    axis: axis.to_string(),
                })?;
            binding
                .values
                .insert(axis.to_string(), serde_yaml_ng::Value::from(name));
            if let Some(model) = map
                .get(serde_yaml_ng::Value::from("model"))
                .and_then(serde_yaml_ng::Value::as_str)
            {
                binding.model = Some(ModelId::from(model));
            }
        }
        scalar => {
            binding.values.insert(axis.to_string(), scalar.clone());
        }
    }
    Ok(())
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
                let text = project.context().read_file(&resolved)?;
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
        | PrefixBlock::Context { cache, .. }
        | PrefixBlock::External { cache, .. } => cache,
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
        let bindings = assembly.expand_matrix().expect("expand");
        assert_eq!(bindings.len(), 1);
        assert!(bindings[0].values.is_empty());
    }

    #[test]
    fn single_axis_matrix_expands_in_declaration_order() {
        let yaml = "name: x\nmodel: m\nmatrix:\n  case: [a, b, c]\n";
        let assembly = Assembly::from_yaml_str(yaml).expect("parses");
        let bindings = assembly.expand_matrix().expect("expand");
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
        let bindings = assembly.expand_matrix().expect("expand");
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
    fn scalar_axis_binding_has_no_model_override() {
        // A scalar axis value never sets the per-binding model override,
        // so `meta.model` falls back to the assembly default.
        let yaml = "name: x\nmodel: m\nmatrix:\n  case: [a, b]\n";
        let assembly = Assembly::from_yaml_str(yaml).expect("parses");
        let bindings = assembly.expand_matrix().expect("expand");
        assert!(
            bindings.iter().all(|b| b.model.is_none()),
            "scalar bindings carry no model override",
        );
    }

    #[test]
    fn map_axis_unwraps_name_into_values_and_model_into_override() {
        let yaml = "\
name: x
model: default-model
matrix:
  provider:
    - { name: anthropic, model: claude-opus-4-7 }
    - { name: openai,    model: gpt-5-turbo }
";
        let assembly = Assembly::from_yaml_str(yaml).expect("parses");
        let bindings = assembly.expand_matrix().expect("expand");
        assert_eq!(bindings.len(), 2);

        // `values` holds the scalar `name`, never the map.
        assert_eq!(
            bindings[0].values.get("provider").and_then(|v| v.as_str()),
            Some("anthropic"),
        );
        assert_eq!(
            bindings[1].values.get("provider").and_then(|v| v.as_str()),
            Some("openai"),
        );
        // The map's `model` becomes the per-binding override.
        assert_eq!(
            bindings[0].model.as_ref().map(AsRef::as_ref),
            Some("claude-opus-4-7"),
        );
        assert_eq!(
            bindings[1].model.as_ref().map(AsRef::as_ref),
            Some("gpt-5-turbo"),
        );
    }

    #[test]
    fn map_axis_without_model_leaves_override_unset() {
        // A map axis may carry only `name:`; the model override stays None and
        // the binding falls back to the assembly default.
        let yaml = "\
name: x
model: default-model
matrix:
  provider:
    - { name: anthropic }
";
        let assembly = Assembly::from_yaml_str(yaml).expect("parses");
        let bindings = assembly.expand_matrix().expect("expand");
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings[0].values.get("provider").and_then(|v| v.as_str()),
            Some("anthropic"),
        );
        assert!(bindings[0].model.is_none());
    }

    #[test]
    fn map_axis_missing_name_is_a_loud_error() {
        // A map value missing `name:` must fail loudly rather than serialize
        // the map into a filename stem.
        let yaml = "\
name: x
model: default-model
matrix:
  provider:
    - { model: claude-opus-4-7 }
";
        let assembly = Assembly::from_yaml_str(yaml).expect("parses");
        let err = assembly
            .expand_matrix()
            .expect_err("map value missing name must error");
        match err {
            AssemblyError::MatrixMapMissingName { axis } => assert_eq!(axis, "provider"),
            other @ AssemblyError::Parse(_) => {
                panic!("expected MatrixMapMissingName, got {other:?}")
            }
        }
    }

    #[test]
    fn external_prefix_block_round_trips_and_omits_default_cache() {
        // The new External variant round-trips through YAML unchanged, and its
        // `cache` flag honours the same `skip_serializing_if` the sibling
        // variants do: `cache: true` survives, `cache: false` is omitted on
        // emit. Guards Metric 5 (no regression) for the new variant's serde shape.
        let yaml = "\
name: x
model: m
prefix:
  - { kind: external, path: ../sib/SKILL.md, cache: true }
  - { kind: external, path: ../other/NOTES.md }
";
        let assembly = Assembly::from_yaml_str(yaml).expect("parses");
        assert!(matches!(
            assembly.prefix[0],
            PrefixBlock::External { ref path, cache: true } if path == "../sib/SKILL.md"
        ));
        assert!(matches!(
            assembly.prefix[1],
            PrefixBlock::External { cache: false, .. }
        ));

        let emitted = assembly.to_yaml_string().expect("emit");
        let reparsed = Assembly::from_yaml_str(&emitted).expect("emitted re-parses");
        assert_eq!(reparsed, assembly, "External round-trips as a fixed point");
        assert!(
            !emitted.contains("cache: false"),
            "default cache: false is omitted on serialize, got:\n{emitted}",
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

    #[test]
    fn context_block_count_truncates_glob_after_sort() {
        use std::fs;

        let tmp = tempfile::tempdir().expect("tempdir");
        let ctx_dir = tmp.path().join("ctx");
        fs::create_dir_all(&ctx_dir).expect("mkdir ctx");
        for (i, name) in ["01.md", "02.md", "03.md", "04.md", "05.md"]
            .iter()
            .enumerate()
        {
            fs::write(ctx_dir.join(name), format!("body{i}")).expect("write context file");
        }

        let project = crate::content::project::Project::open(tmp.path()).expect("open project");
        let block = PrefixBlock::Context {
            source: String::from("ctx"),
            glob: Some(String::from("*.md")),
            count: Some(2),
            cache: false,
        };
        let binding = Binding::default();
        let body = resolve_prefix_block(&project, &block, &binding).expect("resolve");

        assert_eq!(body, "body0\nbody1");
    }
}
