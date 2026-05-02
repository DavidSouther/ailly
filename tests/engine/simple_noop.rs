//! Feature test for the engine module slice.
//!
//! User story: a developer loads a `Conversation` of two sibling turns,
//! hands it to a `Generator` driving a `Noop` engine, and observes a
//! deterministic stream of `TurnEvent`s for each turn.

use std::sync::Arc;

use futures::StreamExt;

use ailly::content::Conversation;
use ailly::engine::{Generator, Noop, Settings, StopReason, TurnEvent};
use ailly::mem_fs;

#[tokio::test]
async fn generator_runs_two_turn_sequence_through_noop() {
    let fs = mem_fs! {
        "root": {
            ".aillyrc.toml": r#"system = "you are helpful""#,
            "01.toml": r#"prompt = "first turn""#,
            "02.toml": r#"prompt = "second turn""#,
        },
    };
    let conversation = Conversation::load(fs.join("root").unwrap())
        .await
        .expect("load conversation");

    let engine = Arc::new(Noop::default());
    let generator = Generator::new(conversation, engine, Settings::default());

    let events: Vec<TurnEvent> = generator.run().collect().await;

    let mut idx = 0;
    for turn_filename in ["01.toml", "02.toml"] {
        match &events[idx] {
            TurnEvent::Started { path } => {
                assert!(
                    path.as_str().ends_with(turn_filename),
                    "expected Started for {turn_filename}, got {}",
                    path.as_str()
                );
            }
            other => panic!("expected Started for {turn_filename}, got {other:?}"),
        }
        idx += 1;

        let mut delta_concat = String::new();
        while let Some(TurnEvent::Delta { path, text }) = events.get(idx) {
            assert!(
                path.as_str().ends_with(turn_filename),
                "delta path {} does not match {turn_filename}",
                path.as_str()
            );
            delta_concat.push_str(text);
            idx += 1;
        }
        assert!(
            !delta_concat.is_empty(),
            "expected at least one Delta for {turn_filename}"
        );

        match &events[idx] {
            TurnEvent::Finished {
                path,
                response,
                stop_reason,
                ..
            } => {
                assert!(
                    path.as_str().ends_with(turn_filename),
                    "finished path {} does not match {turn_filename}",
                    path.as_str()
                );
                assert_eq!(*response, delta_concat, "response equals concatenated deltas");
                assert!(matches!(stop_reason, StopReason::EndTurn));
                assert!(
                    response.contains(&format!("noop response for {}", path.as_str())),
                    "response should embed the deterministic Noop envelope keyed by path; got {response}"
                );
            }
            other => panic!("expected Finished for {turn_filename}, got {other:?}"),
        }
        idx += 1;
    }

    assert_eq!(
        idx,
        events.len(),
        "no events expected after the last Finished"
    );

    for ev in &events {
        match ev {
            TurnEvent::Failed { .. } | TurnEvent::Skipped { .. } => {
                panic!("unexpected event in happy path: {ev:?}");
            }
            _ => {}
        }
    }
}
