use crate::knowledge::clarify::{KnowledgeBase, KnowledgeError};

pub struct StdinKnowledgeBase;

impl StdinKnowledgeBase {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StdinKnowledgeBase {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_answer(
    line: &str,
    default: Option<&str>,
    question: &str,
) -> Result<String, KnowledgeError> {
    let trimmed = line.trim();
    if !trimmed.is_empty() {
        return Ok(trimmed.to_string());
    }
    if let Some(d) = default {
        return Ok(d.to_string());
    }
    Err(KnowledgeError::NoAnswer {
        question: question.to_string(),
    })
}

#[async_trait::async_trait]
impl KnowledgeBase for StdinKnowledgeBase {
    async fn ask(&self, question: &str, default: Option<&str>) -> Result<String, KnowledgeError> {
        eprint!("{question}");
        if let Some(d) = default {
            eprint!(" [{d}]");
        }
        eprint!(": ");
        let _ = std::io::Write::flush(&mut std::io::stderr());

        let line = tokio::task::spawn_blocking(|| {
            let mut buf = String::new();
            std::io::stdin().read_line(&mut buf).map(|_| buf)
        })
        .await
        .map_err(|e| KnowledgeError::Other(anyhow::Error::new(e)))?
        .map_err(|e| KnowledgeError::Other(anyhow::Error::new(e)))?;

        resolve_answer(&line, default, question)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_returns_trimmed_line_when_stdin_provides_one() {
        let result = resolve_answer("  hello world  \n", None, "q");
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn empty_stdin_falls_back_to_default_when_present() {
        let result = resolve_answer("\n", Some("the-default"), "q");
        assert_eq!(result.unwrap(), "the-default");
    }

    #[test]
    fn empty_stdin_with_no_default_returns_no_answer() {
        let result = resolve_answer("   \n", None, "what");
        match result {
            Err(KnowledgeError::NoAnswer { question }) => assert_eq!(question, "what"),
            other => panic!("expected NoAnswer; got {other:?}"),
        }
    }
}
