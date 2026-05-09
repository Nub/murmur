# murmur

Decentralized P2P voice and text chat. Native, encrypted, no servers.

Built in Rust with zero browser technology. Messages travel directly between peers, end-to-end encrypted. No accounts, no sign-ups, no data collection.

## Features

- **P2P networking** — no central server, peers connect directly via libp2p
- **E2E encryption** — all messages encrypted with X25519 + AES-256-GCM, ephemeral keys for forward secrecy
- **Voice chat** — WebRTC with Opus codec and RNNoise AI noise suppression
- **Audio sync** — NTP-style clock synchronization for aligned multi-peer playback
- **NAT traversal** — UPnP auto port-forward, AutoNAT public address discovery, STUN for voice
- **Offline delivery** — peers cache encrypted messages for offline users (store-and-forward)
- **Persistent identity** — Ed25519 keypair saved locally, stable peer ID across restarts
- **Native GUI** — GPU-accelerated via wgpu/iced, not Electron, not a browser

## Install

### Nix (Linux)

```sh
nix run github:Nub/murmur
```

Or build locally:

```sh
git clone https://github.com/Nub/murmur.git
cd murmur
nix build
./result/bin/murmur
```

### Windows

Download `murmur-windows.zip` from [releases](https://github.com/Nub/murmur/releases), extract, run `murmur.exe`.

Or build from source:

```sh
nix build .#windows-zip
```

## Connect to friends

1. Open Settings (gear icon, top-right)
2. Copy one of your listening addresses (click to copy)
3. Send it to your friend
4. They click the `→` button in the top bar and paste your address
5. You appear in each other's peer list

If both peers are on the same LAN, discovery is automatic via mDNS.

For internet connections, at least one peer needs to be reachable (UPnP or manual port forward). Share the public address, not the `127.0.0.1` or `192.168.x.x` one.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                      murmur                         │
├──────────┬──────────┬───────────┬───────────────────┤
│   GUI    │  Crypto  │  Network  │      Media        │
│  iced    │ X25519   │  libp2p   │    WebRTC         │
│  wgpu    │ AES-GCM  │ gossipsub │    Opus           │
│          │ HKDF     │ Kademlia  │    RNNoise        │
│          │          │ mDNS      │    cpal           │
│          │          │ AutoNAT   │    Audio sync     │
│          │          │ UPnP      │                   │
├──────────┴──────────┴───────────┴───────────────────┤
│                    sled (storage)                    │
└─────────────────────────────────────────────────────┘
```

**Text channels** use gossipsub pub/sub. Each server+channel is a topic. Messages are encrypted with a group key derived from the topic via HKDF.

**Direct messages** use libp2p request-response with per-peer X25519 key exchange and ephemeral keys for forward secrecy.

**Voice** uses WebRTC peer connections with DTLS-SRTP encryption. Signaling (SDP/ICE) goes through the libp2p voice protocol. Audio is captured via cpal, denoised with RNNoise, encoded with Opus, and streamed as RTP.

**Discovery** works at three levels:
- **LAN**: mDNS finds peers on the local network automatically
- **Internet**: Kademlia DHT + AutoNAT for address discovery, UPnP for port forwarding
- **Manual**: paste a peer's multiaddr to connect directly

## Data storage

All data is stored locally in `~/.local/share/murmur/` (Linux) or `%APPDATA%\murmur\` (Windows):

| File | Contents |
|------|----------|
| `identity.key` | Your Ed25519 keypair (protobuf encoded) |
| `state.db/` | Servers, channels, messages, peer profiles |
| `crypto.db/` | E2E encryption keys |
| `relay.db/` | Store-and-forward message cache |
| `logs/` | Rolling log files |
| `crash.log` | Last crash details (if any) |

## Slash commands

Type these in the message input:

| Command | Description |
|---------|-------------|
| `/name <name>` | Change your display name |
| `/status <online\|away\|dnd>` | Set your status |
| `/connect <multiaddr>` | Connect to a peer by address |

## Building

Requires [Nix](https://nixos.org/download/) with flakes enabled.

```sh
nix build              # Linux native
nix build .#windows-zip # Windows cross-compiled
nix develop            # Dev shell with cargo, rust-analyzer
```

## License

MIT
