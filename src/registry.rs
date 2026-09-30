//! The retained participant role model for the legacy admin surface.
//! Conversation registry reads and reconciliation retired under W2.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Participant {
    pub id: i64,
    pub name: String,
    pub display: String,
    pub kind: String,
    pub adapter: Option<String>,
    pub respond: String,
    /// 'user' or 'admin'. The admin surface (operator boundary) is
    /// gated on it; v1 assignment is the config's admin list.
    pub role: String,
}

impl Participant {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}
