//! Ephemeral LAN discovery. Only public identity, direct IPs and a display name are advertised.
use super::{
    network::{Event, Peer, Shared},
    store::err,
};
use futures_util::StreamExt;
use iroh::{
    Endpoint, EndpointAddr, EndpointId, TransportAddr,
    address_lookup::{AddressLookup, EndpointData, EndpointInfo},
};
use iroh_mdns_address_lookup::{DiscoveryEvent, MdnsAddressLookup};
use std::{
    io,
    sync::mpsc,
    time::{Duration, Instant},
};

pub const WINDOW: Duration = Duration::from_secs(600);
pub fn publication(addr: &EndpointAddr, name: &str) -> io::Result<EndpointData> {
    if name.trim().is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
        return Err(err("Invalid nearby device name"));
    }
    let mut data = EndpointData::from_iter(
        addr.addrs
            .iter()
            .filter(|a| matches!(a, TransportAddr::Ip(_)))
            .take(16)
            .cloned(),
    );
    data.set_user_data(Some(name.parse().map_err(err)?));
    Ok(data)
}
pub fn candidate(info: &EndpointInfo, own: EndpointId) -> io::Result<Option<Peer>> {
    if info.endpoint_id == own {
        return Ok(None);
    }
    let name = info
        .data
        .user_data()
        .ok_or_else(|| err("Nearby device has no name"))?
        .to_string();
    let addr = info.to_endpoint_addr();
    let data = publication(&addr, &name)?;
    let addrs: Vec<_> = data
        .ip_addrs()
        .filter(|a| !a.ip().is_unspecified() && !a.ip().is_multicast())
        .copied()
        .collect();
    if addrs.is_empty() {
        return Ok(None);
    }
    Ok(Some(Peer {
        addr: addrs
            .into_iter()
            .fold(EndpointAddr::from(info.endpoint_id), |addr, ip| {
                addr.with_ip_addr(ip)
            }),
        name,
    }))
}
pub fn refresh_trusted(shared: &Shared, peer: &Peer) {
    if let Some(trusted) = shared.lock().unwrap().auth.peers.get_mut(&peer.addr.id) {
        // Routes are untrusted hints; iroh still authenticates the endpoint key.
        // Never grant access or replace a trusted display name from an advertisement.
        trusted.addr = peer.addr.clone();
    }
}
pub struct Discovery {
    pub started: Instant,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Discovery {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Discovery {
    pub async fn start(
        endpoint: Endpoint,
        name: String,
        events: mpsc::Sender<Event>,
        shared: Option<Shared>,
    ) -> io::Result<Self> {
        let mdns = MdnsAddressLookup::builder()
            .service_name("munin-sync-v1")
            .build(endpoint.id())
            .map_err(err)?;
        let mut stream = mdns.subscribe().await;
        mdns.publish(&publication(&endpoint.addr(), &name)?);
        let task = tokio::spawn(async move {
            let mut publish = tokio::time::interval(Duration::from_secs(2));
            loop {
                tokio::select! {
                    _ = publish.tick() => if let Ok(data) = publication(&endpoint.addr(), &name) { mdns.publish(&data); },
                    event = stream.next() => match event {
                        Some(DiscoveryEvent::Discovered { endpoint_info, .. }) => {
                            if let Ok(Some(peer)) = candidate(&endpoint_info, endpoint.id()) {
                                if let Some(shared) = &shared { refresh_trusted(shared, &peer); }
                                let _ = events.send(Event::Nearby(peer));
                            }
                        },
                        Some(DiscoveryEvent::Expired { endpoint_id }) => { let _ = events.send(Event::NearbyExpired(endpoint_id)); },
                        Some(_) => {},
                        None => break,
                    }
                }
            }
        });
        Ok(Self {
            started: Instant::now(),
            task,
        })
    }
}
