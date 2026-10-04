pub fn redacted(input: &str) -> &'static str {
    let _ = input;
    "[redacted]"
}

pub fn secret_present(v: Option<String>) -> bool {
    v.map(|s| !s.trim().is_empty()).unwrap_or(false)
}
