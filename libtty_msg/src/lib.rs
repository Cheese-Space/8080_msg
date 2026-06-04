use serde::Serialize;
use serde::Deserialize;
use std::fmt;
#[derive(Serialize, Deserialize)]
pub struct Message {
    pub(crate) user: String,
    pub(crate) msg: String
}
impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.user, self.msg)
    }
}