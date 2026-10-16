# Teloren Setup

Connecting the teloren terminal frontend to the local Veloren server.

## Prerequisites

- Local server running (`./scripts/start-server.sh`)
- teloren binary built at `/tmp/teloren-probe/target/debug/teloren`

## Connect

```bash
./scripts/connect-teloren.sh HERMES
```

Attach: `tmux attach -t veloren-agent`  
Detach: `Ctrl-b d`  
Stop: `tmux kill-session -t veloren-agent`

## Controls

| Key | Action |
|---|---|
| wasd | Move |
| SPACE | Jump |
| x | Primary attack |
| z | Secondary attack |
| g | Toggle glide |
| r | Respawn |
| return | Open chat |
| q | Quit |

## Known issue: DNS resolution

teloren uses hickory-resolver via `veloren-client`, which may fail to resolve `127.0.0.1` in some environments even though raw TCP works.

Workaround:

```bash
no_proxy=* /tmp/teloren-probe/target/debug/teloren \
  --server 127.0.0.1:14004 \
  --username HERMES \
  --character HERMES \
  --password ''
```

## Alternative: raw TCP client

If teloren won't connect, a raw `std::net::TcpStream::connect("127.0.0.1:14004")` works fine — the issue is isolated to hickory-resolver's `lookup_host`.
