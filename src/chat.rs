use serde_json::Value;

/// Flattens JSON text components for display. Click and hover events are never executed.
pub fn flatten_component(value: &Value) -> String {
    let mut out = String::new();
    append(value, &mut out);
    out
}
fn append(value: &Value, out: &mut String) {
    match value {
        Value::String(s) => out.push_str(s),
        Value::Array(items) => for item in items { append(item, out); },
        Value::Object(map) => {
            if let Some(text) = map.get("text") { append(text, out); }
            else if let Some(key) = map.get("translate").and_then(Value::as_str) { out.push_str(key); }
            else if let Some(key) = map.get("keybind").and_then(Value::as_str) { out.push_str(key); }
            if let Some(extra) = map.get("extra") { append(extra, out); }
        }
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Null => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_text() {
        let v: Value = serde_json::from_str(r#"{"text":"Hi ","extra":[{"text":"there"}]}"#).unwrap();
        assert_eq!(flatten_component(&v), "Hi there");
    }
}
