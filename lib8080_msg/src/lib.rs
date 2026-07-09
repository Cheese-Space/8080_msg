//! The internal library for server_8080 and tty_8080.  
//! More usage examples will come when the guide on making a custom client is finished.  
//! 
//! # async
//! If you want to write a [`Packet`] asyncly, you need to enable the async feature.  
//! The async feature is not enabled by default.
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
use serde::Serialize;
use serde::Deserialize;
#[cfg(feature = "async")]
use tokio::io::AsyncWriteExt;
use std::io::{self, Write};
use std::fmt;
use std::marker::Unpin;
// could change into a TinyStr in the future
/// the type representing a username
/// 
/// it is not guaranteed that this type will always be a String
pub type Username = String;
#[derive(Serialize, Deserialize, Debug, Clone)]
/// represents a message
pub struct Message {
    /// the user who send the message
    user: Username,
    /// the actual message
    /// 
    /// the message must be valid utf-8
    msg: String
}
impl Message {
    #[inline]
    /// get a refrence to the username
    pub fn get_username(&self) -> &str {
        &self.user
    }
    #[inline]
    /// get a refrence to the message
    pub fn get_message(&self) -> &str {
        &self.msg
    }
    #[must_use]
    /// create a new message
    pub fn new(user: &str, msg: &str) -> Self {
        Self { user: user.to_string(), msg: msg.to_string() }
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.user, self.msg)
    }
}
/// represents what a [`User`] can and can't do
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub enum UserPrivilege {
    /// only read
    ReadOnly,
    #[default]
    /// read + write, is the default privilege
    Normal,
    /// read + write + kick + change user privilege
    Admin
}
/// a user
#[derive(Debug, Clone)]
pub struct User {
    /// the name of the user
    name: Username,
    /// the privilege of the user, see [`UserPirvilege`]
    privilege: UserPrivilege
}
impl User {
    #[inline]
    /// get the privilege of the user
    pub const fn get_privilege(&self) -> UserPrivilege {
        self.privilege
    }
    #[inline]
    /// set the privilege of the user
    pub const fn set_privilege(&mut self, privilege: UserPrivilege) {
        self.privilege = privilege;
    }
    #[inline]
    /// get a refrence to the username of the user
    pub fn get_username(&self) -> &str {
        &self.name
    }
    #[inline]
    /// create a new User
    pub fn new(name: &str, privilege: Option<UserPrivilege>) -> Self {
        let privilege = privilege.unwrap_or_default();
        Self { name: name.to_string(), privilege }
    }
}
#[derive(Serialize, Deserialize, Debug, Clone)]
/// data which client(s) and the server can send to each other
pub enum Packet {
    /// client: signals that a client wants to exit
    Exit,
    /// client: signals that a client wants to join
    Join(Username),
    /// client: requests the port to connect to
    GetPort,
    /// client: asks the server to kick a certain person
    Kick(Username),
    /// client: asks the server to change the privilege of a certain user  
    /// server: if None is specified for the user, change the privilege of the current user
    SetPrivilege(Option<Username>, UserPrivilege),
    /// client + server: sends a message to a client or the server
    Msg(Message)
}
impl Packet {
    /// send a [`Packet`] to a writer 
    pub fn send<W: Write>(&self, stream: &mut W) -> io::Result<()> {
        let data_as_bytes = Vec::from(self);
        stream.write_all(&data_as_bytes)
    }
    /// send a [`Packet`] to an async writer
    /// 
    /// Note that this function is only available with the async feature enabled.  
    /// Also note that This function only works on async writers which implement tokio's [`AsyncWriteExt`](https://docs.rs/tokio/latest/tokio/io/trait.AsyncWriteExt.html) trait.  
    /// If you want to send a packet to a non-tokio async writer, then you can convert the packet to a [`Vec<u8>`](https://doc.rust-lang.org/std/vec/struct.Vec.html):
    /// ```ignore
    /// let packet = Packet::Exit;
    /// let packet_as_bytes = Vec::from(packet);
    /// ```  
    // todo: allow all async writers?
    #[cfg(feature = "async")]
    #[cfg_attr(docsrs, doc(cfg(feature = "async")))]
    pub async fn send_async<W: AsyncWriteExt + Unpin>(&self, stream: &mut W) -> io::Result<()> {
        let data_as_bytes = Vec::from(self);
        stream.write_all(&data_as_bytes).await
    }
    /// get the inner [`Message`] of a [`Packet`]
    /// 
    /// Returns None if self ≠ Packet::Msg.
    pub fn get_inner_msg(&self) -> Option<&Message> {
        if let Packet::Msg(msg) = self {
            Some(msg)
        }
        else {
            None
        }
    }
}
impl From<&Packet> for Vec<u8> {
    fn from(value: &Packet) -> Self {
        let contents = serde_json::to_string(value).unwrap().as_bytes().to_vec();
        let mut header = (contents.len() as u32).to_be_bytes().to_vec();
        for i in contents {
            header.push(i);
        }
        header
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;
    #[test]
    fn send_test() {
        let packet = Packet::GetPort;
        let packet_as_bytes = Vec::from(&packet);
        let mut should_be_same_as_packet: Vec<u8> = Vec::with_capacity(packet_as_bytes.len());
        packet.send(&mut should_be_same_as_packet).unwrap();
        assert_eq!(should_be_same_as_packet, packet_as_bytes);
    }
    #[test]
    fn inner_msg_fail_test() {
        let packet = Packet::Exit;
        assert_matches!(packet.get_inner_msg(), None);
    }
}