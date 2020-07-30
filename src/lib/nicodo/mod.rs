use serde::Deserialize;

mod xml;

pub use xml::*;

#[derive(Debug, Deserialize)]
pub struct Chat {
    pub date: u64,
    pub vpos: u64,
    pub user_id: Option<String>,
    pub id: Option<String>,
    pub mail: Option<String>,
    pub content: String,
}
