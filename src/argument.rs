use std::convert::TryFrom;

use chrono::{DateTime, Duration, FixedOffset};

use crate::error::{Error, Result};

pub enum Args {
    L1(L1Args),
    L2(L2Args),
    Backup(BackupArgs),
    Rpc(RpcArgs),
}

pub enum L1Args {
    Stop,
    Start,
    Restart,
    Status,
    ResetData { peer_store: bool },
}

pub struct L2Args {}

pub struct BackupArgs {
    pub(crate) logs_around: (DateTime<FixedOffset>, DateTime<FixedOffset>),
    pub(crate) peer_store: bool,
}

pub enum RpcArgs {
    GetPeers { stats: bool },
}

impl Args {
    pub fn load_from_inputs() -> Result<Self> {
        let yaml = clap::load_yaml!("argument.yaml");
        let matches = clap::App::from_yaml(yaml)
            .version(clap::crate_version!())
            .author(clap::crate_authors!("\n"))
            .get_matches();
        Self::try_from(&matches)
    }
}

impl<'a> TryFrom<&'a clap::ArgMatches<'a>> for Args {
    type Error = Error;
    fn try_from(matches: &'a clap::ArgMatches) -> Result<Self> {
        match matches.subcommand() {
            ("l1", Some(matches)) => L1Args::try_from(matches).map(Self::L1),
            ("l2", Some(matches)) => L2Args::try_from(matches).map(Self::L2),
            ("backup", Some(matches)) => BackupArgs::try_from(matches).map(Self::Backup),
            ("rpc", Some(matches)) => RpcArgs::try_from(matches).map(Self::Rpc),
            _ => unreachable!(),
        }
    }
}

impl<'a> TryFrom<&'a clap::ArgMatches<'a>> for L1Args {
    type Error = Error;
    fn try_from(matches: &'a clap::ArgMatches) -> Result<Self> {
        match matches.subcommand() {
            ("start", Some(_matches)) => Ok(Self::Start),
            ("stop", Some(_matches)) => Ok(Self::Stop),
            ("restart", Some(_matches)) => Ok(Self::Restart),
            ("status", Some(_matches)) => Ok(Self::Status),
            ("reset-data", Some(matches)) => {
                let peer_store = matches.is_present("peer-store");
                Ok(Self::ResetData { peer_store })
            }
            _ => unreachable!(),
        }
    }
}

impl<'a> TryFrom<&'a clap::ArgMatches<'a>> for L2Args {
    type Error = Error;
    fn try_from(_matches: &'a clap::ArgMatches) -> Result<Self> {
        Ok(Self {})
    }
}

impl<'a> TryFrom<&'a clap::ArgMatches<'a>> for BackupArgs {
    type Error = Error;
    fn try_from(matches: &'a clap::ArgMatches) -> Result<Self> {
        let logs_around = matches
            .value_of("logs-around")
            .map(|s| {
                DateTime::parse_from_rfc3339(s)
                    .map_err(|err| {
                        Error::Arg(format!("failed to parse \"logs-around\" since {}", err))
                    })
                    .map(|base| {
                        let dur = Duration::minutes(10);
                        (base - dur, base + dur)
                    })
            })
            .unwrap_or_else(|| unreachable!())?;
        let peer_store = matches.is_present("peer-store");
        Ok(Self {
            logs_around,
            peer_store,
        })
    }
}

impl<'a> TryFrom<&'a clap::ArgMatches<'a>> for RpcArgs {
    type Error = Error;
    fn try_from(matches: &'a clap::ArgMatches) -> Result<Self> {
        match matches.subcommand() {
            ("get_peers", Some(matches)) => {
                let stats = matches.is_present("stats");
                Ok(Self::GetPeers { stats })
            }
            _ => unreachable!(),
        }
    }
}
