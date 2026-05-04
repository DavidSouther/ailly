use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use vfs::VfsPath;

use crate::content::{AssistantResponse, Conversation, ResponseUsage};
use crate::engine::{Engine, EngineEvent, EngineInput, Settings, StopReason, Usage};

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
    cancel: CancellationToken,
}

impl Generator {
    pub fn new(conversation: Conversation, engine: Arc<dyn Engine>, settings: Settings) -> Self {
        Self {
            conversation,
            engine,
            settings,
            cancel: CancellationToken::new(),
        }
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub fn run(mut self) -> Pin<Box<dyn Stream<Item = TurnEvent> + Send>> {
        Box::pin(async_stream::stream! {
            for idx in 0..self.conversation.turn_count() {
                let path = self.conversation.turn(idx).path().clone();

                yield TurnEvent::Started { path: path.clone() };

                let preamble = self.conversation.preamble_for(self.conversation.turn(idx));
                let history = self.conversation.messages_for(self.conversation.turn(idx));
                let input = EngineInput { preamble, history };

                let mut events = match self.engine.stream(input, &self.settings, path.as_str()) {
                    Ok(s) => s,
                    Err(e) => {
                        yield TurnEvent::Failed { path, error: Arc::new(e) };
                        continue;
                    }
                };

                while let Some(ev) = events.next().await {
                    match ev {
                        EngineEvent::Text(t) => {
                            yield TurnEvent::Delta { path: path.clone(), text: t };
                        }
                        EngineEvent::Final(r) => {
                            let response = AssistantResponse {
                                text: r.text.clone(),
                                model: r.model.clone(),
                                engine: Some(self.engine.name().to_string()),
                                stop_reason: Some(r.stop_reason.to_string()),
                                usage: r.usage.as_ref().map(|u| ResponseUsage {
                                    input_tokens: u.input_tokens,
                                    output_tokens: u.output_tokens,
                                }),
                            };
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
    use crate::knowledge::skills::FsSkillRepository;
    use crate::engine::Noop;
    use crate::mem_fs;

    #[tokio::test]
    async fn single_turn_emits_started_deltas_finished() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "first""#,
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap(), &FsSkillRepository::new(&fs.join("root").unwrap())).await.unwrap();
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
                _request_label: &str,
            ) -> anyhow::Result<EngineStream> {
                let response = EngineResponse {
                    text: "answer".to_string(),
                    model: Some("metadata-model".to_string()),
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
        let convo = Conversation::load(dir.clone(), &FsSkillRepository::new(&dir)).await.unwrap();
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
        let convo = Conversation::load(fs.join("root").unwrap(), &FsSkillRepository::new(&fs.join("root").unwrap())).await.unwrap();
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
