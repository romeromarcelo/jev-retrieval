//! Golden wire-format tests.
//!
//! The bodies below were captured from the official TypeSafe Python SDK's
//! debug logs — they are the contract the hand-rolled serde mirror must honor.
//! Equality is asserted on JSON values, not strings: key order is irrelevant on
//! the wire, and BTreeMap serialization order differs from the SDK's dict order.

use jevr::jev::schemas::{Answer, JevRequest, JevResponse, NoulCriteria, Question};
use serde_json::{json, Value};

/// SDK-captured request body. The "s" (score) question is part of the capture,
/// but the pipeline never asks Score questions (KISS) — request tests compare
/// against this body with "s" removed; response tests use the full capture.
const GOLDEN_REQUEST: &str = r#"{"state":{"code":"def f(): pass"},"model":"jev-1.13.0","questions":{"m":{"type":"noul","instructions":"Is `code` a function?","criteria":{"true":"defines a function","false":"no function"}},"k":{"type":"choice","instructions":"Language of `code`?","criteria":{"py":"Python","rs":null}},"s":{"type":"score","instructions":"Complexity of `code`?","criteria":["trivial","moderate","complex"]}}}"#;

const GOLDEN_RESPONSE: &str = r#"{"model":"jev-1.13.0","answers":{"m":{"type":"noul","noul":0.9},"k":{"type":"choice","choice":"py","confidence":1.0,"probabilities":{"rs":0.0,"py":1.0}},"s":{"type":"score","score":0.0,"confidence":1.0,"legend":{"0":"trivial","1":"moderate","2":"complex"},"probabilities":{"0":1.0,"1":0.0,"2":0.0}}},"usage":{"input_tokens":386,"output_tokens":61}}"#;

fn golden_request() -> Value {
    serde_json::from_str(GOLDEN_REQUEST).unwrap()
}

#[test]
fn noul_question_matches_captured_body() {
    let question = Question::Noul {
        instructions: "Is `code` a function?".into(),
        criteria: Some(NoulCriteria {
            true_means: "defines a function".into(),
            false_means: "no function".into(),
        }),
    };
    assert_eq!(
        serde_json::to_value(&question).unwrap(),
        golden_request()["questions"]["m"]
    );
}

#[test]
fn noul_question_without_criteria_omits_the_field() {
    let question = Question::Noul {
        instructions: "Is `code` a function?".into(),
        criteria: None,
    };
    let value = serde_json::to_value(&question).unwrap();
    assert!(value.get("criteria").is_none());
}

#[test]
fn choice_question_matches_captured_body() {
    let question = Question::Choice {
        instructions: "Language of `code`?".into(),
        criteria: [
            ("py".to_string(), Some("Python".to_string())),
            ("rs".to_string(), None),
        ]
        .into(),
    };
    assert_eq!(
        serde_json::to_value(&question).unwrap(),
        golden_request()["questions"]["k"]
    );
}

#[test]
fn full_request_matches_captured_body() {
    let request = JevRequest {
        state: json!({"code": "def f(): pass"}),
        model: "jev-1.13.0".into(),
        questions: [
            (
                "m".to_string(),
                Question::Noul {
                    instructions: "Is `code` a function?".into(),
                    criteria: Some(NoulCriteria {
                        true_means: "defines a function".into(),
                        false_means: "no function".into(),
                    }),
                },
            ),
            (
                "k".to_string(),
                Question::Choice {
                    instructions: "Language of `code`?".into(),
                    criteria: [
                        ("py".to_string(), Some("Python".to_string())),
                        ("rs".to_string(), None),
                    ]
                    .into(),
                },
            ),
        ]
        .into(),
    };
    let mut expected = golden_request();
    expected["questions"].as_object_mut().unwrap().remove("s");
    assert_eq!(serde_json::to_value(&request).unwrap(), expected);
}

