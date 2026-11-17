use crate::sender;
use lib8080_msg as lib;
use sha2::{Digest, Sha256};
use std::collections::hash_map::Entry;
use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::sync::Arc;
use tokio::sync::oneshot;
/// is a sha256 id.
type ID = [u8; 32];
type Result<T, E = Error> = ::core::result::Result<T, E>;
#[derive(Debug)]
pub enum Error {
    Serde(serde_json::Error),
    Io(io::Error),
    RecvClosed,
    UserAlreadyExits,
    UserDoesntExist,
    CantKickYourself,
    GotKicked,
    NoPerm,
    CantChangeOwnPerm,
}
impl From<serde_json::Error> for Error {
    #[inline]
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value)
    }
}
impl From<io::Error> for Error {
    #[inline]
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serde(e) => write!(f, "serde error: {e}"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::RecvClosed => write!(f, "receiver already closed"),
            Self::UserAlreadyExits => write!(f, "user already exists"),
            Self::UserDoesntExist => write!(f, "user doesn't exist"),
            Self::CantKickYourself => write!(f, "cannot kick yourself"),
            Self::GotKicked => write!(f, "you were kicked"),
            Self::NoPerm => write!(f, "don't have permission to do the requested operation"),
            Self::CantChangeOwnPerm => write!(f, "cannot change your own permission"),
        }
    }
}
impl StdError for Error {}
pub struct Query {
    packet: lib::Packet,
    sender: oneshot::Sender<Result<Vec<u8>>>,
    id: ID,
}
impl Query {
    #[inline]
    pub async fn process_and_send(self, state: &mut sender::State) -> Result<()> {
        let data = self.process(state).await;
        self.send(data)
    }
    async fn process(&self, state: &mut sender::State) -> Result<Vec<u8>> {
        use lib::Packet;
        // we handle Join here, so that we can check if the user has been kicked bellow
        // if we didn't check for join here, then this function would wrongfully return Error::GotKicked when you would try to join
        if let Packet::Join(ref name) = self.packet {
            let hash: ID = Sha256::digest(name.as_bytes()).into();
            let privilege = if state.user_book.is_empty() {
                Some(lib::UserPrivilege::Admin)
            } else {
                None // UserPrivilege::Normal, should probably not use Option in the User constructor
            };
            // TODO: replace with try_insert once that stabilizes
            // if we used insert(), then the old value would be overwritten by the new value hence Entry is used
            let entry = state.user_book.entry(hash);
            match entry {
                Entry::Occupied(_) => return Err(Error::UserAlreadyExits),
                Entry::Vacant(_) => {
                    entry.insert_entry(lib::User::new(name, privilege));
                    return Ok(Vec::new());
                }
            }
        }
        // if get returns None, then the user has been kicked
        let user = state.user_book.get(&self.id).ok_or(Error::GotKicked)?;
        match self.packet {
            Packet::Exit => {
                // we ignore the case were the user doesn't exist, as a client should only be able to send one exit request
                state.user_book.remove(&self.id);
                Ok(Vec::new())
            }
            Packet::Join(_) => unreachable!("handled above line: {} in {}", line!(), file!()),
            Packet::GetPort => {
                let packet = Packet::Msg(lib::Message::new_from_owned(
                    "server".into(),
                    format!("port = {}", state.port),
                ));
                Ok(Vec::try_from(&packet)?)
            }
            Packet::Kick(ref n) => {
                let hash: ID = Sha256::digest(n.as_bytes()).into();
                if hash == self.id {
                    return Err(Error::CantKickYourself);
                }
                if !matches!(user.get_privilege(), lib::UserPrivilege::Admin) {
                    return Err(Error::NoPerm);
                }
                state
                    .user_book
                    .remove(&hash)
                    .ok_or(Error::UserDoesntExist)?;
                Ok(Vec::new())
            }
            // TODO: tell the other user they have been de/promoted via a private msg
            Packet::SetPrivilege(ref n, p) => {
                let hash: ID = Sha256::digest(n.as_bytes()).into();
                if !matches!(user.get_privilege(), lib::UserPrivilege::Admin) {
                    return Err(Error::NoPerm);
                }
                if hash == self.id {
                    return Err(Error::CantChangeOwnPerm);
                }
                let other_user = state
                    .user_book
                    .get_mut(&hash)
                    .ok_or(Error::UserDoesntExist)?;
                other_user.set_privilege(p);
                Ok(Vec::new())
            }
            Packet::Msg(_) | Packet::FileMsg { .. } => {
                let packet = self.packet.clone();
                let _ = state.msg_sender.send(Arc::new(packet));
                Ok(Vec::new())
            }
            Packet::File(_) => unreachable!("handled in reader thread"),
            Packet::FetchFile { ref id } => {
                let actual_name = state
                    .file_book
                    .get(id)
                    .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
                let mut file = lib::UserFile::new_async(&state.cache_path.join(id)).await?;
                file.set_name(actual_name.as_str());
                let packet = Packet::File(lib::FileTransfer::new(lib::Message::new("", ""), file));
                Ok(Vec::try_from(&packet)?)
            }
        }
    }
    #[inline]
    fn send(self, data: Result<Vec<u8>>) -> Result<()> {
        self.sender.send(data).map_err(|_| Error::RecvClosed)
    }
}
