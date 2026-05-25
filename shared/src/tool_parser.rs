//! Tool call parser for converting LLM browser text output into structured OpenAI tool_calls.
//!
//! DeepSeek (and other LLMs) in the browser output tool calls as text patterns:
//! - JSON code blocks: \`\`\`json\n{"name": "func", "arguments": {...}}\n\`\`\`
//! - XML-style blocks (for models that use them)
//! - Raw JSON objects
//!
//! This parser detects these patterns and converts them to structured `ToolCall` format.

use crate::{FunctionCall, ToolCall};

/// Result of parsing text for tool calls.
#[derive(Debug, Clone)]
pub struct ToolCallParseResult {
    /// Whether any tool calls were found.
    pub has_tool_calls: bool,
    /// Extracted tool calls.
    pub tool_calls: Vec<ToolCall>,
    /// Any text that appeared before the tool calls (prefix).
    pub prefix_text: String,
    /// Any text that appeared after the tool calls (suffix).
    pub suffix_text: String,
}

/// Streaming tool call parser that accumulates text and detects tool call patterns.
#[derive(Clone)]
pub struct ToolCallParser {
    buffer: String,
    /// Detected tool calls so far.
    tool_calls: Vec<ToolCall>,
    /// Text before the first tool call.
    prefix: String,
    /// Whether we've started seeing tool call patterns.
    in_tool_call: bool,
    /// Counter for generating unique tool call IDs.
    call_counter: u32,
}

