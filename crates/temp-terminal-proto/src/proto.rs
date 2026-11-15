//! Control protocol structures, line-delimited JSON-RPC, and errors matching `tempterm/internal/proto/proto.go`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead};
use thiserror::Error;

/// Protocol version number. Clients with mismatched versions are refused.
pub const PROTOCOL_VERSION: i64 = 1;

/// Maximum allowable line length in control protocol (64 MiB), matching Go `MaxLine = 64 << 20`.
pub const MAX_LINE: usize = 64 << 20;

/// Standard control protocol error codes.
pub const CODE_USAGE: &str = "usage";
pub const CODE_NOT_FOUND: &str = "not_found";
pub const CODE_REMOTE_CONTROL: &str = "remote_control";
pub const CODE_BUSY: &str = "busy";
pub const CODE_TIMEOUT: &str = "timeout";
pub const CODE_APP_CLOSED: &str = "app_closed";
pub const CODE_INTERNAL: &str = "internal";

/// Control protocol error type with an attached code.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{msg}")]
pub struct ProtocolError {
    pub code: String,
    pub msg: String,
}

impl ProtocolError {
    pub fn new(code: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            msg: msg.into(),
        }
    }
}

/// Helpers to match Go's `encoding/json` `omitempty` semantics:
/// Go omits empty strings, numeric 0, boolean false, and empty slices/maps.
#[inline]
pub fn is_empty_str(s: &str) -> bool {
    s.is_empty()
}

#[inline]
pub fn is_zero_i64(n: &i64) -> bool {
    *n == 0
}

#[inline]
pub fn is_zero_f64(n: &f64) -> bool {
    *n == 0.0
}

#[inline]
pub fn is_false(b: &bool) -> bool {
    !*b
}

#[inline]
pub fn is_none_or_empty_str(opt: &Option<String>) -> bool {
    match opt {
        None => true,
        Some(s) => s.is_empty(),
    }
}

#[inline]
pub fn is_none_or_zero_i64(opt: &Option<i64>) -> bool {
    match opt {
        None => true,
        Some(n) => *n == 0,
    }
}

#[inline]
pub fn is_none_or_zero_f64(opt: &Option<f64>) -> bool {
    match opt {
        None => true,
        Some(n) => *n == 0.0,
    }
}

#[inline]
pub fn is_none_or_false(opt: &Option<bool>) -> bool {
    match opt {
        None => true,
        Some(b) => !*b,
    }
}

#[inline]
pub fn is_none_or_empty_vec<T>(opt: &Option<Vec<T>>) -> bool {
    match opt {
        None => true,
        Some(v) => v.is_empty(),
    }
}

#[inline]
pub fn is_none_or_empty_map<K, V>(opt: &Option<std::collections::BTreeMap<K, V>>) -> bool {
    match opt {
        None => true,
        Some(m) => m.is_empty(),
    }
}

/// Deserializer helper to treat JSON `null` (or missing value) as `Default::default()`,
/// exactly matching Go `encoding/json.Unmarshal` semantics for non-pointer types.
pub fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de> + Default,
{
    let opt = Option::<T>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// Client handshake message (first line sent upon connection).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hello {
    pub kind: String,
    pub version: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub client: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub pane: String,
}

/// Server handshake response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Welcome {
    pub version: i64,
    pub pid: i64,
    pub started: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub app: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub error: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub cols: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub rows: i64,
}

/// JSON-RPC call request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub id: i64,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC response or notification event message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Message {
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub error: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub code: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub event: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_state::deserialize"
    )]
    pub state: Option<State>,
}

/// Project in workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Project {
    #[serde(default, deserialize_with = "null_as_default")]
    pub id: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub dir: String,
}

/// Listening port of a process in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Port {
    #[serde(default, deserialize_with = "null_as_default")]
    pub port: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub name: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub process: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub pid: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub url: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub open: bool,
}

/// Git worktree metadata for a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Worktree {
    #[serde(default, deserialize_with = "null_as_default")]
    pub dir: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub branch: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub base: String,
}

