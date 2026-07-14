use std::io::Read;

/// Read piped standard input, if any. Returns `None` on a terminal or when the
/// input is empty after trimming.
pub fn read_stdin() -> Option<String> {
    use std::io::IsTerminal;

    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }

    ignore_sigint();

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

/// Ctrl+C delivers SIGINT to every process in the foreground pipeline. A
/// producer like `yap dictate` traps it to flush its final output, but noki
/// would die mid-read and lose the note. Ignoring SIGINT lets the read run
/// to EOF, so the note is stored no matter how the producer is stopped. Not
/// restored afterwards: the process is short-lived and everything after the
/// read is the work being protected.
#[cfg(unix)]
fn ignore_sigint() {
    // SAFETY: `signal` with SIG_IGN installs no user handler and reads no
    // user data; the only effect is process-global disposition, set once
    // before the blocking read.
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
    }
}

#[cfg(not(unix))]
fn ignore_sigint() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_stdin_trims_and_empties_to_none() {
        assert_eq!(clean_stdin("  hello \n"), Some("hello".to_string()));
        assert_eq!(clean_stdin("   \n"), None);
    }

    #[cfg(unix)]
    #[test]
    fn ignore_sigint_marks_sigint_ignored() {
        // NOTE: this leaves SIGINT ignored for the rest of the test binary;
        // children spawned by later tests inherit it (see editor.rs's
        // reset_sigint test).
        ignore_sigint();

        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::sigaction(libc::SIGINT, std::ptr::null(), &mut action) };
        assert_eq!(rc, 0);
        assert_eq!(action.sa_sigaction, libc::SIG_IGN);
    }
}
