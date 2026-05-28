use std::collections::{HashMap, HashSet};

use crate::DriverError;

/// Expand invocation-time variables in raw Algraf source.
///
/// Values are inserted as already-escaped Algraf source fragments. Diagnostics
/// and parser spans after expansion refer to the expanded source.
pub fn expand_variables(
    source: &str,
    variables: &HashMap<String, String>,
) -> Result<String, DriverError> {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch != '$' {
            out.push(ch);
            continue;
        }

        match chars.peek().copied() {
            Some((_, '{')) => {
                chars.next();
                let mut name = String::new();
                let mut closed = false;
                for (_, ch) in chars.by_ref() {
                    if ch == '}' {
                        closed = true;
                        break;
                    }
                    name.push(ch);
                }
                if !closed {
                    return Err(DriverError::Usage(
                        "unterminated variable placeholder; expected ${name}".to_string(),
                    ));
                }
                push_variable(&mut out, &name, variables)?;
            }
            Some((_, next)) if is_name_start(next) => {
                let mut name = String::new();
                while let Some((_, next)) = chars.peek().copied() {
                    if !is_name_continue(next) {
                        break;
                    }
                    name.push(next);
                    chars.next();
                }
                push_variable(&mut out, &name, variables)?;
            }
            _ => out.push('$'),
        }
    }

    Ok(out)
}

/// Parse repeated `key=value` CLI assignments, rejecting duplicates.
pub fn parse_variable_assignments(
    assignments: &[String],
) -> Result<HashMap<String, String>, DriverError> {
    let mut variables = HashMap::new();
    let mut seen = HashSet::new();
    for assignment in assignments {
        let Some((key, value)) = assignment.split_once('=') else {
            return Err(DriverError::Usage(format!(
                "invalid variable {assignment:?}; expected key=value"
            )));
        };
        if !is_valid_name(key) {
            return Err(DriverError::Usage(format!(
                "invalid variable name {key:?}; use letters, digits, and underscores, starting with a letter or underscore"
            )));
        }
        if !seen.insert(key.to_string()) {
            return Err(DriverError::Usage(format!(
                "duplicate variable {key:?}; pass each --var key once"
            )));
        }
        variables.insert(key.to_string(), value.to_string());
    }
    Ok(variables)
}

fn push_variable(
    out: &mut String,
    name: &str,
    variables: &HashMap<String, String>,
) -> Result<(), DriverError> {
    if !is_valid_name(name) {
        return Err(DriverError::Usage(format!(
            "invalid variable placeholder {name:?}; expected ${{name}}"
        )));
    }
    let value = variables
        .get(name)
        .ok_or_else(|| DriverError::Usage(format!("undefined variable {name:?}")))?;
    out.push_str(value);
    Ok(())
}

fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    is_name_start(first) && chars.all(is_name_continue)
}

fn is_name_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_name_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_braced_and_bare_placeholders() {
        let variables = HashMap::from([
            ("color".to_string(), "#e74c3c".to_string()),
            ("size".to_string(), "3".to_string()),
        ]);

        let expanded =
            expand_variables(r#"Point(fill: "$color", size: ${size})"#, &variables).unwrap();

        assert_eq!(expanded, r##"Point(fill: "#e74c3c", size: 3)"##);
    }

    #[test]
    fn rejects_missing_and_duplicate_variables() {
        let variables = HashMap::new();
        assert!(matches!(
            expand_variables("Point(size: $size)", &variables),
            Err(DriverError::Usage(message)) if message.contains("undefined variable")
        ));

        assert!(matches!(
            parse_variable_assignments(&["a=1".to_string(), "a=2".to_string()]),
            Err(DriverError::Usage(message)) if message.contains("duplicate variable")
        ));
    }
}