/// Layout tree of panes in a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Layout {
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub pane: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub row: bool,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_f64"
    )]
    pub ratio: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<Box<Layout>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<Box<Layout>>,
}

/// Agent turn status or OSC record information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Turn {
    #[serde(default, deserialize_with = "null_as_default")]
    pub tree: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub root: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub at: i64,
}

/// Agent session identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AgentSession {
    #[serde(default, deserialize_with = "null_as_default")]
    pub agent: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub id: String,
}

/// OSC 7501 status record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OscRecord {
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub id: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub state: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub kind: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub progress: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub app: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub title: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub msg: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub updated: i64,
}

/// Pane info snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Pane {
    #[serde(default, deserialize_with = "null_as_default")]
    pub id: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub session: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub project: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub role: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub pid: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub fgpgid: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub process: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub agent: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub runtime: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub shell: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub dir: String,
    #[serde(
        rename = "processDir",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub process_dir: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub title: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub url: String,
    #[serde(default, skip_serializing_if = "is_none_or_empty_vec")]
    pub command: Option<Vec<String>>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub status: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub since: i64,
    #[serde(default, deserialize_with = "null_as_default")]
    pub activity: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub unread: bool,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub message: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub kind: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub summary: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub progress: i64,
    #[serde(default, skip_serializing_if = "is_none_or_empty_vec")]
    pub records: Option<Vec<OscRecord>>,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub bells: i64,
    #[serde(rename = "exitCode", default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(
        rename = "commandExit",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub command_exit: Option<i64>,
    #[serde(
        rename = "agentSession",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub agent_session: Option<AgentSession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<Turn>,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub restored: bool,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub inbox: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub held: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub asking: String,
}

/// Project process info.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Process {
    #[serde(default, deserialize_with = "null_as_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub command: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub dir: String,
    #[serde(default, skip_serializing_if = "is_none_or_empty_map")]
    pub env: Option<std::collections::BTreeMap<String, String>>,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub port: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub autostart: bool,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub restart: String,
    #[serde(default, skip_serializing_if = "is_none_or_empty_vec")]
    pub watch: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub url: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub status: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub pane: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub session: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub pid: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub restarts: i64,
    #[serde(
        rename = "lastExit",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub last_exit: i64,
    #[serde(
        rename = "lastChange",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub last_change: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub since: i64,
    #[serde(default, deserialize_with = "null_as_default")]
    pub trusted: bool,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub changed: bool,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub message: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub link: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub share: String,
    #[serde(
        rename = "shareVia",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub share_via: String,
}

/// Session in state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Session {
    #[serde(default, deserialize_with = "null_as_default")]
    pub id: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub project: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub name: String,
    #[serde(
        rename = "agentNamed",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub agent_named: bool,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub focus: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub zoomed: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
    #[serde(default)]
    pub panes: Option<Vec<Pane>>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub lead: Pane,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<Worktree>,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub parent: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub process: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proc: Option<Process>,
}

/// Complete state representation answering `state` method or event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct State {
    #[serde(default)]
    pub projects: Option<Vec<Project>>,
    #[serde(default)]
    pub sessions: Option<Vec<Session>>,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub viewed: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub started: i64,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub workspace: String,
    #[serde(default, skip_serializing_if = "is_none_or_empty_vec")]
    pub ports: Option<Vec<Port>>,
}

pub mod deserialize_state {
    use super::*;
    use serde::de::{self, Deserializer, MapAccess, Visitor};
    use std::fmt;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<State>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OptStateVisitor;

        impl<'de> Visitor<'de> for OptStateVisitor {
            type Value = Option<State>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a State object, null, or missing")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(None)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(None)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_map(self)
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut projects = None;
                let mut sessions = None;
                let mut viewed: Option<String> = None;
                let mut started = 0;
                let mut workspace: Option<String> = None;
                let mut ports = None;

