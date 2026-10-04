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
