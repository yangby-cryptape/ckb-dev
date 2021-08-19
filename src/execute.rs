use std::{
    fs::OpenOptions,
    io::{prelude::*, BufReader, SeekFrom},
    process::{Command, Stdio},
};

use chrono::{DateTime, Utc};
use fs_extra::dir;
use regex::Regex;
use serde_json::{json, to_string_pretty};
use tempfile::TempDir;
use walkdir::WalkDir;

use crate::{
    argument::{Args, BackupArgs, L1Args, L2Args, RpcArgs},
    config::Config,
    error::{Error, Result},
    qiniu,
    rpc_client::RpcClient,
};

const LOG_TIMESTAMP_REGEX: &str =
    r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}([.]\d{1,3}) [+-]\d{2}:\d{2} ";
const LOG_TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S.%f %:z";
const LOG_MIN_TAIL_CHECK: usize = 200;

pub trait CanExecute {
    fn execute(&self, cfg: &Config) -> Result<()>;
}

impl CanExecute for Args {
    fn execute(&self, cfg: &Config) -> Result<()> {
        match self {
            Self::L1(inner) => inner.execute(cfg),
            Self::L2(inner) => inner.execute(cfg),
            Self::Backup(inner) => inner.execute(cfg),
            Self::Rpc(inner) => inner.execute(cfg),
        }
    }
}

impl CanExecute for L1Args {
    fn execute(&self, cfg: &Config) -> Result<()> {
        let mut command = match self {
            Self::Start => {
                let mut command = Command::new("systemctl");
                command.args(&["start", &cfg.normal.ckb.service_name]);
                command
            }
            Self::Stop => {
                let mut command = Command::new("systemctl");
                command.args(&["stop", &cfg.normal.ckb.service_name]);
                command
            }
            Self::Restart => {
                let mut command = Command::new("systemctl");
                command.args(&["restart", &cfg.normal.ckb.service_name]);
                command
            }
            Self::Status => {
                let mut command = Command::new("systemctl");
                command.args(&["status", &cfg.normal.ckb.service_name]);
                command
            }
            Self::ResetData { peer_store } => {
                let ckb_bin_path = cfg.normal.ckb.bin_path.to_str().expect("ckb.bin_path");
                let ckb_root_dir = cfg.normal.ckb.root_dir.to_str().expect("ckb.root_dir");
                let mut command = Command::new(ckb_bin_path);
                command.args(&["reset-data", "--force", "-C", ckb_root_dir]);
                if *peer_store {
                    command.arg("--network-peer-store");
                } else {
                    command.arg("--all");
                }
                command
            }
        };
        command.stdout(Stdio::inherit()).output().map_err(|err| {
            let msg = format!("failed to execute `{:?}` since {}", command, err);
            Error::Exec(msg)
        })?;
        Ok(())
    }
}

impl CanExecute for L2Args {
    fn execute(&self, _cfg: &Config) -> Result<()> {
        Ok(())
    }
}

