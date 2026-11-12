# wifi-radar-node

Sensing **node agent**: one scan → one `NodeObservation` (stdout NDJSON or
TCP publish to `wifi-radar-fuse --listen`).

Passive observation only — **not** survey-grade; **not** a product 3D claim.

```bash
# Offline / CI
cargo run -p wifi-radar-node -- --demo --node-id node-a --pose 0,0,1,0

# Live radio (nl80211, iw fallback)
cargo run -p wifi-radar-node -- -i wlan0 --node-id node-a --pose 0,0,1,0

# Publish to a oneshot fuse listener
cargo run -p wifi-radar-fuse -- --listen 127.0.0.1:9800 --solver mlat
cargo run -p wifi-radar-node -- --demo --node-id node-a --pose 0,0,1,0 \
  --publish 127.0.0.1:9800
```
