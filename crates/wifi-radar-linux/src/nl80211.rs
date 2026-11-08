//! Native nl80211 scan backend (genetlink TRIGGER_SCAN + GET_SCAN dump).
//!
//! Passive observation only — no injection / deauth. `TRIGGER_SCAN` requests a
//! fresh kernel scan; on failure (permissions, busy iface) the backend still
//! falls back to dumping the existing scan cache.

use std::fs;
use std::iter::once;
use std::thread;
use std::time::Duration;

use neli::attr::Attribute;
use neli::consts::nl::NlmF;
use neli::consts::nl80211::Nl80211Command;
use neli::consts::socket::NlFamily;
use neli::genl::{AttrTypeBuilder, Genlmsghdr, GenlmsghdrBuilder, NlattrBuilder, NoUserHeader};
use neli::nl::NlPayload;
use neli::neli_enum;
use neli::router::synchronous::NlRouter;
use neli::types::GenlBuffer;
use neli::utils::Groups;
use wifi_radar_core::{ScanBackend, ScanError, WifiDevice};

use crate::bss::{device_from_bss_attrs, parse_nla_stream};

pub use crate::bss::{devices_from_nl80211_attrs, encode_nla, RawAttr};

/// Default wait after a successful `TRIGGER_SCAN` before dumping results.
pub const DEFAULT_TRIGGER_WAIT: Duration = Duration::from_millis(2500);

/// Minimal nl80211 attribute types needed for TRIGGER_SCAN / GET_SCAN.
#[neli_enum(serialized_type = "u16")]
pub enum Nl80211Attr {
    /// Placeholder.
    Unspec = 0,
    /// Interface index (`NL80211_ATTR_IFINDEX`).
    Ifindex = 3,
    /// Nested BSS (`NL80211_ATTR_BSS`).
    Bss = 47,
}

impl neli::consts::genl::NlAttrType for Nl80211Attr {}

/// Native nl80211 backend using genetlink TRIGGER_SCAN + GET_SCAN dump.
pub struct Nl80211Backend {
    iface: String,
    /// Sleep after a successful trigger before dumping the cache.
    trigger_wait: Duration,
    /// Last scan used a successful trigger (vs dump-only fallback).
    last_triggered: bool,
}

impl Nl80211Backend {
    /// Create an nl80211 backend for `iface`.
    pub fn new(iface: impl Into<String>) -> Self {
        Self {
            iface: iface.into(),
            trigger_wait: DEFAULT_TRIGGER_WAIT,
            last_triggered: false,
        }
    }

    /// Interface name this backend scans.
    pub fn iface(&self) -> &str {
        &self.iface
    }

    /// Whether the most recent [`ScanBackend::scan`] issued a successful trigger.
    pub fn last_triggered(&self) -> bool {
        self.last_triggered
    }

    /// Override the post-trigger wait (tests / tuning).
    pub fn set_trigger_wait(&mut self, wait: Duration) {
        self.trigger_wait = wait;
    }
}

impl ScanBackend for Nl80211Backend {
    fn name(&self) -> &'static str {
        "nl80211"
    }

    fn scan(&mut self) -> Result<Vec<WifiDevice>, ScanError> {
        self.last_triggered = false;
        match trigger_scan(&self.iface) {
            Ok(()) => {
                self.last_triggered = true;
                thread::sleep(self.trigger_wait);
            }
            Err(_) => {
                // Cache dump still useful without CAP_NET_ADMIN / when busy.
            }
        }
        dump_scan(&self.iface)
    }
}

/// Request a fresh scan on `iface` via nl80211 `TRIGGER_SCAN` (ifindex only).
pub fn trigger_scan(iface: &str) -> Result<(), ScanError> {
    let ifindex = read_ifindex(iface)?;
    let (sock, _) = NlRouter::connect(NlFamily::Generic, Some(0), Groups::empty())
        .map_err(|e| ScanError::Message(format!("nl80211 socket: {e}")))?;
    let family_id = sock
        .resolve_genl_family("nl80211")
        .map_err(|e| ScanError::Message(format!("resolve nl80211 family: {e}")))?;

    let attrs = once(
        NlattrBuilder::default()
            .nla_type(
                AttrTypeBuilder::default()
                    .nla_type(Nl80211Attr::Ifindex)
                    .build()
                    .map_err(|e| ScanError::Message(format!("attr type: {e}")))?,
            )
            .nla_payload(ifindex)
            .build()
            .map_err(|e| ScanError::Message(format!("attr payload: {e}")))?,
    )
    .collect::<GenlBuffer<_, _>>();

    let recv = sock
        .send::<_, _, u16, Genlmsghdr<Nl80211Command, Nl80211Attr>>(
            family_id,
            NlmF::ACK,
            NlPayload::Payload(
                GenlmsghdrBuilder::<Nl80211Command, Nl80211Attr, NoUserHeader>::default()
                    .cmd(Nl80211Command::TriggerScan)
                    .version(1)
                    .attrs(attrs)
                    .build()
                    .map_err(|e| ScanError::Message(format!("genl header: {e}")))?,
            ),
        )
        .map_err(|e| ScanError::Message(format!("TRIGGER_SCAN send: {e}")))?;

    for msg in recv {
        let msg = msg.map_err(|e| ScanError::Message(format!("TRIGGER_SCAN recv: {e}")))?;
        match msg.nl_payload() {
            NlPayload::Err(e) => {
                return Err(ScanError::Message(format!("TRIGGER_SCAN nl err: {e}")));
            }
            NlPayload::Ack(_)
            | NlPayload::Empty
            | NlPayload::Payload(_)
            | NlPayload::DumpExtAck(_) => {}
        }
    }
    Ok(())
}