impl CanExecute for BackupArgs {
    fn execute(&self, cfg: &Config) -> Result<()> {
        let ckb_data_dir = &cfg.normal.ckb.data_dir;
        let tmp_dir = TempDir::new().map_err(|err| {
            let msg = format!("failed to create tempdir since {}", err);
            Error::Exec(msg)
        })?;
        let tgz_path = {
            let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
            let target_name = format!("{}-{}.tar.gz", cfg.normal.host.ip, timestamp);
            tmp_dir.path().join(target_name)
        };
        let mut command = Command::new("tar");
        command.current_dir(tmp_dir.path()).args(&[
            "-czvf",
            tgz_path.as_path().to_str().expect("tgz_path.to_str()"),
        ]);
        if self.peer_store {
            let src_path = ckb_data_dir.join("network").join("peer_store");
            let mut options = dir::CopyOptions::new();
            options.copy_inside = true;
            dir::copy(&src_path, &tmp_dir, &options).map_err(|err| {
                let msg = format!(
                    "failed to copy '{}' into '{}' since {}",
                    src_path.display(),
                    tmp_dir.path().display(),
                    err
                );
                Error::Exec(msg)
            })?;
            command.arg("peer_store");
        } else {
            let logs_dir = ckb_data_dir.join("logs");
            let dst_path = tmp_dir.path().join("ckb.log");
            let mut write_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dst_path)
                .expect("open log file to write");
            let re = Regex::new(LOG_TIMESTAMP_REGEX).expect("compile regex");
            'read_logfile: for entry in WalkDir::new(logs_dir).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_dir() {
                    continue;
                }
                let is_log = entry
                    .file_name()
                    .to_str()
                    .map(|s| s.ends_with(".log"))
                    .unwrap_or(false);
                if !is_log {
                    continue;
                }
                log::trace!("read log file '{}'", entry.path().display());
                let mut read_file = OpenOptions::new()
                    .read(true)
                    .open(entry.path())
                    .expect("open log file to read");
                let mut check_tail = false;
                let mut skip_at_head = 0;
                for line in BufReader::new(&read_file).lines() {
                    let line_str = line.expect("read line");
                    if let Some(mat) = re.find(&line_str) {
                        let line_ts_str = &line_str.split_at(mat.end() - 1).0;
                        let line_ts = DateTime::parse_from_str(line_ts_str, LOG_TIMESTAMP_FORMAT)
                            .unwrap_or_else(|err| panic!("regex should be right but {}", err));
                        if line_ts > self.logs_around.1 {
                            log::trace!(
                                "skip log file '{}' since after time scope",
                                entry.path().display(),
                            );
                            continue 'read_logfile;
                        } else if line_ts < self.logs_around.0 {
                            check_tail = true;
                        }
                        break;
                    }
                    skip_at_head += 1
                }
                if check_tail {
                    read_file.seek(SeekFrom::Start(0)).expect("seek to start");
                    let lines_count = BufReader::new(&read_file).lines().count();
                    read_file.seek(SeekFrom::Start(0)).expect("seek to start");
                    let last_lines: Vec<String> = if lines_count > LOG_MIN_TAIL_CHECK {
                        BufReader::new(&read_file)
                            .lines()
                            .skip(lines_count - LOG_MIN_TAIL_CHECK)
                            .map(|line| line.expect("read line"))
                            .collect()
                    } else {
                        BufReader::new(&read_file)
                            .lines()
                            .map(|line| line.expect("read line"))
                            .collect()
                    };
                    for line_str in last_lines.into_iter().rev() {
                        if let Some(mat) = re.find(&line_str) {
                            let line_ts_str = &line_str.split_at(mat.end() - 1).0;
                            let line_ts =
                                DateTime::parse_from_str(line_ts_str, LOG_TIMESTAMP_FORMAT)
                                    .unwrap_or_else(|err| {
                                        panic!("regex should be right but {}", err)
                                    });
                            if line_ts < self.logs_around.0 {
                                log::trace!(
                                    "skip log file '{}' since before time scope",
                                    entry.path().display()
                                );
                                continue 'read_logfile;
                            }
                            break;
                        }
                    }
                }
                read_file.seek(SeekFrom::Start(0)).expect("seek to start");
                for line in BufReader::new(&read_file).lines().skip(skip_at_head) {
                    let line_str = line.expect("read line");
                    if let Some(mat) = re.find(&line_str) {
                        let line_ts_str = &line_str.split_at(mat.end() - 1).0;
                        let line_ts = DateTime::parse_from_str(line_ts_str, LOG_TIMESTAMP_FORMAT)
                            .unwrap_or_else(|err| panic!("regex should be right but {}", err));
                        if line_ts < self.logs_around.0 {
                            continue;
                        } else if line_ts > self.logs_around.1 {
                            break;
                        }
                    }
                    write_file
                        .write_all(line_str.as_bytes())
                        .expect("write into log file");
                    write_file
                        .write_all("\n".as_bytes())
                        .expect("write into log file");
                }
            }
            command.arg("ckb.log");
        }
        command.stdout(Stdio::inherit()).output().map_err(|err| {
            let msg = format!("failed to execute `{:?}` since {}", command, err);
            Error::Exec(msg)
        })?;
        let url = qiniu::upload(&cfg.secret.qiniu, tgz_path.as_path())?;
        println!("Upload {} to {}", tgz_path.as_path().display(), url);
        drop(tmp_dir);
        Ok(())
    }
}

impl CanExecute for RpcArgs {
    fn execute(&self, cfg: &Config) -> Result<()> {
        let cli = RpcClient::new(&cfg.normal.ckb.rpc_url)?;
        match self {
            Self::GetPeers { stats } => {
                let peers = cli.get_peers()?;
                let output = if *stats {
                    let (inbound_peers_count, outbound_peers_count) =
                        peers.iter().fold((0, 0), |acc, peer| {
                            if peer.is_outbound {
                                (acc.0, acc.1 + 1)
                            } else {
                                (acc.0 + 1, acc.1)
                            }
                        });
                    let output = json!({
                      "inbound_peers_count": inbound_peers_count,
                      "outbound_peers_count": outbound_peers_count
                    });
                    to_string_pretty(&output)
                } else {
                    to_string_pretty(&peers)
                }
                .expect("serde_json::to_string(..)");
                println!("{}", output)
            }
        }
        Ok(())
    }
}
