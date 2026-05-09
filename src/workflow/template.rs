use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct Context {
    values: BTreeMap<String, String>,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedPlaceholder {
    pub placeholder: String,
}

/// Single-pass `{{ name }}` substitution. Whitespace inside the braces is
/// trimmed. The first `{{ name }}` whose `name` is absent from `ctx` aborts
/// with `Err(UnresolvedPlaceholder)`. The output is not re-scanned: a value
/// that contains `{{` after substitution does not retrigger.
pub fn substitute(text: &str, ctx: &Context) -> Result<String, UnresolvedPlaceholder> {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end_rel) = text[i + 2..].find("}}") {
                let inner = &text[i + 2..i + 2 + end_rel];
                let key = inner.trim();
                match ctx.get(key) {
                    Some(value) => out.push_str(value),
                    None => {
                        return Err(UnresolvedPlaceholder {
                            placeholder: key.to_string(),
                        });
                    }
                }
                i += end_rel + 4;
                continue;
            }
        }
        // SAFETY of indexing: bytes[i] is a UTF-8 byte; we step by char width.
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok(out)
}

/// Walk every string leaf of `value`, applying `substitute` in place. Returns
/// the first unresolved placeholder encountered in walk order.
pub fn substitute_value(
    value: &mut toml::Value,
    ctx: &Context,
) -> Result<(), UnresolvedPlaceholder> {
    match value {
        toml::Value::String(s) => {
            *s = substitute(s, ctx)?;
        }
        toml::Value::Array(items) => {
            for item in items.iter_mut() {
                substitute_value(item, ctx)?;
            }
        }
        toml::Value::Table(table) => {
            for (_, v) in table.iter_mut() {
                substitute_value(v, ctx)?;
            }
        }
        toml::Value::Integer(_)
        | toml::Value::Float(_)
        | toml::Value::Boolean(_)
        | toml::Value::Datetime(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_of(pairs: &[(&str, &str)]) -> Context {
        let mut c = Context::new();
        for (k, v) in pairs {
            c.insert(*k, *v);
        }
        c
    }

    #[test]
    fn substitute_replaces_named_placeholders() {
        let ctx = ctx_of(&[("name", "x")]);
        let out = substitute("hi {{ name }}", &ctx).unwrap();
        assert_eq!(out, "hi x");
    }

    #[test]
    fn substitute_trims_whitespace_inside_braces() {
        let ctx = ctx_of(&[("name", "x")]);
        assert_eq!(substitute("{{name}}", &ctx).unwrap(), "x");
        assert_eq!(substitute("{{ name }}", &ctx).unwrap(), "x");
        assert_eq!(substitute("{{  name  }}", &ctx).unwrap(), "x");
    }

    #[test]
    fn substitute_returns_first_unresolved() {
        let ctx = ctx_of(&[("a", "1")]);
        let err = substitute("{{ a }} and {{ b }}", &ctx).unwrap_err();
        assert_eq!(err.placeholder, "b");
    }

    #[test]
    fn substitute_passes_through_text_with_no_placeholders() {
        let ctx = Context::new();
        assert_eq!(substitute("plain text", &ctx).unwrap(), "plain text");
    }

    #[test]
    fn substitute_does_not_rescan_output() {
        // If `name` resolves to text that itself looks like a placeholder,
        // the output is preserved verbatim.
        let ctx = ctx_of(&[("name", "{{ other }}")]);
        let out = substitute("{{ name }}", &ctx).unwrap();
        assert_eq!(out, "{{ other }}");
    }

    #[test]
    fn substitute_value_walks_nested_tables_and_arrays() {
        let ctx = ctx_of(&[("a", "X"), ("b", "Y")]);
        let mut value: toml::Value = toml::from_str(
            r#"
            top = "leading {{ a }} trailing"
            list = ["{{ a }}", "{{ b }}", "literal"]

            [nested]
            inside = "{{ a }}-{{ b }}"
            "#,
        )
        .unwrap();

        substitute_value(&mut value, &ctx).unwrap();

        assert_eq!(
            value.get("top").unwrap().as_str().unwrap(),
            "leading X trailing"
        );
        let list = value.get("list").unwrap().as_array().unwrap();
        assert_eq!(list[0].as_str().unwrap(), "X");
        assert_eq!(list[1].as_str().unwrap(), "Y");
        assert_eq!(list[2].as_str().unwrap(), "literal");
        assert_eq!(
            value
                .get("nested")
                .unwrap()
                .get("inside")
                .unwrap()
                .as_str()
                .unwrap(),
            "X-Y"
        );
    }

    #[test]
    fn substitute_value_returns_first_unresolved_in_walk_order() {
        let ctx = ctx_of(&[("known", "K")]);
        let mut value: toml::Value = toml::from_str(
            r#"
            outer = "{{ known }}"
            inner = "{{ missing }}"
            "#,
        )
        .unwrap();

        let err = substitute_value(&mut value, &ctx).unwrap_err();
        assert_eq!(err.placeholder, "missing");
    }
}