                while let Some(key) = access.next_key::<String>()? {
                    match key.as_str() {
                        "projects" => projects = access.next_value()?,
                        "sessions" => sessions = access.next_value()?,
                        "viewed" => {
                            let opt: Option<String> = access.next_value()?;
                            viewed = opt;
                        }
                        "started" => {
                            let opt: Option<i64> = access.next_value()?;
                            started = opt.unwrap_or(0);
                        }
                        "workspace" => {
                            let opt: Option<String> = access.next_value()?;
                            workspace = opt;
                        }
                        "ports" => ports = access.next_value()?,
                        _ => {
                            let _ = access.next_value::<serde_json::Value>()?;
                        }
                    }
                }

                Ok(Some(State {
                    projects,
                    sessions,
                    viewed: viewed.unwrap_or_default(),
                    started,
                    workspace: workspace.unwrap_or_default(),
                    ports,
                }))
            }
        }

        deserializer.deserialize_option(OptStateVisitor)
    }
}

/// Parameter structures for individual protocol methods.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CreateParams {
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub project: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub name: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub dir: String,
    #[serde(default, skip_serializing_if = "is_none_or_empty_vec")]
    pub command: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "is_none_or_empty_vec")]
    pub env: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub focus: bool,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub url: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub split: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub direction: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_f64"
    )]
    pub ratio: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeParams>,
    #[serde(
        rename = "by",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub by_pane: String,
}

/// `WorktreeParams` make a session's worktree: on `branch`, a new branch from `base`.
/// Corresponds to Go `tempterm/internal/proto/proto.go:207-213`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorktreeParams {
    pub branch: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub base: String,
    #[serde(
        rename = "noEnv",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub no_env: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Created {
    pub session: String,
    pub pane: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IDParams {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RenameParams {
    pub id: String,
    pub name: String,
    #[serde(
        rename = "by",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub by_pane: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SendParams {
    pub id: String,
    pub data: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub paste: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KeyParams {
    pub id: String,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ReadParams {
    pub id: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub scrollback: bool,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub lines: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ReadResult {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WaitParams {
    pub id: String,
    #[serde(default, skip_serializing_if = "is_none_or_empty_vec")]
    pub status: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub exit: bool,
    #[serde(
        rename = "timeout",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BrowserParams {
    pub id: String,
    pub op: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub selector: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub text: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub enter: bool,
    #[serde(
        rename = "timeout",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BrowserResult {
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub text: String,
    #[serde(
        default,
        skip_serializing_if = "is_none_or_empty_vec",
        with = "crate::base64"
    )]
    pub png: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AgentSessionParams {
    pub pid: i64,
    pub agent: String,
    pub session: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AgentSessionResult {
    pub pane: String,
    pub session: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub away: bool,
}

/// `ProcessParams` name a project process for `process.start`, `stop`, `restart`
/// and `trust`; `process.list` and `process.sync` take the project alone.
/// Corresponds to Go `tempterm/internal/proto/proto.go:131`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProcessParams {
    #[serde(default, deserialize_with = "null_as_default")]
    pub project: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub name: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub via: String,
}

// Mail states
pub const MAIL_QUEUED: &str = "queued";
pub const MAIL_DELIVERED: &str = "delivered";
pub const MAIL_REPLIED: &str = "replied";
pub const MAIL_NO_REPLY: &str = "noreply";
pub const MAIL_FAILED: &str = "failed";

/// Mail is a message between agents, or from tempterm to one, as the
/// mailbox carries it (package mail).
/// Corresponds to Go `tempterm/internal/proto/proto.go:267-287`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Mail {
    #[serde(default, deserialize_with = "null_as_default")]
    pub id: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub from: String,
    #[serde(
        rename = "fromName",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub from_name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub to: String,
    #[serde(
        rename = "toName",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub to_name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub text: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub note: bool,
    #[serde(default, deserialize_with = "null_as_default")]
    pub state: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub reply: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub screen: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub why: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub created: i64,
}

/// `MailAskParams` send a message (`mail.ask`).
/// Corresponds to Go `tempterm/internal/proto/proto.go:299-307`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MailAskParams {
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub from: String,
    #[serde(
        rename = "fromName",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub from_name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub to: String,
    #[serde(
        rename = "toName",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub to_name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub text: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub note: bool,
    #[serde(
        rename = "noWait",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub no_wait: bool,
}

/// `MailAwaitParams` wait for a message's answer (`mail.await`).
/// Corresponds to Go `tempterm/internal/proto/proto.go:310-313`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MailAwaitParams {
    #[serde(default, deserialize_with = "null_as_default")]
    pub id: String,
    #[serde(
        rename = "timeout",
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_zero_i64"
    )]
    pub timeout_ms: i64,
}

/// `MailReplyParams` answer a message (`mail.reply`).
/// Corresponds to Go `tempterm/internal/proto/proto.go:317-321`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MailReplyParams {
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub id: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub from: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub text: String,
}

/// `MailLimitParams` let pane From ask pane To at most N times more (`mail.limit`).
/// Corresponds to Go `tempterm/internal/proto/proto.go:325-329`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MailLimitParams {
    #[serde(default, deserialize_with = "null_as_default")]
    pub from: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub to: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub n: i32,
}

/// `MailListParams` list the messages to and from a pane (`mail.list`).
/// Corresponds to Go `tempterm/internal/proto/proto.go:333-335`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MailListParams {
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub pane: String,
}

