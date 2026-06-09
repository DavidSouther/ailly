//! Feature test: `ailly run` writes multiline assistant responses as readable
//! YAML literal block scalars, not escaped single-line double-quoted strings.
//!
//! User story: an operator runs a conversation whose blank assistant turn the
//! model fills with a multiline markdown response. The response arrives with a
//! stray trailing space and CRLF line endings (the realistic case that flips
//! libyaml to quoted output). The written conversation file stores the body as
//! a `content: |` block whose newlines are literal, the run write-path having
//! normalized CRLF -> LF and stripped trailing whitespace so the block is
//! valid. Interior content is preserved and the file round-trips.
//!
//! Red until `fill_blank_assistant` normalizes content on write.

use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::Conversation;
use ailly_two::engine::engine::NoopEngine;

const BLANK_ASSISTANT_FIXTURE: &str = "\
---
model: noop
---
role: user
content: Summarize the project.
---
role: assistant
";

/// Multiline markdown reply carrying the two libyaml block-refusal triggers the
/// run write-path is meant to normalize: CRLF line endings and a trailing space
/// before a newline.
const MULTILINE_REPLY: &str = "# Summary\r\n\r\nThis line ends in a stray space \nand this line is clean.\n\n- alpha\n- beta\n";

#[tokio::test]
async fn run_writes_multiline_assistant_as_block_scalar() {
    let mut conv = Conversation::from_yaml_str(BLANK_ASSISTANT_FIXTURE).expect("fixture parses");
    let engine = NoopEngine::from_replies([MULTILINE_REPLY]);

    conv.run(&engine)
        .await
        .expect("run fills the blank assistant");

    let emitted = conv.to_yaml_string().expect("emits");

    // The assistant body is a literal block scalar, not a quoted scalar.
    assert!(
        emitted.contains("content: |"),
        "expected a `content: |` block scalar, got:\n{emitted}"
    );
    // No escaped newline sequence anywhere: nothing fell back to double-quoted.
    assert!(
        !emitted.contains("\\n"),
        "expected no escaped \\n sequences, got:\n{emitted}"
    );
    // The block lines appear literally, indented under `content:`.
    assert!(
        emitted.contains("\n  # Summary\n"),
        "expected the heading as a literal block line, got:\n{emitted}"
    );
    assert!(
        emitted.contains("\n  - alpha\n"),
        "expected the list item as a literal block line, got:\n{emitted}"
    );

    // The written file round-trips back to the same conversation.
    let reparsed = Conversation::from_yaml_str(&emitted).expect("emitted re-parses");
    assert_eq!(reparsed, conv, "block-scalar emission round-trips");

    // The stored body was normalized on write: no CRLF, no space-before-newline.
    let body = conv
        .session
        .last()
        .and_then(|m| m.body.as_ref())
        .expect("assistant turn is filled");
    let Content::Text(text) = body else {
        panic!("expected a text body, got {body:?}");
    };
    assert!(!text.contains('\r'), "CRLF was normalized to LF");
    assert!(
        !text.contains(" \n"),
        "trailing space before newline was stripped"
    );
    assert_eq!(
        text,
        "# Summary\n\nThis line ends in a stray space\nand this line is clean.\n\n- alpha\n- beta\n",
        "interior content preserved, only trailing whitespace and CRLF normalized"
    );
}

/// Documents the accepted boundary: a response containing an interior tab is
/// the one realistic case libyaml still quotes (it refuses block style for any
/// tab). The tab is preserved verbatim and the file round-trips; only the
/// readability win is forgone, never data. Revisit with serde-saphyr.
#[tokio::test]
async fn run_preserves_tabs_losslessly_even_when_quoted() {
    let mut conv = Conversation::from_yaml_str(BLANK_ASSISTANT_FIXTURE).expect("fixture parses");
    let engine = NoopEngine::from_replies(["before\n\tindented-with-tab\nafter\n"]);

    conv.run(&engine)
        .await
        .expect("run fills the blank assistant");
    let emitted = conv.to_yaml_string().expect("emits");

    let reparsed = Conversation::from_yaml_str(&emitted).expect("emitted re-parses");
    assert_eq!(reparsed, conv, "tabbed content round-trips losslessly");

    let body = conv
        .session
        .last()
        .and_then(|m| m.body.as_ref())
        .expect("assistant turn is filled");
    let Content::Text(text) = body else {
        panic!("expected a text body, got {body:?}");
    };
    assert!(
        text.contains('\t'),
        "the interior tab is preserved verbatim"
    );
}
