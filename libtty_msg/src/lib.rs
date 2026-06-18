use serde::Serialize;
use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::io::AsyncWriteExt;
use std::io;
use std::fmt;
pub type Username = String;
#[derive(Serialize, Deserialize, Debug)]
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
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub enum UserPrivelige {
    ReadOnly,
    #[default]
    Normal,
    Admin
}
#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    name: Username,
    privelige: UserPrivelige
}
impl User {
    #[inline]
    pub const fn get_privelige(&self) -> UserPrivelige {
        self.privelige
    }
    #[inline]
    pub const fn set_privelige(&mut self, privelige: UserPrivelige) {
        self.privelige = privelige;
    }
    #[inline]
    pub fn get_username(&self) -> &String {
        &self.name
    }
    #[inline]
    pub fn new(name: String, privelige: Option<UserPrivelige>) -> Self {
        let privelige = privelige.unwrap_or_default();
        Self { name, privelige }
    }
}
#[derive(Serialize, Deserialize, Debug)]
pub enum Packet {
    Exit(Username),
    Join(Username),
    Kick {asker: Username, kicked: Username},
    GetPort(Username),
    Msg(Message)
}
impl Packet {
    pub async fn send(self, stream: &mut TcpStream) -> io::Result<()> {
        let data_as_bytes = Vec::from(self);
        stream.write_all(&data_as_bytes).await
    }
    pub async fn send_from_writer(self, stream: &mut OwnedWriteHalf) -> io::Result<()> {
        let data_as_bytes = Vec::from(self);
        stream.write_all(&data_as_bytes).await
    }
}
impl From<Packet> for Vec<u8> {
    fn from(value: Packet) -> Self {
        let contents = serde_json::to_string(&value).unwrap().as_bytes().to_vec();
        let mut header = (contents.len() as u32).to_ne_bytes().to_vec();
        for i in contents {
            header.push(i);
        }
        header
    }
}