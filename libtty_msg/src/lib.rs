use serde::Serialize;
use serde::Deserialize;
use std::io::Write;
use std::net::TcpStream;
use std::io;
use std::fmt;
pub type Username = String;
#[derive(Serialize, Deserialize)]
pub struct Message {
    user: Username,
    msg: String
}
impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.user, self.msg)
    }
}
impl Message {
    #[must_use]
    pub fn new(user: &str, msg: &str) -> Self {
        Self { user: user.to_string(), msg: msg.to_string() }
    }
}
#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum UserPrivelige {
    ReadOnly,
    Normal,
    Admin
}
#[derive(Serialize, Deserialize)]
pub struct User {
    name: Username,
    privelige: UserPrivelige
}
#[derive(Serialize, Deserialize)]
pub enum Packet {
    Exit(User),
    Join(User),
    Kick(User),
    GetPort(User),
    Msg(Message)
}
impl Packet {
    pub fn send(&self, stream: &mut TcpStream) -> io::Result<()> {
        writeln!(stream, "{}", serde_json::to_string(self).unwrap())
    }
}