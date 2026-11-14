//! A validator of the JSON Schema keywords a stage's structured output uses. A schema with any
//! other keyword is refused, so a keyword is never ignored where it would decide.
use serde_json::Value;

const KEYWORDS: [&str; 13] = [
    "type",
    "enum",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "pattern",
];

/// Whether `schema` uses only the keywords this validator implements, with well-formed values.
pub fn supported(schema: &Value) -> bool {
    let Some(o) = schema.as_object() else {
        return false;
    };
    o.iter().all(|(k, v)| match k.as_str() {
        "properties" => v.as_object().is_some_and(|p| p.values().all(supported)),
        "items" => supported(v),
        "required" | "enum" => v.is_array(),
        "additionalProperties" => v == false,
        "pattern" => v
            .as_str()
            .is_some_and(|p| regex_lite::Regex::new(p).is_ok()),
        "type" => v.is_string() || v.as_array().is_some_and(|t| t.iter().all(Value::is_string)),
        k => KEYWORDS.contains(&k) && v.is_number(),
    })
}

fn kind(v: &Value, t: &str) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        _ => false,
    }
}

/// Whether `v` validates against a `supported` schema.
pub fn valid(schema: &Value, v: &Value) -> bool {
    let num = |k: &str| schema[k].as_f64();
    let len = |k: &str| schema[k].as_u64().map(|n| n as usize);
    let types = match &schema["type"] {
        Value::String(t) => vec![t.as_str()],
        Value::Array(t) => t.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    if !types.is_empty() && !types.iter().any(|t| kind(v, t)) {
        return false;
    }
    if schema["enum"].as_array().is_some_and(|e| !e.contains(v)) {
        return false;
    }
    if let Some(s) = v.as_str() {
        let n = s.chars().count();
        let pattern = schema["pattern"].as_str();
        if len("minLength").is_some_and(|m| n < m)
            || len("maxLength").is_some_and(|m| n > m)
            || pattern.is_some_and(|p| !regex_lite::Regex::new(p).is_ok_and(|r| r.is_match(s)))
        {
            return false;
        }
    }
    if let Some(x) = v.as_f64()
        && (num("minimum").is_some_and(|m| x < m) || num("maximum").is_some_and(|m| x > m))
    {
        return false;
    }
    if let Some(a) = v.as_array()
        && (len("minItems").is_some_and(|m| a.len() < m)
            || len("maxItems").is_some_and(|m| a.len() > m)
            || schema
                .get("items")
                .is_some_and(|i| !a.iter().all(|x| valid(i, x))))
    {
        return false;
    }
    if let Some(o) = v.as_object() {
        let props = schema["properties"].as_object();
        let required = schema["required"].as_array().into_iter().flatten();
        if required
            .filter_map(Value::as_str)
            .any(|k| !o.contains_key(k))
        {
            return false;
        }
        for (k, x) in o {
            match props.and_then(|p| p.get(k)) {
                Some(s) if !valid(s, x) => return false,
                None if schema["additionalProperties"] == false => return false,
                _ => (),
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{supported, valid};
    use serde_json::json;
    #[test]
    fn the_keywords() {
        let s = json!({"type":"object","additionalProperties":false,"required":["h","l"],
            "properties":{"h":{"type":"string","pattern":"^[0-9a-f]{4}$"},
            "l":{"type":"array","maxItems":2,"items":{"enum":["a","b"]}},
            "n":{"type":["integer","null"],"minimum":1}}});
        assert!(supported(&s));
        assert!(valid(&s, &json!({"h":"00af","l":["a"]})));
        assert!(valid(&s, &json!({"h":"00af","l":[],"n":null})));
        assert!(!valid(&s, &json!({"h":"00af","l":[],"n":0})));
        assert!(!valid(&s, &json!({"h":"the head","l":[]})));
        assert!(!valid(&s, &json!({"h":"00af","l":["a","b","a"]})));
        assert!(!valid(&s, &json!({"h":"00af","l":["c"]})));
        assert!(!valid(&s, &json!({"h":"00af"})));
        assert!(!valid(&s, &json!({"h":"00af","l":[],"x":1})));
        assert!(!supported(&json!({"type":"object","oneOf":[]})));
        assert!(!supported(&json!({"pattern":"("})));
    }
}