/// Dump cached scan results for `iface` via nl80211 GET_SCAN.
pub fn dump_scan(iface: &str) -> Result<Vec<WifiDevice>, ScanError> {
    let ifindex = read_ifindex(iface)?;
    let (sock, _) = NlRouter::connect(NlFamily::Generic, Some(0), Groups::empty())
        .map_err(|e| ScanError::Message(format!("nl80211 socket: {e}")))?;
    let family_id = sock
        .resolve_genl_family("nl80211")
        .map_err(|e| ScanError::Message(format!("resolve nl80211 family: {e}")))?;

    let attrs = once(
        NlattrBuilder::default()
            .nla_type(
                AttrTypeBuilder::default()
                    .nla_type(Nl80211Attr::Ifindex)
                    .build()
                    .map_err(|e| ScanError::Message(format!("attr type: {e}")))?,
            )
            .nla_payload(ifindex)
            .build()
            .map_err(|e| ScanError::Message(format!("attr payload: {e}")))?,
    )
    .collect::<GenlBuffer<_, _>>();

    let recv = sock
        .send::<_, _, u16, Genlmsghdr<Nl80211Command, Nl80211Attr>>(
            family_id,
            NlmF::DUMP | NlmF::ACK,
            NlPayload::Payload(
                GenlmsghdrBuilder::<Nl80211Command, Nl80211Attr, NoUserHeader>::default()
                    .cmd(Nl80211Command::GetScan)
                    .version(1)
                    .attrs(attrs)
                    .build()
                    .map_err(|e| ScanError::Message(format!("genl header: {e}")))?,
            ),
        )
        .map_err(|e| ScanError::Message(format!("GET_SCAN send: {e}")))?;

    let mut devices = Vec::new();
    for msg in recv {
        let msg = msg.map_err(|e| ScanError::Message(format!("GET_SCAN recv: {e}")))?;
        let payload = match msg.nl_payload() {
            NlPayload::Payload(p) => p,
            _ => continue,
        };
        for attr in payload.attrs().get_attr_handle().iter() {
            if *attr.nla_type().nla_type() != Nl80211Attr::Bss {
                continue;
            }
            let nested = parse_nla_stream(attr.payload().as_ref());
            if let Some(d) = device_from_bss_attrs(&nested) {
                devices.push(d);
            }
        }
    }
    Ok(devices)
}

fn read_ifindex(iface: &str) -> Result<u32, ScanError> {
    if iface.is_empty()
        || iface.contains('/')
        || iface.contains('\0')
        || iface == "."
        || iface == ".."
    {
        return Err(ScanError::Message(format!("invalid iface name: {iface:?}")));
    }
    let path = format!("/sys/class/net/{iface}/ifindex");
    let text = fs::read_to_string(&path).map_err(|e| {
        ScanError::Message(format!("read ifindex for {iface} ({path}): {e}"))
    })?;
    text.trim()
        .parse()
        .map_err(|e| ScanError::Message(format!("parse ifindex for {iface}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_reports_name_and_iface() {
        let b = Nl80211Backend::new("wlan0");
        assert_eq!(b.name(), "nl80211");
        assert_eq!(b.iface(), "wlan0");
        assert!(!b.last_triggered());
    }

    #[test]
    fn rejects_unsafe_iface_names() {
        assert!(read_ifindex("../passwd").is_err());
        assert!(read_ifindex("").is_err());
        assert!(read_ifindex("wlan0/../eth0").is_err());
    }

    #[test]
    fn dump_scan_missing_iface_errors() {
        let err = dump_scan("wifi-radar-no-such-iface")
            .expect_err("missing iface must not succeed");
        assert!(
            err.to_string().contains("ifindex"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn trigger_scan_missing_iface_errors() {
        let err = trigger_scan("wifi-radar-no-such-iface")
            .expect_err("missing iface must not succeed");
        assert!(
            err.to_string().contains("ifindex"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn scan_falls_back_when_trigger_impossible() {
        let mut b = Nl80211Backend::new("wifi-radar-no-such-iface");
        b.set_trigger_wait(Duration::from_millis(0));
        let err = b.scan().expect_err("missing iface must fail dump");
        assert!(!b.last_triggered());
        assert!(
            err.to_string().contains("ifindex"),
            "unexpected error: {err}"
        );
    }
}
