use crate::{ID, query};
use lib::Packet;
use lib8080_msg as lib;
use sha2::{Digest, Sha256};
use std::error::Error as StdError;
use std::fmt;
use std::io;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Serde(serde_json::Error),
    InvalidUname(&'static str),
    FirstRequestNotJoin,
    UserAlreadyExist,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Serde(e) => write!(f, "serde error: {e}"),
            Self::InvalidUname(e) => write!(f, "{e}"),
            Self::FirstRequestNotJoin => write!(f, "first request wasn't join request"),
            Self::UserAlreadyExist => write!(f, "user already exists"),
        }
    }
}
impl From<io::Error> for Error {
    #[inline]
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for Error {
    #[inline]
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value)
    }
}
impl From<&'static str> for Error {
    #[inline]
    fn from(value: &'static str) -> Self {
        Self::InvalidUname(value)
    }
}
impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Serde(e) => Some(e),
            _ => None,
        }
    }
}
/// checks if a username is valid
// you normally shouldn't  return string literals as an Err, but this is only used in one place anyway so it is fine
pub fn username_check(username: &str) -> Result<(), &'static str> {
    match username {
        "" => Err("invalid username: an empty username isn't allowed"),
        "me" => Err("invalid username: me"),
        "server" => Err("invalid username: server"),
        with_space if with_space.contains(' ') => {
            Err("invalid username: a username isn't allowed to have spaces")
        }
        _ => Ok(()),
    }
}
// unlikely that the client would disconnect suddenly hence cold is used
#[cold]
pub async fn handle_closed_client(sender: mpsc::Sender<query::Query>, uid: ID) {
    log::error!("client closed");
    let (exit_query, _) = query::Query::new(lib::Packet::Exit, uid);
    let _ = sender.send(exit_query).await;
}
pub async fn register_user<R: AsyncReadExt + Unpin>(
    stream: &mut R,
    query_sender: &mpsc::Sender<query::Query>,
) -> Result<ID, Error> {
    let size = stream.read_u32().await? as usize;
    let mut data = vec![0u8; size];
    stream.read_exact(&mut data).await?;
    let Packet::Join(uname) = serde_json::from_slice(&data)? else {
        return Err(Error::FirstRequestNotJoin);
    };
    username_check(&uname)?;
    let uid: ID = Sha256::digest(&uname).into();
    let (query, receiver) = query::Query::new(Packet::Join(uname), uid);
    // only way the recv is closed is when the query processing thread closes which only happens
    // when the program terminates, so we can just ignore the closed error
    let _ = query_sender.send(query).await;
    receiver
        .await
        .expect("should send something before closing")
        .map_err(|_| Error::UserAlreadyExist)?;
    Ok(uid)
}
