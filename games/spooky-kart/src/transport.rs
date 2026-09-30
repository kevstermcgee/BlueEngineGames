//! Sockets for the game: the engine's raw-datagram transports behind one type, plus an in-memory network
//! with configurable delay and loss for tests. Nothing here knows the game's protocol.
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use vesper3d::viewer::net::{
    trusted_certificate, Datagram, DatagramTransport, Identity, PeerId, SecureSocket, TransportProfile, UdpTransport,
};

/// Either engine transport, chosen at run time.
pub struct AnyTransport(Box<dyn DatagramTransport>);

impl AnyTransport {
    pub fn new(inner: impl DatagramTransport + 'static) -> Self {
        Self(Box::new(inner))
    }
}

impl DatagramTransport for AnyTransport {
    fn send(&self, peer: PeerId, data: &[u8]) -> vesper3d::Result<usize> {
        self.0.send(peer, data)
    }
    fn payload_limit(&self, peer: PeerId) -> usize {
        self.0.payload_limit(peer)
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        self.0.receive()
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        self.0.local_addr()
    }
}

/// The server's socket: raw UDP for development, QUIC/TLS 1.3 (pinned certificate, key from
/// `BLUE_TLS_KEY_FILE`) for production, exactly as the engine's own server does it.
pub fn server_transport(profile: TransportProfile, address: &str) -> vesper3d::Result<AnyTransport> {
    Ok(match profile {
        TransportProfile::Development => AnyTransport::new(UdpTransport::bind(address)?),
        TransportProfile::Production => AnyTransport::new(SecureSocket::server(address.parse()?, Identity::load()?)?),
    })
}

/// A client's socket toward `server`.
pub fn client_transport(profile: TransportProfile, server: SocketAddr) -> vesper3d::Result<AnyTransport> {
    Ok(match profile {
        TransportProfile::Development => {
            AnyTransport::new(UdpTransport::bind(if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })?)
        }
        TransportProfile::Production => AnyTransport::new(SecureSocket::client(server, trusted_certificate()?)?),
    })
}

struct Pending {
    deliver_at: u64,
    from: SocketAddr,
    to: SocketAddr,
    data: Vec<u8>,
}

struct NetState {
    now: u64,
    latency: u64,
    jitter: u64,
    /// Chance of dropping a datagram, in thousandths.
    loss: u64,
    rng: u64,
    in_flight: VecDeque<Pending>,
    inboxes: HashMap<SocketAddr, VecDeque<Datagram>>,
    pub sent: u64,
    pub dropped: u64,
}

/// An in-memory network: `advance()` moves virtual time one tick, delivering what is due.
#[derive(Clone)]
pub struct LoopNet(Arc<Mutex<NetState>>);

impl LoopNet {
    /// `latency` and `jitter` are in ticks (one tick is 1/60 s); `loss` is the percentage dropped.
    pub fn new(latency: u64, jitter: u64, loss_percent: f32, seed: u64) -> Self {
        Self(Arc::new(Mutex::new(NetState {
            now: 0,
            latency,
            jitter,
            loss: (loss_percent * 10.) as u64,
            rng: seed | 1,
            in_flight: VecDeque::new(),
            inboxes: HashMap::new(),
            sent: 0,
            dropped: 0,
        })))
    }

    /// A new endpoint with the given address.
    pub fn endpoint(&self, address: SocketAddr) -> LoopEnd {
        self.0.lock().unwrap().inboxes.entry(address).or_default();
        LoopEnd { net: self.clone(), address }
    }

    /// Move time forward one tick and deliver every datagram that is now due.
    pub fn advance(&self) {
        let mut n = self.0.lock().unwrap();
        n.now += 1;
        let now = n.now;
        let mut later = VecDeque::new();
        while let Some(p) = n.in_flight.pop_front() {
            if p.deliver_at <= now {
                n.inboxes.entry(p.to).or_default().push_back(Datagram { peer: p.from, data: p.data });
            } else {
                later.push_back(p);
            }
        }
        n.in_flight = later;
    }

    /// (sent, dropped) datagram counts so far.
    pub fn counts(&self) -> (u64, u64) {
        let n = self.0.lock().unwrap();
        (n.sent, n.dropped)
    }
}

pub struct LoopEnd {
    net: LoopNet,
    address: SocketAddr,
}

impl DatagramTransport for LoopEnd {
    fn send(&self, peer: PeerId, data: &[u8]) -> vesper3d::Result<usize> {
        let mut n = self.net.0.lock().unwrap();
        n.sent += 1;
        n.rng ^= n.rng << 13;
        n.rng ^= n.rng >> 7;
        n.rng ^= n.rng << 17;
        if n.rng % 1000 < n.loss {
            n.dropped += 1;
            return Ok(data.len());
        }
        let jitter = if n.jitter > 0 { (n.rng >> 20) % (n.jitter + 1) } else { 0 };
        let deliver_at = n.now + n.latency + jitter;
        let from = self.address;
        n.in_flight.push_back(Pending { deliver_at, from, to: peer, data: data.to_vec() });
        Ok(data.len())
    }
    fn payload_limit(&self, _: PeerId) -> usize {
        1200
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        let mut n = self.net.0.lock().unwrap();
        Ok(n.inboxes.entry(self.address).or_default().drain(..).collect())
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        Ok(self.address)
    }
}
