use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use rig::tool::ToolDyn;
use tokio_util::sync::CancellationToken;
use vfs::VfsPath;

use crate::content::{AssistantResponse, Conversation};
use crate::engine::{
    EmptyRegistry, Engine, EngineEvent, EngineInput, Settings, StopReason, ToolRegistry, Usage,
};

#[derive(Debug, Clone)]
pub enum SkipReason {
    MetaSkip,
    AlreadyHasResponse,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum TurnEvent {
    Started {
        path: VfsPath,
    },
    Delta {
        path: VfsPath,
        text: String,
    },
    ToolCall {
        path: VfsPath,
        call: rig::message::ToolCall,
    },
    ToolResult {
        path: VfsPath,
        result: rig::message::ToolResult,
    },
    Skipped {
        path: VfsPath,
        reason: SkipReason,
    },
    Finished {
        path: VfsPath,
        response: String,
        stop_reason: StopReason,
        usage: Option<Usage>,
    },
    Failed {
        path: VfsPath,
        error: Arc<anyhow::Error>,
    },
}

pub struct Generator {
    conversation: Conversation,
    engine: Arc<dyn Engine>,
    settings: Settings,
    registry: Arc<dyn ToolRegistry>,
    cancel: CancellationToken,
}

impl Generator {
    pub fn new(conversation: Conversation, engine: Arc<dyn Engine>, settings: Settings) -> Self {
        Self {
            conversation,
            engine,
            settings,
            registry: Arc::new(EmptyRegistry),
            cancel: CancellationToken::new(),
        }
    }