impl ToolCallParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            tool_calls: Vec::new(),
            prefix: String::new(),
            in_tool_call: false,
            call_counter: 0,
        }
    }

    /// Feed a text chunk into the parser.
    pub fn feed(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);
        self.try_extract_calls();
    }

    /// Finish parsing and return results.
    pub fn finish(mut self) -> ToolCallParseResult {
        // Try one final extraction
        self.try_extract_calls();

        let has_tool_calls = !self.tool_calls.is_empty();
        let suffix = if self.in_tool_call {
            // Remaining buffer after last extracted call
            self.buffer.clone()
        } else {
            self.buffer[self.prefix.len()..].to_string()
        };

        ToolCallParseResult {
            has_tool_calls,
            tool_calls: self.tool_calls,
            prefix_text: self.prefix.clone(),
            suffix_text: suffix.trim().to_string(),
        }
    }

    fn try_extract_calls(&mut self) {
        let content = self.buffer.clone();

        // Look for JSON code blocks: ```json ... ``` or ``` ... ```
        // Try multiple start patterns to handle different formatting
        let start_patterns = [
            "\n```json\n",     // newline before code block
            "\n```json\r\n",
            "```json\n",       // at buffer start
            "```json\r\n",
            "\n```json",       // no trailing newline yet (still streaming)
            "```json",         // at buffer start, still streaming
        ];
        let end_patterns = ["\n```", "\n```"];

        for start_pat in &start_patterns {
            if let Some(start_idx) = content.find(start_pat) {
                let json_start = start_idx + start_pat.len();
                for end_pat in &end_patterns {
                    if let Some(end_idx) = content[json_start..].find(end_pat) {
                        let json_str = &content[json_start..json_start + end_idx].trim();
                        if let Ok(tool_calls) = parse_json_tool_calls(json_str, &mut self.call_counter) {
                            if !tool_calls.is_empty() {
                                if !self.in_tool_call {
                                    self.prefix = content[..start_idx].trim().to_string();
                                    self.in_tool_call = true;
                                }
                                self.tool_calls.extend(tool_calls);
                                let after = json_start + end_idx + end_pat.len();
                                self.buffer = self.buffer[after..].to_string();
                                return;
                            }
                        }
                    }
                }
            }
        }

        // Look for raw JSON tool call objects (no code fences)
        // Pattern: {"name": "...", "arguments": {...}}  (with or without spaces)
        if !self.in_tool_call {
            let name_patterns = [
                r#"{"name": ""#,    // standard JSON spacing
                r#"{"name":""#,     // compact
                r#"{ "name": ""#,   // space after brace
            ];
            for name_pat in &name_patterns {
                if let Some(start) = content.find(name_pat) {
                    // Find the matching closing brace
                    let mut depth = 0i32;
                    let mut end = None;
                    for (i, ch) in content[start..].char_indices() {
                        match ch {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = Some(start + i + 1);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(end_idx) = end {
                        let json_str = &content[start..end_idx];
                        if let Ok(tool_calls) = parse_json_tool_calls(json_str, &mut self.call_counter) {
                            if !tool_calls.is_empty() {
                                self.prefix = content[..start].trim().to_string();
                                self.in_tool_call = true;
                                self.tool_calls.extend(tool_calls);
                                self.buffer = self.buffer[end_idx..].to_string();
                                return;
                            }
                        }
                    }
                }
            }
        }

        // Look for XML-style tool calls (some models use this format)
        // Pattern: <tool_calls>[{"name": "...", "arguments": {...}}]</tool_calls>
        if !self.in_tool_call {
            if let Some(start) = content.find("<tool_calls>") {
                if let Some(end) = content[start..].find("</tool_calls>") {
                    let xml_start = start + "<tool_calls>".len();
                    let json_str = &content[xml_start..start + end];
                    if let Ok(tool_calls) = parse_json_tool_calls(json_str, &mut self.call_counter) {
                        self.prefix = content[..start].trim().to_string();
                        self.in_tool_call = true;
                        self.tool_calls.extend(tool_calls);
                        let after = start + end + "</tool_calls>".len();
                        self.buffer = self.buffer[after..].to_string();
                        return;
                    }
                }
            }
        }
    }
}

/// Parse a JSON string that may contain a single tool call or an array of tool calls.
fn parse_json_tool_calls(json_str: &str, counter: &mut u32) -> Result<Vec<ToolCall>, serde_json::Error> {
    let trimmed = json_str.trim();

    // Try parsing as array first
    if trimmed.starts_with('[') {
        let calls: Vec<RawToolCall> = serde_json::from_str(trimmed)?;
        return Ok(calls.into_iter().map(|raw| raw.to_tool_call(counter)).collect());
    }

    // Try parsing as single object
    let raw: RawToolCall = serde_json::from_str(trimmed)?;
    Ok(vec![raw.to_tool_call(counter)])
}

/// Raw JSON structure for parsing tool calls from LLM output.
/// Handles both {"name": "...", "arguments": {...}} and
/// {"name": "...", "arguments": "{...}"} (arguments as string).
#[derive(serde::Deserialize)]
struct RawToolCall {
    name: String,
    #[serde(default)]
    arguments: Option<serde_json::Value>,
}

impl RawToolCall {
    fn to_tool_call(self, counter: &mut u32) -> ToolCall {
        *counter += 1;
        let args_str = match self.arguments {
            Some(serde_json::Value::String(s)) => s,
            Some(val) => serde_json::to_string(&val).unwrap_or_default(),
            None => "{}".to_string(),
        };

        ToolCall {
            id: format!("call_{}", counter),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: self.name,
                arguments: args_str,
            },
        }
    }
}

/// Build a system prompt injection that tells the LLM how to output tool calls.
/// This is injected into the system message when the request includes tools.
pub fn build_tool_system_prompt(tools: &[crate::Tool]) -> String {
    let tool_defs: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.function.name,
                "description": t.function.description,
                "parameters": t.function.parameters,
            })
        })
        .collect();

    format!(
        r#"You have access to the following tools. To call a tool, output a JSON code block with the function call:

```json
{{"name": "function_name", "arguments": {{"param1": "value1"}}}}
```

For multiple tool calls, output an array:

```json
[{{"name": "func1", "arguments": {{"x": 1}}}}, {{"name": "func2", "arguments": {{"y": 2}}}}]
```

Available tools:
{}"#,
        serde_json::to_string_pretty(&tool_defs).unwrap_or_default()
    )
}

