use std::{fmt::Display, fs::OpenOptions, io::prelude::*};

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

#[derive(Default, Deserialize)]
pub(crate) struct AppConfig {
    pub(crate) data_dir: PathBuf,
    pub(crate) rpc: RpcConfig,
}

#[derive(Default, Deserialize)]
pub(crate) struct RpcConfig {
    pub(crate) listen_address: String,
}

impl AppConfig {
    pub(crate) fn load_from_workdir(dir: &Path) -> Result<Self> {
        let path = dir.join("ckb.toml");
        if !path.exists() {
            return Err(Self::create_error(&path, "find", None::<usize>));
        }
        OpenOptions::new()
            .read(true)
            .open(&path)
            .map_err(|err| Self::create_error(&path, "open", Some(err)))
            .and_then(|mut file| {
                let mut data = Vec::new();
                file.read_to_end(&mut data)
                    .map_err(|err| Self::create_error(&path, "read", Some(err)))
                    .map(|_| data)
            })
            .and_then(|data| {
                toml::from_slice(&data).map_err(|err| Self::create_error(&path, "parse", Some(err)))
            })
    }

    fn create_error<E>(path: &Path, step: &str, error_opt: Option<E>) -> Error
    where
        E: Display,
    {
        let msg = if let Some(error) = error_opt {
            format!(
                "failed to {} the ckb config file \"{}\" since {}",
                step,
                path.display(),
                error
            )
        } else {
            format!(
                "failed to {} the ckb config file \"{}\"",
                step,
                path.display()
            )
        };
        Error::Cfg(msg)
    }
}
