use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("config error: {0}")]
    Cfg(String),
    #[error("argument error: {0}")]
    Arg(String),
    #[error("execute error: {0}")]
    Exec(String),
    #[error("rpc error: {0}")]
    Rpc(String),
    #[error("qiniu error: {0}")]
    Qiniu(String),
}

pub(crate) type Result<T> = ::std::result::Result<T, Error>;

impl Error {
    pub(crate) fn config_not_found(key: &str) -> Self {
        Self::Cfg(format!("[{}] not found", key))
    }
}
