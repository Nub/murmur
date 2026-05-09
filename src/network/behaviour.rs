use crate::types::{DirectMessage, VoiceSignal};
use libp2p::{autonat, gossipsub, identify, kad, mdns, request_response, swarm::NetworkBehaviour, upnp};

/// Combined network behaviour for murmur.
#[derive(NetworkBehaviour)]
pub struct MurmurBehaviour {
    /// Pub/sub messaging for channels and presence
    pub gossipsub: gossipsub::Behaviour,
    /// Local network peer discovery
    pub mdns: mdns::tokio::Behaviour,
    /// Distributed hash table for internet-wide discovery
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    /// Peer identification and capability exchange
    pub identify: identify::Behaviour,
    /// Direct messages via request-response
    pub dm: request_response::cbor::Behaviour<DirectMessage, DirectMessage>,
    /// Voice call signaling
    pub voice: request_response::cbor::Behaviour<VoiceSignal, VoiceSignal>,
    /// AutoNAT: discover if we're publicly reachable + our external address
    pub autonat: autonat::Behaviour,
    /// UPnP: automatically open ports on the router
    pub upnp: upnp::tokio::Behaviour,
}
