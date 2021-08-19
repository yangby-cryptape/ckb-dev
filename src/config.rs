use std::{net::IpAddr, path::PathBuf, str::FromStr as _};

use ini::Ini;
use url::Url;

use crate::{
    ckb_config::AppConfig as CkbConfig,
    error::{Error, Result},
};

const NORMAL_CONFIG_FILE: &str = "/etc/ckbdev.conf";
const SECRET_CONFIG_FILE: &str = "/etc/ckbdev.secret.conf";

pub struct Config {
    pub(crate) normal: NormalConfig,
    pub(crate) secret: SecretConfig,
}

pub(crate) struct NormalConfig {
    pub(crate) host: HostSection,
    pub(crate) ckb: CkbSection,
}

pub(crate) struct SecretConfig {
    pub(crate) qiniu: QiniuSection,
}

pub(crate) struct HostSection {
    pub(crate) ip: IpAddr,
    #[allow(dead_code)]
    pub(crate) name: String, // TODO
}

pub(crate) struct CkbSection {
    pub(crate) service_name: String,
    pub(crate) bin_path: PathBuf,
    pub(crate) root_dir: PathBuf,
    pub(crate) data_dir: PathBuf,
    pub(crate) rpc_url: Url,
}

pub(crate) struct QiniuSection {
    pub(crate) access_key: String,
    pub(crate) secret_key: String,
    pub(crate) bucket: String,
    pub(crate) domain: Url,
    pub(crate) path_prefix: String,
}

impl Config {
    pub fn load_from_files() -> Result<Self> {
        let normal = NormalConfig::load_from_file(NORMAL_CONFIG_FILE)?;
        let secret = SecretConfig::load_from_file(SECRET_CONFIG_FILE)?;
        Ok(Self { normal, secret })
    }
}

impl NormalConfig {
    pub(crate) fn load_from_file(path: &str) -> Result<Self> {
        let ini = Ini::load_from_file(path)
            .map_err(|err| Error::Cfg(format!("failed to load \"{}\" since {}", path, err)))?;
        let host = {
            let prop = ini
                .section(Some("host"))
                .ok_or_else(|| Error::config_not_found("host"))?;
            let ip = prop
                .get("ip")
                .ok_or_else(|| Error::config_not_found("host.ip"))
                .and_then(|s| {
                    IpAddr::from_str(s).map_err(|err| {
                        Error::Cfg(format!("failed to parse [host.ip] since {}", err))
                    })
                })?;
            let name = prop
                .get("name")
                .ok_or_else(|| Error::config_not_found("host.name"))?
                .to_owned();
            HostSection { ip, name }
        };
        let ckb = {
            let prop = ini
                .section(Some("ckb"))
                .ok_or_else(|| Error::config_not_found("ckb"))?;
            let service_name = prop
                .get("service_name")
                .ok_or_else(|| Error::config_not_found("ckb.service_name"))?
                .to_owned();
            let bin_path = prop
                .get("bin_path")
                .ok_or_else(|| Error::config_not_found("ckb.bin_path"))
                .map(PathBuf::from)?;
            let root_dir = prop
                .get("root_dir")
                .ok_or_else(|| Error::config_not_found("ckb.root_dir"))
                .map(PathBuf::from)?;
            let ckb_cfg = CkbConfig::load_from_workdir(&root_dir)?;
            let data_dir = root_dir.join(&ckb_cfg.data_dir);
            let rpc_url = {
                let url = format!("http://{}", ckb_cfg.rpc.listen_address);
                Url::parse(&url).map_err(|err| {
                    Error::Cfg(format!(
                        "failed to parse CKB RPC URL [{}] since {}",
                        url, err
                    ))
                })
            }?;
            CkbSection {
                service_name,
                bin_path,
                root_dir,
                data_dir,
                rpc_url,
            }
        };
        Ok(Self { host, ckb })
    }
}

impl SecretConfig {
    pub(crate) fn load_from_file(path: &str) -> Result<Self> {
        let ini = Ini::load_from_file(path)
            .map_err(|err| Error::Cfg(format!("failed to load \"{}\" since {}", path, err)))?;
        let qiniu = {
            let prop = ini
                .section(Some("qiniu"))
                .ok_or_else(|| Error::config_not_found("qiniu"))?;
            let access_key = prop
                .get("access_key")
                .ok_or_else(|| Error::config_not_found("qiniu.access_key"))?
                .to_owned();
            let secret_key = prop
                .get("secret_key")
                .ok_or_else(|| Error::config_not_found("qiniu.secret_key"))?
                .to_owned();
            let bucket = prop
                .get("bucket")
                .ok_or_else(|| Error::config_not_found("qiniu.bucket"))?
                .to_owned();
            let domain = prop
                .get("domain")
                .ok_or_else(|| Error::config_not_found("qiniu.domain"))
                .and_then(|s| {
                    let u = Url::parse(s).map_err(|err| {
                        Error::Cfg(format!("failed to parse [qiniu.domain] since {}", err))
                    })?;
                    if u.scheme() != "http" && u.scheme() != "https" {
                        let msg = "invalid [qiniu.domain], scheme should be \"http\" or \"https\"";
                        let err = Error::Cfg(msg.to_owned());
                        Err(err)
                    } else {
                        Ok(u)
                    }
                })?;
            let path_prefix = prop
                .get("path_prefix")
                .ok_or_else(|| Error::config_not_found("qiniu.path_prefix"))?
                .to_owned();
            QiniuSection {
                access_key,
                secret_key,
                bucket,
                domain,
                path_prefix,
            }
        };
        Ok(Self { qiniu })
    }
}
