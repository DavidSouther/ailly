use serde_json::Value;

use crate::knowledge::permissions::{Classification, Classifier};

pub struct WebSearchClassifier;

impl Classifier for WebSearchClassifier {
    fn classify(&self, _args: &Value) -> Classification {
        Classification::Safe
    }
}

pub struct WebFetchClassifier;

impl Classifier for WebFetchClassifier {
    fn classify(&self, _args: &Value) -> Classification {
        Classification::Safe
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_search_classifier_returns_safe_for_well_formed_args() {
        let args = serde_json::json!({ "query": "ddd", "max_results": 3 });
        assert_eq!(WebSearchClassifier.classify(&args), Classification::Safe);
    }

    #[test]
    fn web_search_classifier_returns_safe_for_null_args() {
        assert_eq!(
            WebSearchClassifier.classify(&Value::Null),
            Classification::Safe
        );
    }

    #[test]
    fn web_fetch_classifier_returns_safe_for_well_formed_args() {
        let args = serde_json::json!({ "url": "https://example.test/" });
        assert_eq!(WebFetchClassifier.classify(&args), Classification::Safe);
    }

    #[test]
    fn web_fetch_classifier_returns_safe_for_null_args() {
        assert_eq!(
            WebFetchClassifier.classify(&Value::Null),
            Classification::Safe
        );
    }
}
