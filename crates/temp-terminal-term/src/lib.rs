//! Virtual terminal state machine and OSC escape sequence scanner for temp-terminal.

pub mod osc;
pub mod records;

pub use osc::{encode_status, neutralize, parse_status, Report, Scanner, MAX_LEN, QUERY_REPLY};
pub use records::{Records, AGENT_ID, MAX_RECORDS};