/// Check if a text chunk contains a tool call pattern (for quick detection during streaming).
pub fn contains_tool_call_pattern(text: &str) -> bool {
    text.contains("```json")
        || text.contains(r#""name":""#)
        || text.contains("<tool_calls>")
}

/// Convenience: parse an entire text string for tool calls (non-streaming).
pub fn parse_tool_calls_from_text(text: &str) -> ToolCallParseResult {
    let mut parser = ToolCallParser::new();
    parser.feed(text);
    parser.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_json_code_block() {
        let text = r#"I'll edit the file for you.

```json
{"name": "edit_file", "arguments": {"path": "src/main.rs", "old_text": "fn main()", "new_text": "fn main() {\n    println!(\"hello\");\n}"}}
```

Done!"#;

        let result = parse_tool_calls_from_text(text);
        assert!(result.has_tool_calls);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].function.name, "edit_file");
        assert!(result.prefix_text.contains("I'll edit"));
        assert!(result.suffix_text.contains("Done"));
    }

    #[test]
    fn test_parse_array_of_tool_calls() {
        let text = r#"Let me fix both files.

```json
[{"name": "edit_file", "arguments": {"path": "a.rs", "old_text": "old", "new_text": "new"}}, {"name": "edit_file", "arguments": {"path": "b.rs", "old_text": "old2", "new_text": "new2"}}]
```"#;

        let result = parse_tool_calls_from_text(text);
        assert!(result.has_tool_calls);
        assert_eq!(result.tool_calls.len(), 2);
        assert_eq!(result.tool_calls[0].function.name, "edit_file");
        assert_eq!(result.tool_calls[1].function.name, "edit_file");
    }

    #[test]
    fn test_parse_raw_json_no_fences() {
        let text = r#"Here's the fix:
{"name": "edit_file", "arguments": {"path": "main.rs", "old_text": "bug", "new_text": "fixed"}}"#;

        let result = parse_tool_calls_from_text(text);
        assert!(result.has_tool_calls);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].function.name, "edit_file");
    }

    #[test]
    fn test_parse_xml_style() {
        let text = r#"I'll help you with that.

<tool_calls>[{"name": "read_file", "arguments": {"path": "Cargo.toml"}}]</tool_calls>"#;

        let result = parse_tool_calls_from_text(text);
        assert!(result.has_tool_calls);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].function.name, "read_file");
    }

    #[test]
    fn test_no_tool_calls() {
        let text = "I'll help you with that. Here's the code:\n\n```rust\nfn main() {}\n```";
        let result = parse_tool_calls_from_text(text);
        assert!(!result.has_tool_calls);
    }

    #[test]
    fn test_prefix_text_preserved() {
        let text = r#"I'll read the file first.

```json
{"name": "read_file", "arguments": {"path": "src/main.rs"}}
```"#;

        let result = parse_tool_calls_from_text(text);
        assert!(result.has_tool_calls);
        assert!(result.prefix_text.contains("I'll read"));
    }

    #[test]
    fn test_streaming_parser() {
        let mut parser = ToolCallParser::new();

        // Feed in small chunks — realistic LLM streaming
        parser.feed("I'll edit the file.\n\n```json\n");
        parser.feed("{\"name\": \"edit_file\", \"arguments\": {\"path\": \"main.rs\", \"old_text\": \"old\", \"new_text\": \"new\"}}");
        parser.feed("\n```");

        let result = parser.finish();
        assert!(result.has_tool_calls);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].function.name, "edit_file");
    }

    #[test]
    fn test_build_tool_system_prompt() {
        use crate::{FunctionDefinition, Tool};

        let tools = vec![Tool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "edit_file".to_string(),
                description: Some("Edit a file".to_string()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "old_text": {"type": "string"},
                        "new_text": {"type": "string"}
                    }
                })),
            },
        }];

        let prompt = build_tool_system_prompt(&tools);
        assert!(prompt.contains("edit_file"));
        assert!(prompt.contains("```json"));
        assert!(prompt.contains("Available tools"));
    }

    #[test]
    fn test_contains_tool_call_pattern() {
        assert!(contains_tool_call_pattern("```json"));
        assert!(contains_tool_call_pattern(r#"{"name":"foo""#));
        assert!(contains_tool_call_pattern("<tool_calls>"));
        assert!(!contains_tool_call_pattern("Hello, how can I help?"));
    }

    #[test]
    fn test_arguments_as_string() {
        let text = r#"```json
{"name": "edit_file", "arguments": "{\"path\": \"main.rs\", \"old_text\": \"old\", \"new_text\": \"new\"}"}
```"#;

        let result = parse_tool_calls_from_text(text);
        assert!(result.has_tool_calls);
        assert_eq!(result.tool_calls.len(), 1);
        assert!(result.tool_calls[0].function.arguments.contains("main.rs"));
    }
}
