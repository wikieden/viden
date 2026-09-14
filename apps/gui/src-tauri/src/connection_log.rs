//! Structured, always-on host logging for the Core connection state machine.
//!
//! E3d defect 37: the Core connection dropped roughly forty minutes into a
//! session, never recovered, and the process wrote **zero bytes** with
//! `RUST_LOG=info` set — so the run could not say what the transport did. The
//! reason `RUST_LOG` produced nothing is that this workspace has no `log` or
//! `tracing` subscriber anywhere: a level filter was never going to make a
//! line appear. Adding a logging framework for one state machine would be a
//! dependency and a configuration surface; the transitions themselves are what
//! a diagnosis needs.
//!
//! So they are written to stderr unconditionally, one line per transition,
//! never per poll. The shape is `key=value` after a fixed prefix so a `grep`
//! over a captured log reads without a parser, and every value is quoted when
//! it could contain a space.
//!
//! This is host diagnostics, not a product surface: what the operator sees is
//! the D6 recovery projection's own reconnect block. The two are written from
//! the same transitions so they cannot disagree.

/// The fixed prefix every line carries, so one `grep` finds the whole machine.
pub const CONNECTION_LOG_PREFIX: &str = "viden-gui core-connection";

/// Writes one connection-state line to stderr.
///
/// `event` names what happened (`state`, `reconnect-attempt`,
/// `reconnect-result`, `reconnect-gave-up`); `fields` are its facts in the
/// order a reader wants them.
pub fn log_connection(event: &str, fields: &[(&str, &str)]) {
    let mut line = format!("{CONNECTION_LOG_PREFIX} {event}");
    for (key, value) in fields {
        if value.is_empty() {
            continue;
        }
        if value.contains(' ') || value.contains('"') {
            line.push_str(&format!(" {key}={}", quoted(value)));
        } else {
            line.push_str(&format!(" {key}={value}"));
        }
    }
    eprintln!("{line}");
}

/// One value, quoted and escaped, so a detail carrying spaces stays one field.
fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' => out.push(' '),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::quoted;

    #[test]
    fn a_detail_with_spaces_and_quotes_stays_one_field() {
        assert_eq!(quoted("a \"b\" c"), "\"a \\\"b\\\" c\"");
    }

    #[test]
    fn a_newline_never_splits_a_line_in_two() {
        assert_eq!(quoted("first\nsecond"), "\"first second\"");
    }
}