/// `MergeParams` merge the worktree of session `target` into the branch of pane `by`.
/// Corresponds to Go `tempterm/internal/proto/proto.go:340-348`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MergeParams {
    pub by: String,
    pub target: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub commit: bool,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub message: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub keep: bool,
}

/// `Merged` is the result of `worktree.merge`.
/// Corresponds to Go `tempterm/internal/proto/proto.go:351-356`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Merged {
    pub branch: String,
    pub into: String,
    #[serde(default, skip_serializing_if = "is_none_or_empty_vec")]
    pub commits: Option<Vec<String>>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub removed: bool,
}

/// `ReportParams` give pane `id` a status report (report), as OSC 7501 would.
/// Corresponds to Go `tempterm/internal/proto/proto.go:360-363`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ReportParams {
    pub id: String,
    pub body: String,
}

/// `NotifyParams` sends a system notification for pane `id`.
/// Corresponds to Go `tempterm/internal/proto/proto.go:365-369`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct NotifyParams {
    pub id: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub title: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub body: String,
}

/// `ProjectParams` identify a project folder.
/// Corresponds to Go `tempterm/internal/proto/proto.go:387-390`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProjectParams {
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub name: String,
    pub dir: String,
}

/// `GitParams` ask for a pane's changes against a base: "turn", "head", or "branch".
/// Corresponds to Go `tempterm/internal/proto/proto.go:420-424`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GitParams {
    pub pane: String,
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "is_empty_str"
    )]
    pub base: String,
    #[serde(default, skip_serializing_if = "is_none_or_empty_vec")]
    pub paths: Option<Vec<String>>,
}

/// `Clients` describe apps and client counts attached to daemon.
/// Corresponds to Go `tempterm/internal/proto/proto.go:445-448`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Clients {
    #[serde(default, deserialize_with = "null_as_default")]
    pub apps: i32,
    #[serde(default, deserialize_with = "null_as_default")]
    pub attached: HashMap<String, i32>,
}

/// Reads a line from a buffered reader up to `max` bytes, matching Go `ReadLine`.
pub fn read_line<R: BufRead>(r: &mut R, max: usize) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    loop {
        let available = r.fill_buf()?;
        if available.is_empty() {
            if line.is_empty() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "eof"));
            }
            return Ok(line);
        }

        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            let chunk_len = pos + 1;
            if line.len() + chunk_len > max {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "proto: line too long",
                ));
            }
            line.extend_from_slice(&available[..chunk_len]);
            r.consume(chunk_len);
            return Ok(line);
        } else {
            let chunk_len = available.len();
            if line.len() + chunk_len > max {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "proto: line too long",
                ));
            }
            line.extend_from_slice(available);
            r.consume(chunk_len);
        }
    }
}
