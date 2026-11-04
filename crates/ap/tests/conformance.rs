use ap::{RenderRequest, Selector, render};
use serde_json::Value;

#[test]
fn frozen_library_cases_and_pure_projection() {
    let corpus: Value =
        serde_json::from_str(include_str!("../conformance/cases.json")).expect("frozen JSON");
    for case in corpus["cases"].as_array().expect("case list") {
        let id = case["id"].as_str().expect("case id");
        let request: RenderRequest =
            serde_json::from_value(case["request"].clone()).expect("frozen typed request");
        let original = request.clone();
        let first = render(&request);
        assert_eq!(request, original, "{id}: input changed");
        assert_eq!(first, render(&request), "{id}: nondeterministic result");
        if let Some(expected) = case["expected"].get("plan") {
            let plan = first.unwrap_or_else(|error| panic!("{id}: unexpected refusal {error:?}"));
            assert_eq!(serde_json::to_value(&plan).unwrap(), *expected, "{id}");
            let mut projected = request.clone();
            for artifact in &plan.artifacts {
                match artifact.selector {
                    Selector::CodexInstructionLink => {
                        projected.codex.instructions = artifact.after.clone();
                    }
                    Selector::ClaudeCanonicalImport => {
                        projected.claude.instructions = artifact.after.clone();
                    }
                    Selector::AgyInstructionLink | Selector::AgyGeminiAlias => {
                        panic!("{id}: frozen S02 must not emit an agy selector");
                    }
                }
            }
            let repeated = render(&projected).unwrap_or_else(|error| {
                panic!("{id}: after-state projection refused {error:?}");
            });
            assert_eq!(repeated.preserved, plan.preserved, "{id}: preservation");
            assert_eq!(repeated.artifacts.len(), 2, "{id}: artifact count");
            for (before, after) in plan.artifacts.iter().zip(&repeated.artifacts) {
                assert_eq!(before.path, after.path, "{id}: path");
                assert_eq!(before.selector, after.selector, "{id}: selector");
                assert_eq!(before.after, after.before, "{id}: after becomes before");
                assert_eq!(before.after, after.after, "{id}: desired bytes changed");
                assert!(!after.changed, "{id}: pure projection must be a no-op");
            }
        } else {
            let error = first.expect_err(&format!("{id}: expected refusal"));
            assert_eq!(
                serde_json::to_value(error).unwrap(),
                case["expected"]["error"],
                "{id}"
            );
        }
    }
}
