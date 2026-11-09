//! Shared fixtures for the resolver tests: a temp home directory, an injectable environment (so no
//! test touches the process environment or the developer's real `~/.aws`) and a frozen clock.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{Context, Env, format_time};

/// The frozen "now" every test runs at: 2023-11-14T22:13:20Z.
pub(crate) const NOW_SECS: u64 = 1_700_000_000;

pub(crate) fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(NOW_SECS)
}

/// RFC 3339 for the frozen now plus `offset_secs`.
pub(crate) fn iso(offset_secs: i64) -> String {
    let secs = (NOW_SECS as i64 + offset_secs) as u64;
    format_time(UNIX_EPOCH + Duration::from_secs(secs))
}

/// Epoch seconds of the frozen now plus `offset_secs`.
pub(crate) fn epoch(offset_secs: i64) -> u64 {
    (NOW_SECS as i64 + offset_secs) as u64
}

pub(crate) fn run<F: Future>(future: F) -> F::Output {
    futures::executor::block_on(future)
}

pub(crate) struct Fixture {
    pub(crate) dir: tempfile::TempDir,
    vars: Vec<(String, String)>,
}

impl Fixture {
    /// A fresh home directory; IMDS disabled so a chain that falls through never leaves the box.
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().expect("create a temp home");
        let home = dir.path().to_string_lossy().into_owned();
        Self {
            dir,
            vars: vec![
                ("HOME".to_string(), home),
                ("AWS_EC2_METADATA_DISABLED".to_string(), "true".to_string()),
            ],
        }
    }

    pub(crate) fn home(&self) -> &Path {
        self.dir.path()
    }

    pub(crate) fn set(mut self, key: &str, value: &str) -> Self {
        self.vars.retain(|(k, _)| k != key);
        self.vars.push((key.to_string(), value.to_string()));
        self
    }

    pub(crate) fn unset(mut self, key: &str) -> Self {
        self.vars.retain(|(k, _)| k != key);
        self
    }

    /// Writes `content` to `rel` under the home directory, creating parents.
    pub(crate) fn write(&self, rel: &str, content: &str) -> PathBuf {
        let path = self.home().join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent directory")).expect("mkdir");
        std::fs::write(&path, content).expect("write fixture file");
        path
    }

    pub(crate) fn config(&self, content: &str) -> PathBuf {
        self.write(".aws/config", content)
    }

    pub(crate) fn credentials(&self, content: &str) -> PathBuf {
        self.write(".aws/credentials", content)
    }

    pub(crate) fn env(&self) -> Env {
        let pairs: Vec<(&str, &str)> = self
            .vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        Env::from_pairs(&pairs)
    }

    pub(crate) fn ctx(&self) -> Context {
        Context::for_test(self.env(), Some(now()))
    }
}

/// An STS `AssumeRole*` XML answer.
pub(crate) fn sts_xml(action: &str, key: &str, secret: &str, token: &str, expiry: &str) -> String {
    format!(
        "<{action}Response xmlns=\"https://sts.amazonaws.com/doc/2011-06-15/\"><{action}Result>\
         <Credentials><AccessKeyId>{key}</AccessKeyId><SecretAccessKey>{secret}</SecretAccessKey>\
         <SessionToken>{token}</SessionToken><Expiration>{expiry}</Expiration></Credentials>\
         </{action}Result></{action}Response>"
    )
}

/// An STS `AssumeRole*` JSON answer (what `Accept: application/json` yields).
pub(crate) fn sts_json(
    action: &str,
    key: &str,
    secret: &str,
    token: &str,
    expiry_epoch: u64,
) -> String {
    format!(
        "{{\"{action}Response\":{{\"{action}Result\":{{\"Credentials\":{{\"AccessKeyId\":\"{key}\",\
         \"SecretAccessKey\":\"{secret}\",\"SessionToken\":\"{token}\",\"Expiration\":{expiry_epoch}.0}}}}}}}}"
    )
}
