use crate::{
    CostUsageRecord, Message, Role, RuntimeEvent, RuntimeWireEvent, SessionId, ToolCall,
    ToolResult, decode_tool_input, encode_tool_input,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PermissionLogEntry {
    pub timestamp: u64,
    pub tool_name: String,
    pub decision: String,
    pub reason: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandLogEntry {
    pub timestamp: u64,
    pub name: String,
    pub args: Vec<String>,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionMetaEntry {
    pub timestamp: u64,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
// Evolution discipline: a persisted entry kind must be addable without breaking
// a sibling crate's replay code. `non_exhaustive` forces every out-of-crate
// match to carry a wildcard arm, matching the loader contract that an
// unrecognized on-disk line is quarantined, not fatal.
#[non_exhaustive]
pub enum TranscriptEntry {
    Message { message: Message },
    ToolCall { call: ToolCall },
    ToolResult { result: ToolResult },
    Permission { entry: PermissionLogEntry },
    Command { entry: CommandLogEntry },
    SessionMeta { entry: SessionMetaEntry },
    CostUsage { cost: Box<CostUsageRecord> },
    RuntimeEvent { event: Box<RuntimeEvent> },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TranscriptRowId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TranscriptCursor {
    pub session_id: SessionId,
    pub ordinal: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptRowKind {
    Message { message: Message },
    ToolCall { call: ToolCall },
    ToolResult { result: ToolResult },
    Permission { entry: PermissionLogEntry },
    Command { entry: CommandLogEntry },
    SessionMeta { entry: SessionMetaEntry },
    CostUsage { cost: Box<CostUsageRecord> },
    RuntimeEvent { event: Box<RuntimeEvent> },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TranscriptRow {
    pub id: TranscriptRowId,
    pub cursor: TranscriptCursor,
    pub timestamp: Option<u64>,
    pub kind: TranscriptRowKind,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TranscriptPageRequest {
    pub session_id: SessionId,
    pub before: Option<TranscriptCursor>,
    pub limit: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TranscriptPage {
    pub rows: Vec<TranscriptRow>,
    pub older: Option<TranscriptCursor>,
    pub newer: Option<TranscriptCursor>,
    pub has_more: bool,
}

impl TranscriptRow {
    pub fn from_entry(session_id: &str, ordinal: u64, entry: &TranscriptEntry) -> Self {
        let cursor = TranscriptCursor {
            session_id: session_id.to_string(),
            ordinal,
        };
        Self {
            id: TranscriptRowId(format!("{session_id}:{ordinal}")),
            cursor,
            timestamp: entry.timestamp(),
            kind: TranscriptRowKind::from(entry),
        }
    }
}

impl TranscriptEntry {
    pub fn coalescing_message_id(&self) -> Option<&str> {
        match self {
            TranscriptEntry::Message { message } if message.role == Role::Assistant => {
                Some(&message.id)
            }
            _ => None,
        }
    }

    pub fn timestamp(&self) -> Option<u64> {
        match self {
            TranscriptEntry::Message { message } => Some(message.timestamp),
            TranscriptEntry::Permission { entry } => Some(entry.timestamp),
            TranscriptEntry::Command { entry } => Some(entry.timestamp),
            TranscriptEntry::SessionMeta { entry } => Some(entry.timestamp),
            TranscriptEntry::CostUsage { cost } => cost.recorded_at,
            TranscriptEntry::RuntimeEvent { event } => event.timestamp,
            TranscriptEntry::ToolCall { .. } | TranscriptEntry::ToolResult { .. } => None,
        }
    }
}

impl From<&TranscriptEntry> for TranscriptRowKind {
    fn from(entry: &TranscriptEntry) -> Self {
        match entry {
            TranscriptEntry::Message { message } => TranscriptRowKind::Message {
                message: message.clone(),
            },
            TranscriptEntry::ToolCall { call } => {
                TranscriptRowKind::ToolCall { call: call.clone() }
            }
            TranscriptEntry::ToolResult { result } => TranscriptRowKind::ToolResult {
                result: result.clone(),
            },
            TranscriptEntry::Permission { entry } => TranscriptRowKind::Permission {
                entry: entry.clone(),
            },
            TranscriptEntry::Command { entry } => TranscriptRowKind::Command {
                entry: entry.clone(),
            },
            TranscriptEntry::SessionMeta { entry } => TranscriptRowKind::SessionMeta {
                entry: entry.clone(),
            },
            TranscriptEntry::CostUsage { cost } => TranscriptRowKind::CostUsage {
                cost: Box::new((**cost).clone()),
            },
            TranscriptEntry::RuntimeEvent { event } => TranscriptRowKind::RuntimeEvent {
                event: Box::new((**event).clone()),
            },
        }
    }
}

impl TranscriptEntry {
    pub fn to_json_line(&self) -> String {
        match self {
            TranscriptEntry::Message { message } => format!(
                "{{\"type\":\"message\",\"id\":\"{}\",\"role\":\"{}\",\"content\":\"{}\",\"timestamp\":{},\"tool_name\":{},\"tool_call_id\":{}}}",
                escape_json(&message.id),
                message.role.as_str(),
                escape_json(&message.content),
                message.timestamp,
                optional_json_string(message.tool_name.as_deref()),
                optional_json_string(message.tool_call_id.as_deref())
            ),
            TranscriptEntry::ToolCall { call } => format!(
                "{{\"type\":\"tool_call\",\"id\":\"{}\",\"name\":\"{}\",\"input\":\"{}\"}}",
                escape_json(&call.id),
                escape_json(&call.name),
                escape_json(&encode_tool_input(&call.input))
            ),
            TranscriptEntry::ToolResult { result } => format!(
                "{{\"type\":\"tool_result\",\"tool_call_id\":\"{}\",\"name\":\"{}\",\"output\":\"{}\",\"diff\":{},\"success\":{},\"exit_code\":{}}}",
                escape_json(&result.tool_call_id),
                escape_json(&result.name),
                escape_json(&result.output),
                optional_json_string(result.diff.as_deref()),
                if result.success { "true" } else { "false" },
                optional_i32(result.exit_code)
            ),
            TranscriptEntry::Permission { entry } => format!(
                "{{\"type\":\"permission\",\"timestamp\":{},\"tool_name\":\"{}\",\"decision\":\"{}\",\"reason\":\"{}\",\"message\":{}}}",
                entry.timestamp,
                escape_json(&entry.tool_name),
                escape_json(&entry.decision),
                escape_json(&entry.reason),
                optional_json_string(entry.message.as_deref())
            ),
            TranscriptEntry::Command { entry } => format!(
                "{{\"type\":\"command\",\"timestamp\":{},\"name\":\"{}\",\"args\":\"{}\",\"output\":\"{}\"}}",
                entry.timestamp,
                escape_json(&entry.name),
                escape_json(&entry.args.join("\t")),
                escape_json(&entry.output)
            ),
            TranscriptEntry::SessionMeta { entry } => format!(
                "{{\"type\":\"session_meta\",\"timestamp\":{},\"key\":\"{}\",\"value\":\"{}\"}}",
                entry.timestamp,
                escape_json(&entry.key),
                escape_json(&entry.value)
            ),
            TranscriptEntry::CostUsage { cost } => serde_json::json!({
                "type": "cost_usage",
                "cost": cost,
            })
            .to_string(),
            TranscriptEntry::RuntimeEvent { event } => serde_json::json!({
                "type": "runtime_event",
                "event": event,
            })
            .to_string(),
        }
    }

    pub fn from_json_line(line: &str) -> Result<Self, String> {
        let kind = transcript_entry_type(line).or_else(|_| extract_string_field(line, "type"))?;
        match kind.as_str() {
            "message" => Ok(TranscriptEntry::Message {
                message: Message {
                    id: extract_string_field(line, "id")?,
                    role: Role::parse(&extract_string_field(line, "role")?)
                        .ok_or_else(|| "Unknown role".to_string())?,
                    content: extract_string_field(line, "content")?,
                    timestamp: extract_u64_field(line, "timestamp")?,
                    tool_name: extract_optional_string_field(line, "tool_name")?,
                    tool_call_id: extract_optional_string_field(line, "tool_call_id")?,
                },
            }),
            "tool_call" => Ok(TranscriptEntry::ToolCall {
                call: ToolCall {
                    id: extract_string_field(line, "id")?,
                    name: extract_string_field(line, "name")?,
                    input: decode_tool_input(&extract_string_field(line, "input")?),
                },
            }),
            "tool_result" => Ok(TranscriptEntry::ToolResult {
                result: ToolResult {
                    tool_call_id: extract_string_field(line, "tool_call_id")?,
                    name: extract_string_field(line, "name")?,
                    output: extract_string_field(line, "output")?,
                    diff: extract_optional_string_field(line, "diff")?,
                    success: extract_bool_field(line, "success")?,
                    exit_code: extract_optional_i32_field(line, "exit_code")?,
                },
            }),
            "permission" => Ok(TranscriptEntry::Permission {
                entry: PermissionLogEntry {
                    timestamp: extract_u64_field(line, "timestamp")?,
                    tool_name: extract_string_field(line, "tool_name")?,
                    decision: extract_string_field(line, "decision")?,
                    reason: extract_string_field(line, "reason")?,
                    message: extract_optional_string_field(line, "message")?,
                },
            }),
            "command" => Ok(TranscriptEntry::Command {
                entry: CommandLogEntry {
                    timestamp: extract_u64_field(line, "timestamp")?,
                    name: extract_string_field(line, "name")?,
                    args: extract_string_field(line, "args")?
                        .split('\t')
                        .filter(|part| !part.is_empty())
                        .map(ToString::to_string)
                        .collect(),
                    output: extract_string_field(line, "output")?,
                },
            }),
            "session_meta" => Ok(TranscriptEntry::SessionMeta {
                entry: SessionMetaEntry {
                    timestamp: extract_u64_field(line, "timestamp")?,
                    key: extract_string_field(line, "key")?,
                    value: extract_string_field(line, "value")?,
                },
            }),
            "cost_usage" => cost_usage_entry_from_json_line(line),
            "runtime_event" => runtime_event_entry_from_json_line(line),
            // Forward compatibility: a line written by a newer build is not a
            // corrupt file. The error names the type so the session loader can
            // quarantine exactly this line and report why.
            other => Err(format!("Unknown transcript entry type `{other}`")),
        }
    }
}

/// Reads only the top-level transcript discriminator while discarding payload values.
///
/// This deliberately avoids materializing message bodies or nested runtime-event payloads.
pub fn transcript_entry_type(line: &str) -> Result<String, String> {
    use serde::de::{IgnoredAny, MapAccess, Visitor};

    struct TranscriptTypeVisitor;

    impl<'de> Visitor<'de> for TranscriptTypeVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a transcript entry object with a top-level type field")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut entry_type = None;
            while let Some(key) = map.next_key::<String>()? {
                if key == "type" {
                    entry_type = Some(map.next_value::<String>()?);
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            entry_type.ok_or_else(|| serde::de::Error::missing_field("type"))
        }
    }

    let mut deserializer = serde_json::Deserializer::from_str(line);
    let entry_type = serde::Deserializer::deserialize_map(&mut deserializer, TranscriptTypeVisitor)
        .map_err(|_| "Malformed transcript entry".to_string())?;
    deserializer
        .end()
        .map_err(|_| "Malformed transcript entry".to_string())?;
    Ok(entry_type)
}

/// Reads the stable timestamp field for a transcript entry without materializing payloads.
pub fn transcript_entry_timestamp(line: &str) -> Result<Option<u64>, String> {
    #[derive(serde::Deserialize)]
    struct TimestampField {
        timestamp: Option<u64>,
    }

    #[derive(serde::Deserialize)]
    struct RuntimeEventEnvelope {
        event: TimestampField,
    }

    #[derive(serde::Deserialize)]
    struct RecordedAtField {
        recorded_at: Option<u64>,
    }

    #[derive(serde::Deserialize)]
    struct CostUsageEnvelope {
        cost: RecordedAtField,
    }

    let entry_type = transcript_entry_type(line)?;
    match entry_type.as_str() {
        "message" | "permission" | "command" | "session_meta" => {
            serde_json::from_str::<TimestampField>(line)
                .map(|entry| entry.timestamp)
                .map_err(|_| "Malformed transcript timestamp".to_string())
        }
        "runtime_event" => serde_json::from_str::<RuntimeEventEnvelope>(line)
            .map(|entry| entry.event.timestamp)
            .map_err(|_| "Malformed runtime event timestamp".to_string()),
        "cost_usage" => serde_json::from_str::<CostUsageEnvelope>(line)
            .map(|entry| entry.cost.recorded_at)
            .map_err(|_| "Malformed cost usage timestamp".to_string()),
        "tool_call" | "tool_result" => Ok(None),
        other => Err(format!("Unknown transcript entry type `{other}`")),
    }
}

fn cost_usage_entry_from_json_line(line: &str) -> Result<TranscriptEntry, String> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|_| "Malformed cost usage transcript entry")?;
    let cost_value = value.get("cost").cloned().unwrap_or_else(|| value.clone());
    let cost =
        serde_json::from_value(cost_value).map_err(|_| "Malformed cost usage transcript entry")?;
    Ok(TranscriptEntry::CostUsage {
        cost: Box::new(cost),
    })
}

/// Replays a persisted `runtime_event` line through the same forward-compatible
/// path the live wire uses.
///
/// On-disk replay and the live stream must agree about what "known" means: an
/// event this build cannot reduce is reported as an unknown type so the session
/// loader can quarantine that one line, exactly as the wire reduces
/// [`RuntimeWireEvent::Unknown`] to a no-op. The raw line is never rewritten;
/// transcripts are append-only.
fn runtime_event_entry_from_json_line(line: &str) -> Result<TranscriptEntry, String> {
    #[derive(serde::Deserialize)]
    struct RuntimeEventEntry {
        event: RuntimeWireEvent,
    }

    let entry: RuntimeEventEntry = serde_json::from_str(line)
        .map_err(|err| format!("Malformed runtime event transcript entry: {err}"))?;
    match entry.event {
        RuntimeWireEvent::Known(event) => Ok(TranscriptEntry::RuntimeEvent {
            event: Box::new(event),
        }),
        RuntimeWireEvent::Unknown { event_type, .. } => Err(format!(
            "Unknown runtime event type `{event_type}` in transcript entry"
        )),
    }
}

fn optional_json_string(value: Option<&str>) -> String {
    value
        .map(|value| format!("\"{}\"", escape_json(value)))
        .unwrap_or_else(|| "null".to_string())
}

fn optional_i32(value: Option<i32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn escape_json(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

fn extract_string_field(line: &str, field: &str) -> Result<String, String> {
    let marker = format!("\"{field}\":\"");
    let start = line
        .find(&marker)
        .ok_or_else(|| format!("Missing field `{field}`"))?
        + marker.len();
    parse_json_string_from(line, start)
}

fn extract_optional_string_field(line: &str, field: &str) -> Result<Option<String>, String> {
    let marker = format!("\"{field}\":");
    let start = line
        .find(&marker)
        .ok_or_else(|| format!("Missing field `{field}`"))?
        + marker.len();
    if line[start..].starts_with("null") {
        Ok(None)
    } else if line[start..].starts_with('"') {
        Ok(Some(parse_json_string_from(line, start + 1)?))
    } else {
        Err(format!("Invalid optional string field `{field}`"))
    }
}

/// Decodes the JSON string whose body starts at byte offset `start` in `line`.
///
/// Read-side UTF-8 contract (compatibility follow-up 12, closed by C11): the
/// persisted line is already valid UTF-8. The writer escapes only `"`, `\`,
/// and the three ASCII control characters it needs and emits every other
/// character raw, so the unescaped span is copied character by character.
/// Copying it *byte* by byte and casting each byte to `char` mapped every byte
/// at or above `0x80` to its Latin-1 code point, which replayed a persisted
/// `"café 你好"` as mojibake on every reader of the log. This is a read-side
/// fix only: nothing on disk changes and no migration is involved, because the
/// bytes were always right.
///
/// A `\u` escape is decoded to the character it names, including an astral
/// character written as a surrogate pair, because a foreign writer of this
/// format may produce escapes this crate never emits. An escape that names no
/// character — an unpaired surrogate, or too few hex digits — is unreadable
/// rather than silently reinterpreted: it becomes
/// [`char::REPLACEMENT_CHARACTER`] so the rest of the line still replays.
fn parse_json_string_from(line: &str, start: usize) -> Result<String, String> {
    if start > line.len() || !line.is_char_boundary(start) {
        return Err("JSON string does not start on a character boundary".to_string());
    }
    let mut out = String::new();
    let mut chars = line[start..].chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Ok(out),
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('b') => out.push('\u{8}'),
                Some('f') => out.push('\u{c}'),
                Some('u') => out.push(decode_json_unicode_escape(&mut chars)),
                // `\"`, `\\`, `\/`, and anything else: the escaped character
                // stands for itself, which is the exact inverse of the writer.
                Some(other) => out.push(other),
                None => break,
            },
            other => out.push(other),
        }
    }
    Err("Unterminated JSON string".to_string())
}

/// Decodes the four hex digits after a `\u`, plus the trailing surrogate
/// escape that must follow when those digits name a leading surrogate.
fn decode_json_unicode_escape(chars: &mut std::str::Chars<'_>) -> char {
    let Some(first) = take_four_hex_digits(chars) else {
        return char::REPLACEMENT_CHARACTER;
    };
    if let Some(ch) = char::from_u32(first) {
        return ch;
    }
    // `char::from_u32` rejects exactly the surrogate range, so a leading
    // surrogate is the only value worth pairing; a lone trailing one names
    // nothing on its own.
    if !(0xd800..=0xdbff).contains(&first) {
        return char::REPLACEMENT_CHARACTER;
    }
    // Peek rather than consume: an unpaired leading surrogate must leave the
    // escape that follows it intact so that one still decodes on its own.
    let mut paired = chars.clone();
    if paired.next() != Some('\\') || paired.next() != Some('u') {
        return char::REPLACEMENT_CHARACTER;
    }
    let Some(second) =
        take_four_hex_digits(&mut paired).filter(|value| (0xdc00..=0xdfff).contains(value))
    else {
        return char::REPLACEMENT_CHARACTER;
    };
    *chars = paired;
    let combined = 0x1_0000 + ((first - 0xd800) << 10) + (second - 0xdc00);
    char::from_u32(combined).unwrap_or(char::REPLACEMENT_CHARACTER)
}

fn take_four_hex_digits(chars: &mut std::str::Chars<'_>) -> Option<u32> {
    let mut value = 0;
    for _ in 0..4 {
        value = value * 16 + chars.next()?.to_digit(16)?;
    }
    Some(value)
}

fn extract_u64_field(line: &str, field: &str) -> Result<u64, String> {
    let marker = format!("\"{field}\":");
    let start = line
        .find(&marker)
        .ok_or_else(|| format!("Missing field `{field}`"))?
        + marker.len();
    let tail = &line[start..];
    let end = tail.find([',', '}']).unwrap_or(tail.len());
    tail[..end]
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("Invalid number in `{field}`"))
}

fn extract_optional_i32_field(line: &str, field: &str) -> Result<Option<i32>, String> {
    let marker = format!("\"{field}\":");
    let Some(start) = line.find(&marker).map(|index| index + marker.len()) else {
        return Ok(None);
    };
    let tail = &line[start..];
    if tail.starts_with("null") {
        return Ok(None);
    }
    let end = tail.find([',', '}']).unwrap_or(tail.len());
    tail[..end]
        .trim()
        .parse::<i32>()
        .map(Some)
        .map_err(|_| format!("Invalid number in `{field}`"))
}

fn extract_bool_field(line: &str, field: &str) -> Result<bool, String> {
    let marker = format!("\"{field}\":");
    let start = line
        .find(&marker)
        .ok_or_else(|| format!("Missing field `{field}`"))?
        + marker.len();
    let tail = &line[start..];
    if tail.starts_with("true") {
        Ok(true)
    } else if tail.starts_with("false") {
        Ok(false)
    } else {
        Err(format!("Invalid bool in `{field}`"))
    }
}
