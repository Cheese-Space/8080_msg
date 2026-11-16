use async_sqlite::{self, Pool as DBPool};
use lib8080_msg::{self as lib, Message};
use std::error::Error as StdError;
use std::fmt;
use std::marker::Unpin;
use tokio::io::{self, AsyncWriteExt, BufWriter};
type Result<T, E = Error> = ::core::result::Result<T, E>;
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Sql(async_sqlite::Error),
    Serde(serde_json::Error),
}
impl From<io::Error> for Error {
    #[inline]
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<async_sqlite::Error> for Error {
    #[inline]
    fn from(value: async_sqlite::Error) -> Self {
        Self::Sql(value)
    }
}
impl From<serde_json::Error> for Error {
    #[inline]
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Sql(e) => write!(f, "sql error: {e}"),
            Self::Serde(e) => write!(f, "serde error: {e}"),
        }
    }
}
impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Sql(e) => Some(e),
            Self::Serde(e) => Some(e),
        }
    }
}
// makes sure that this trait can only be implemented in this mod
mod inner {
    pub trait Inner {}
}
impl inner::Inner for DBPool {}
pub trait DbExt: inner::Inner {
    async fn send_n_most_recent<W: AsyncWriteExt + Unpin>(
        &self,
        n: u32,
        writer: &mut W,
    ) -> Result<(), Error>;
    async fn add_record(&self, msg: &lib::Message) -> Result<()>;
}
impl DbExt for DBPool {
    async fn send_n_most_recent<W: AsyncWriteExt + Unpin>(
        &self,
        n: u32,
        writer: &mut W,
    ) -> Result<()> {
        // prevents excessive sys calls which could hurt performance
        let mut writer = BufWriter::new(writer);
        let vec = self
            .conn(move |db| {
                let mut stmt = db.prepare(
                "SELECT user, message FROM Messages WHERE id > (SELECT MAX(id) - ? FROM Messages)",
            )?;
                let iter = stmt.query_map([n], |row| {
                    let user: String = row.get("user")?;
                    let msg: String = row.get("message")?;
                    Ok(lib::Packet::Msg(Message::new(&user, &msg)))
                })?;
                let mut buf = Vec::with_capacity(n as usize);
                for p in iter {
                    buf.push(p?);
                }
                Ok(buf)
            })
            .await?;
        for packet in vec {
            writer.write_all(&Vec::try_from(&packet)?).await?;
        }
        writer.flush().await?;
        Ok(())
    }
    async fn add_record(&self, msg: &lib::Message) -> Result<()> {
        let message = msg.get_message().to_string();
        let user = msg.get_username().to_string();
        self.conn(move |conn| {
            conn.execute(
                "INSERT INTO Messages (user, message) VALUES (?, ?)",
                [user, message],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| e.into())
    }
}
