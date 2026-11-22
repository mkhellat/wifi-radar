# wifi-radar-node

Sensing **node agent**: scan → `NodeObservation` (stdout NDJSON, TCP
`--publish` to `wifi-radar-fuse --listen`, or plaintext WebSocket
`--publish-ws` to `--listen-ws`).

Passive observation only — **not** survey-grade; **not** a product 3D claim.
TCP and WebSocket carry the same NDJSON object. WebSocket sends one text
frame and closes. There is no TLS.

```bash
# Offline / CI
cargo run -p wifi-radar-node -- --demo --node-id node-a --pose 0,0,1,0

# Live radio (nl80211, iw fallback)
cargo run -p wifi-radar-node -- -i wlan0 --node-id node-a --pose 0,0,1,0

# Continuous pairing: fuse listens; node republishes on an interval
cargo run -p wifi-radar-fuse -- --listen 127.0.0.1:9800 --solver mlat
cargo run -p wifi-radar-node -- --demo --node-id node-a --pose 0,0,1,0 \
  --publish 127.0.0.1:9800 --interval 2 --count 5

# Oneshot publish (no --interval)
cargo run -p wifi-radar-node -- --demo --node-id node-a --pose 0,0,1,0 \
  --publish 127.0.0.1:9800

# Same pairing over plaintext WebSocket (one text frame, then close)
cargo run -p wifi-radar-fuse -- --listen-ws 127.0.0.1:9801 --solver mlat
cargo run -p wifi-radar-node -- --demo --node-id node-a --pose 0,0,1,0 \
  --publish-ws 127.0.0.1:9801 --interval 2 --count 5
```
