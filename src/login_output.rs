#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoginOutput {
    pub url: Option<String>,
    pub code: Option<String>,
}

pub fn parse_login_output(raw: &str) -> LoginOutput {
    let mut output = LoginOutput::default();
    for line in raw.lines() {
        let clean = strip_ansi(line);
        let clean = clean.trim();
        if clean.is_empty() {
            continue;
        }

        if let Some(url) = extract_url(clean) {
            match output.url.as_ref() {
                None => output.url = Some(url),
                Some(existing) => {
                    if url.len() > existing.len() {
                        output.url = Some(url);
                    }
                }
            }
        }

        if output.code.is_none() && looks_like_code(clean) {
            output.code = Some(clean.to_string());
        }
    }
    output
}

fn strip_ansi(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                let _ = chars.next();
                for next in chars.by_ref() {
                    if next == 'm' {
                        break;
                    }
                }
                continue;
            }
        }
        output.push(ch);
    }
    output
}

fn extract_url(line: &str) -> Option<String> {
    let http_idx = line.find("http://");
    let https_idx = line.find("https://");

    let start = match (http_idx, https_idx) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };

    let mut url = line[start..].trim().split_whitespace().next()?.to_string();
    while matches!(url.chars().last(), Some('.' | ',' | ';' | ':' | ')' | ']' | '}')) {
        url.pop();
    }
    Some(url)
}

fn looks_like_code(line: &str) -> bool {
    if !line.contains('-') || line.contains(' ') {
        return false;
    }
    let mut has_payload = false;
    for ch in line.chars() {
        if ch == '-' {
            continue;
        }
        has_payload = true;
        if !ch.is_ascii_uppercase() && !ch.is_ascii_digit() {
            return false;
        }
    }
    has_payload && (6..=24).contains(&line.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_code_output() {
        let output = "Open this link\nhttps://auth.openai.com/codex/device\n\nEnter this one-time code\nABCD-EFGH\n";
        let parsed = parse_login_output(output);
        assert_eq!(
            parsed.url.as_deref(),
            Some("https://auth.openai.com/codex/device")
        );
        assert_eq!(parsed.code.as_deref(), Some("ABCD-EFGH"));
    }

    #[test]
    fn parses_local_server_output() {
        let output = "Starting local login server on http://localhost:1455.\nIf your browser did not open, navigate to this URL to authenticate:\n\nhttp://localhost:1455/auth/authorize?foo=bar\n";
        let parsed = parse_login_output(output);
        assert_eq!(
            parsed.url.as_deref(),
            Some("http://localhost:1455/auth/authorize?foo=bar")
        );
        assert_eq!(parsed.code, None);
    }
}