#[test]
fn full_response_parses_including_score_answer() {
    let response: JevResponse = serde_json::from_str(GOLDEN_RESPONSE).unwrap();
    assert_eq!(response.model, "jev-1.13.0");
    assert_eq!(response.usage.input_tokens, 386);
    assert_eq!(response.usage.output_tokens, 61);
    assert_eq!(response.answers.len(), 3);

    let Answer::Noul { noul } = &response.answers["m"] else {
        panic!("expected noul answer");
    };
    assert_eq!(*noul, 0.9);

    let Answer::Choice {
        choice,
        confidence,
        probabilities,
    } = &response.answers["k"]
    else {
        panic!("expected choice answer");
    };
    assert_eq!(choice, "py");
    assert_eq!(*confidence, Some(1.0));
    assert_eq!(probabilities["py"], 1.0);
    assert_eq!(probabilities["rs"], 0.0);

    let Answer::Score {
        score,
        confidence,
        probabilities,
    } = &response.answers["s"]
    else {
        panic!("expected score answer");
    };
    assert_eq!(*score, 0.0);
    assert_eq!(*confidence, Some(1.0));
    assert_eq!(probabilities["0"], 1.0);
}

#[test]
fn doc_questions_serialize_with_text_paths_and_coverage_criteria() {
    // Pins the document-mode question bodies (measured separation: 0.97–0.98
    // relevant vs 0.01–0.06 incidental/unrelated prose). Any change to
    // phrasing or criteria alters cache keys and calibration — re-run the
    // BEIR gates in benchmarks/ before changing.
    use jevr::jev::questions::{doc_file_membership, doc_window_membership};

    let expected_window = json!({
        "type": "noul",
        "instructions": "Is the text in `windows[3].text` from the document at `file` part \
                         of the material that covers `concept`, either addressing it directly \
                         or serving as its dedicated supporting content (definitions, \
                         examples, configuration, references)?",
        "criteria": {
            "true": "Covers the concept, or is a definition/example/configuration/reference \
                     that exists specifically to support it",
            "false": "Merely mentions it in passing, or touches the concept incidentally \
                      while covering something else"
        }
    });
    assert_eq!(
        serde_json::to_value(doc_window_membership(3)).unwrap(),
        expected_window
    );

    let file_question = serde_json::to_value(doc_file_membership()).unwrap();
    assert_eq!(
        file_question["instructions"],
        json!(
            "Considering all windows shown, is the document at `file` part of the \
             material that covers `concept`, including its dedicated supporting \
             content?"
        )
    );
    assert_eq!(file_question["criteria"], expected_window["criteria"]);
}

#[test]
fn listwise_rank_keeps_the_code_phrasing_byte_identical_without_docs() {
    use jevr::jev::questions::listwise_rank;

    let code = serde_json::to_value(listwise_rank(["f0".to_string()].into_iter(), false)).unwrap();
    assert_eq!(
        code["instructions"],
        json!("Which file in `files` most likely implements what `concept` describes?")
    );
    let doc = serde_json::to_value(listwise_rank(["f0".to_string()].into_iter(), true)).unwrap();
    assert_eq!(
        doc["instructions"],
        json!("Which file in `files` most likely contains what `concept` describes?")
    );
}

#[test]
fn noul_answers_never_carry_confidence() {
    // A noul answer with an unexpected confidence field must still parse (serde
    // ignores unknown fields), and a bare one must not require it.
    let bare: Answer = serde_json::from_str(r#"{"type":"noul","noul":0.42}"#).unwrap();
    let Answer::Noul { noul } = bare else {
        panic!("expected noul answer");
    };
    assert_eq!(noul, 0.42);
}

#[test]
fn cached_response_is_served_without_network() {
    use jevr::jev::client::JevClient;

    let dir = std::env::temp_dir().join(format!("jevr-cache-test-{}", std::process::id()));
    let client = JevClient::new("invalid-key".into(), Some(dir.clone()));
    let request = JevRequest {
        state: json!({"code": "def f(): pass"}),
        model: "jev-1.13.0".into(),
        questions: [(
            "m".to_string(),
            Question::Noul {
                instructions: "Is `code` a function?".into(),
                criteria: None,
            },
        )]
        .into(),
    };
    let body_json = serde_json::to_string(&request).unwrap();
    let path = client.cache_path(&body_json).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, GOLDEN_RESPONSE).unwrap();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // An invalid key means any network attempt fails fast with a non-retryable
    // 401 — success here proves the answer came from the cache file.
    let response = runtime.block_on(client.system_one(&request)).unwrap();
    assert_eq!(response.model, "jev-1.13.0");
    assert_eq!(response.usage.input_tokens, 386);
    std::fs::remove_dir_all(&dir).unwrap();
}