    pub fn with_registry(mut self, registry: Arc<dyn ToolRegistry>) -> Self {
        self.registry = registry;
        self
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Flush any accrued text into the conversation as a metadata-less
    /// assistant entry, leaving `buffer` empty. Used to close out the
    /// preceding text run before a tool call or tool result.
    fn flush_text_buffer(&mut self, idx: usize, buffer: &mut String) {
        if buffer.is_empty() {
            return;
        }
        self.conversation.record_response(
            idx,
            AssistantResponse {
                text: std::mem::take(buffer),
                model: None,
                engine: None,
                stop_reason: None,
                usage: None,
            },
        );
    }

    fn resolve_tools_for_turn(
        &self,
        idx: usize,
        path: &VfsPath,
    ) -> anyhow::Result<Vec<Arc<dyn ToolDyn>>> {
        let mut resolved: Vec<Arc<dyn ToolDyn>> = Vec::new();
        let mut unknown: Vec<String> = Vec::new();
        for name in self.conversation.turn(idx).tool_names() {
            match self.registry.resolve(name) {
                Some(tool) => resolved.push(tool),
                None => unknown.push(name.to_string()),
            }
        }
        if unknown.is_empty() {
            return Ok(resolved);
        }
        if self.settings.strict_tools {
            Err(anyhow::anyhow!(
                "unknown tool name(s) at {}: {}",
                path.as_str(),
                unknown.join(", ")
            ))
        } else {
            log::warn!(
                "unknown tool name(s) at {}: {}",
                path.as_str(),
                unknown.join(", ")
            );
            Ok(resolved)
        }
    }

    pub fn run(mut self) -> Pin<Box<dyn Stream<Item = TurnEvent> + Send>> {
        Box::pin(async_stream::stream! {
            for idx in 0..self.conversation.turn_count() {
                let path = self.conversation.turn(idx).path().clone();

                yield TurnEvent::Started { path: path.clone() };

                let resolved = match self.resolve_tools_for_turn(idx, &path) {
                    Ok(t) => t,
                    Err(e) => {
                        yield TurnEvent::Failed { path, error: Arc::new(e) };
                        continue;
                    }
                };

                let preamble = self.conversation.preamble_for(self.conversation.turn(idx));
                let history = self.conversation.messages_for(self.conversation.turn(idx));
                let input = EngineInput { preamble, history };

                let mut events = match self.engine.stream(input, &self.settings, &resolved, path.as_str()) {
                    Ok(s) => s,
                    Err(e) => {
                        yield TurnEvent::Failed { path, error: Arc::new(e) };
                        continue;
                    }
                };

                let mut text_buffer = String::new();
                while let Some(ev) = events.next().await {
                    match ev {
                        EngineEvent::Text(t) => {
                            text_buffer.push_str(&t);
                            yield TurnEvent::Delta { path: path.clone(), text: t };
                        }
                        EngineEvent::ToolCall(call) => {
                            self.flush_text_buffer(idx, &mut text_buffer);
                            self.conversation.record_tool_call(idx, call.clone());
                            yield TurnEvent::ToolCall { path: path.clone(), call };
                        }
                        EngineEvent::ToolResult(result) => {
                            self.flush_text_buffer(idx, &mut text_buffer);
                            self.conversation.record_tool_result(idx, result.clone());
                            yield TurnEvent::ToolResult { path: path.clone(), result };
                        }
                        EngineEvent::Final(mut r) => {
                            r.engine_name = self.engine.name().into();
                            r.text = if !text_buffer.is_empty() {
                                std::mem::take(&mut text_buffer)
                            } else { r.text };
                            let response: AssistantResponse = (&r).into();
                            self.conversation.record_response(idx, response);
                            if let Err(err) = self.conversation.turn(idx).write().await {
                                yield TurnEvent::Failed {
                                    path: path.clone(),
                                    error: Arc::new(anyhow::Error::new(err)),
                                };
                                break;
                            }
                            yield TurnEvent::Finished {
                                path: path.clone(),
                                response: r.text,
                                stop_reason: r.stop_reason,
                                usage: r.usage,
                            };
                            break;
                        }
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Conversation;
    use crate::engine::Noop;
    use crate::knowledge::base::EmptyKnowledgeBase;
    use crate::mem_fs;
    use crate::project::ConversationRoot;

    #[tokio::test]
    async fn single_turn_emits_started_deltas_finished() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "first""#,
            },
        };
        let convo = Conversation::load(
            &ConversationRoot::try_from(fs.join("root").unwrap()).unwrap(),
            &EmptyKnowledgeBase,
        )
        .await
        .unwrap();
        let generator = Generator::new(convo, Arc::new(Noop::default()), Settings::default());

        let events: Vec<TurnEvent> = generator.run().collect().await;

        assert!(matches!(events[0], TurnEvent::Started { .. }));
        let TurnEvent::Started { path: started_path } = &events[0] else {
            unreachable!()
        };
        assert!(started_path.as_str().ends_with("01.toml"));

        let mut delta_concat = String::new();
        let mut finished_idx: Option<usize> = None;
        for (i, ev) in events.iter().enumerate().skip(1) {
            match ev {
                TurnEvent::Delta { text, .. } => delta_concat.push_str(text),
                TurnEvent::Finished { .. } => {
                    finished_idx = Some(i);
                    break;
                }
                other => panic!("unexpected event: {other:?}"),
            }
        }
        let finished_idx = finished_idx.expect("Finished must follow deltas");
        let TurnEvent::Finished {
            response,
            stop_reason,
            ..
        } = &events[finished_idx]
        else {
            unreachable!()
        };
        assert_eq!(*response, delta_concat);
        assert!(matches!(stop_reason, StopReason::EndTurn));
        assert!(response.contains("noop response for "));
        assert_eq!(finished_idx, events.len() - 1);
    }

    #[tokio::test]
    async fn final_event_persists_engine_model_stop_reason_and_usage_to_file() {
        use crate::engine::{EngineEvent, EngineResponse, EngineStream, Usage};

        struct MetadataEngine;
        impl Engine for MetadataEngine {
            fn name(&self) -> &'static str {
                "metadata"
            }
            fn stream(
                &self,
                _input: EngineInput,
                _settings: &Settings,
                _tools: &[Arc<dyn rig::tool::ToolDyn>],
                _request_label: &str,
            ) -> anyhow::Result<EngineStream> {
                let response = EngineResponse {
                    text: "answer".to_string(),
                    engine_name: self.name().into(),
                    model_id: "metadata-model".into(),
                    stop_reason: StopReason::MaxTokens,
                    usage: Some(Usage {
                        input_tokens: 5,
                        output_tokens: 9,
                    }),
                };
                Ok(Box::pin(futures::stream::iter(vec![EngineEvent::Final(
                    response,
                )])))
            }
        }

        let fs = mem_fs! {
            "root": {
                "01.toml": r#"prompt = "ask""#,
            },
        };
        let dir = fs.join("root").unwrap();
        let convo = Conversation::load(
            &ConversationRoot::try_from(dir.clone()).unwrap(),
            &EmptyKnowledgeBase,
        )
        .await
        .unwrap();
        let generator = Generator::new(convo, Arc::new(MetadataEngine), Settings::default());

        let _events: Vec<TurnEvent> = generator.run().collect().await;

        let path = fs.join("root/01.toml").unwrap();
        let written = path.read_to_string().unwrap();
        assert!(
            written.contains("engine = \"metadata\""),
            "missing engine field: {written}"
        );
        assert!(
            written.contains("model = \"metadata-model\""),
            "missing model field: {written}"
        );
        assert!(
            written.contains("stop_reason = \"max_tokens\""),
            "missing stop_reason field: {written}"
        );
        assert!(
            written.contains("input_tokens = 5") && written.contains("output_tokens = 9"),
            "missing usage table: {written}"
        );
    }

    #[tokio::test]
    async fn two_turn_sequence_writes_back_predecessor_response() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "first""#,
                "02.toml": r#"prompt = "second""#,
            },
        };
        let convo = Conversation::load(
            &ConversationRoot::try_from(fs.join("root").unwrap()).unwrap(),
            &EmptyKnowledgeBase,
        )
        .await
        .unwrap();
        let generator = Generator::new(convo, Arc::new(Noop::default()), Settings::default());

        let events: Vec<TurnEvent> = generator.run().collect().await;

        let mut finished_responses: Vec<String> = Vec::new();
        for ev in &events {
            if let TurnEvent::Finished { response, .. } = ev {
                finished_responses.push(response.clone());
            }
        }
        assert_eq!(finished_responses.len(), 2);
        let first_response = &finished_responses[0];
        let second_envelope = &finished_responses[1];
        assert!(
            second_envelope.contains(first_response.trim_end_matches('\n')),
            "second turn's envelope should contain the first turn's response text: {second_envelope}"
        );
    }
}
