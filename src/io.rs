use std::io::Read;

/// Read piped standard input, if any. Returns `None` on a terminal or when the
/// input is empty after trimming.
pub fn read_stdin() -> Option<String> {
    use std::io::IsTerminal;

    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }

    let mut buffer = String::new();
    if stdin.lock().read_to_string(&mut buffer).is_err() {
        return None;
    }
    clean_stdin(&buffer)
}

pub(crate) fn clean_stdin(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_stdin_trims_and_empties_to_none() {
        assert_eq!(clean_stdin("  hello \n"), Some("hello".to_string()));
        assert_eq!(clean_stdin("   \n"), None);
    }
}
