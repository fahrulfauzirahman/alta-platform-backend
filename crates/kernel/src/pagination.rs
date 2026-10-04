use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRequest {
    #[serde(default = "default_limit")]
    pub limit: i64,
    pub cursor: Option<String>,
}

fn default_limit() -> i64 { 50 }

impl PageRequest {
    pub fn normalized(&self) -> Self {
        let limit = self.limit.clamp(1, 200);
        Self { limit, cursor: self.cursor.clone() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageResult<T: Serialize> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

/// Keyset cursor for (created_at DESC, id DESC) ordering.
/// Format: "<unix_micros>:<uuid>" — no extra deps, `:` is legal in query strings.
pub fn encode_cursor(created_at: chrono::DateTime<chrono::Utc>, id: uuid::Uuid) -> String {
    format!("{}:{}", created_at.timestamp_micros(), id)
}

pub fn decode_cursor(s: &str) -> Option<(chrono::DateTime<chrono::Utc>, uuid::Uuid)> {
    let (ts_part, id_part) = s.split_once(':')?;
    let micros: i64 = ts_part.parse().ok()?;
    let id = uuid::Uuid::parse_str(id_part.trim()).ok()?;
    let dt = chrono::DateTime::from_timestamp_micros(micros)?;
    Some((dt, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_roundtrip() {
        let now = chrono::Utc::now();
        let id = uuid::Uuid::new_v4();
        let s = encode_cursor(now, id);
        let (dt, back) = decode_cursor(&s).expect("decode");
        assert_eq!(back, id);
        assert_eq!(dt.timestamp_micros(), now.timestamp_micros());
    }

    #[test]
    fn cursor_rejects_garbage() {
        assert!(decode_cursor("nope").is_none());
        assert!(decode_cursor("123:not-a-uuid").is_none());
    }
}
