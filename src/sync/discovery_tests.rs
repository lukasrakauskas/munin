use super::discovery::*;

#[tokio::test]
#[ignore = "requires host multicast/local-network permissions; run explicitly"]
async fn multicast_smoke_finds_two_real_local_endpoints() {
    use super::network::Event;
    use iroh::{Endpoint, endpoint::presets};
    use std::{sync::mpsc, time::Duration};
    let a = Endpoint::bind(presets::Minimal).await.unwrap();
    let b = Endpoint::bind(presets::Minimal).await.unwrap();
    let (ta, ra) = mpsc::channel();
    let (tb, rb) = mpsc::channel();
    let da = Discovery::start(a.clone(), "Nearby smoke A".into(), ta, None)
        .await
        .unwrap();
    let db = Discovery::start(b.clone(), "Nearby smoke B".into(), tb, None)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut found_a = false;
        let mut found_b = false;
        while !found_a || !found_b {
            for e in ra.try_iter() {
                if let Event::Nearby(p) = e
                    && p.addr.id == b.id()
                {
                    assert_eq!(p.name, "Nearby smoke B");
                    found_b = true;
                }
            }
            for e in rb.try_iter() {
                if let Event::Nearby(p) = e
                    && p.addr.id == a.id()
                {
                    assert_eq!(p.name, "Nearby smoke A");
                    found_a = true;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("multicast discovery must find both endpoints");
    drop(da);
    drop(db);
    a.close().await;
    b.close().await;
}

use iroh::{EndpointAddr, SecretKey, TransportAddr, address_lookup::EndpointInfo};

#[test]
#[ignore = "requires host multicast and local networking; run explicitly"]
fn nearby_worker_flow_pairs_and_syncs_without_an_invitation_code() {
    use crate::{state::State, sync};
    use std::time::{Duration, Instant};
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    std::fs::write(a.path().join("desktop.md"), "Desktop note").unwrap();
    std::fs::write(b.path().join("laptop.md"), "Laptop note").unwrap();
    let mut left = State::restore(Some(a.path().to_owned()), None);
    let mut right = State::restore(Some(b.path().to_owned()), None);
    left.sync.set_enabled(true).unwrap();
    right.sync.set_enabled(true).unwrap();
    left.sync.name_input.text = "Desktop".into();
    left.sync.rename_device().unwrap();
    right.sync.name_input.text = "Laptop".into();
    right.sync.rename_device().unwrap();
    let left_id = left.sync.device_id().unwrap();
    let right_id = right.sync.device_id().unwrap();
    left.sync.set_nearby(true).unwrap();
    right.sync.set_nearby(true).unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    let mut requested = false;
    let mut accepted = false;
    while left.sync.success.is_none() || right.sync.success.is_none() {
        sync::flush(&mut left).unwrap();
        sync::flush(&mut right).unwrap();
        if !requested
            && left.sync.nearby_devices.contains_key(&right_id)
            && right.sync.nearby_devices.contains_key(&left_id)
        {
            left.sync.connect_nearby(&right_id).unwrap();
            requested = true;
        }
        if !accepted && !left.sync.requests.is_empty() && !right.sync.requests.is_empty() {
            let l = left.sync.requests.values().next().unwrap();
            let r = right.sync.requests.values().next().unwrap();
            assert_eq!(l.fingerprint, r.fingerprint);
            assert!(left.sync.peers().is_empty() && right.sync.peers().is_empty());
            let li = l.id.clone();
            let ri = r.id.clone();
            left.sync.decide(&li, true).unwrap();
            right.sync.decide(&ri, true).unwrap();
            accepted = true;
        }
        assert!(
            Instant::now() < until,
            "nearby pairing/sync timed out: left={:?}, right={:?}",
            left.sync.error,
            right.sync.error
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(requested && accepted);
    assert!(left.sync.invitation.is_none() && right.sync.invitation.is_none());
    assert_eq!(
        std::fs::read_to_string(a.path().join("laptop.md")).unwrap(),
        "Laptop note"
    );
    assert_eq!(
        std::fs::read_to_string(b.path().join("desktop.md")).unwrap(),
        "Desktop note"
    );
    assert_eq!(left.sync.success.as_ref().unwrap().name, "Laptop");
    assert_eq!(right.sync.success.as_ref().unwrap().name, "Desktop");
    left.sync.set_nearby(false).unwrap();
    right.sync.set_nearby(false).unwrap();
    assert!(left.sync.requests.is_empty() && right.sync.requests.is_empty());
}

#[test]
fn discovery_refreshes_known_routes_but_never_grants_trust_or_renames_a_peer() {
    use super::network::{Peer, SessionState};
    use std::sync::{Arc, Mutex};
    let known = SecretKey::generate().public();
    let old = EndpointAddr::from(known).with_ip_addr("127.0.0.1:1111".parse().unwrap());
    let new = EndpointAddr::from(known).with_ip_addr("127.0.0.1:2222".parse().unwrap());
    let state = Arc::new(Mutex::new(SessionState {
        doc: automerge::AutoCommit::new(),
        auth: Default::default(),
        name: "This device".into(),
    }));
    state.lock().unwrap().auth.peers.insert(
        known,
        Peer {
            addr: old,
            name: "Trusted laptop".into(),
        },
    );
    refresh_trusted(
        &state,
        &Peer {
            addr: new.clone(),
            name: "Unverified advertised name".into(),
        },
    );
    assert_eq!(state.lock().unwrap().auth.peers[&known].addr, new);
    assert_eq!(
        state.lock().unwrap().auth.peers[&known].name,
        "Trusted laptop"
    );
    refresh_trusted(
        &state,
        &Peer {
            addr: SecretKey::generate().public().into(),
            name: "Unknown".into(),
        },
    );
    assert_eq!(state.lock().unwrap().auth.peers.len(), 1);
}

#[test]
fn mdns_publication_contains_only_direct_addresses_and_name() {
    let id = SecretKey::generate().public();
    let addr = EndpointAddr::from(id)
        .with_ip_addr("127.0.0.1:7777".parse().unwrap())
        .with_relay_url("https://relay.example".parse().unwrap());
    let data = publication(&addr, "My laptop").unwrap();
    assert_eq!(data.ip_addrs().count(), 1);
    assert_eq!(
        data.relay_urls().count(),
        0,
        "relay addresses and capabilities aren't LAN advertisements"
    );
    assert_eq!(data.user_data().unwrap().to_string(), "My laptop");
    assert!(publication(&addr, &"x".repeat(81)).is_err());
    assert!(publication(&addr, "bad\nname").is_err());
}

#[test]
fn discovered_candidates_reject_self_invalid_names_and_non_direct_addresses() {
    let own = SecretKey::generate().public();
    let other = SecretKey::generate().public();
    let addr = EndpointAddr::from(other).with_ip_addr("127.0.0.1:8888".parse().unwrap());
    let data = publication(&addr, "Desktop").unwrap();
    let info = EndpointInfo {
        endpoint_id: other,
        data,
    };
    let peer = candidate(&info, own).unwrap().unwrap();
    assert_eq!(peer.addr.id, other);
    assert_eq!(peer.name, "Desktop");
    assert!(
        peer.addr
            .addrs
            .iter()
            .all(|a| matches!(a, TransportAddr::Ip(_)))
    );
    assert!(candidate(&info, other).unwrap().is_none());
    let relay = EndpointAddr::from(other).with_relay_url("https://relay.example".parse().unwrap());
    let info = EndpointInfo {
        endpoint_id: other,
        data: publication(&relay, "Desktop").unwrap(),
    };
    assert!(candidate(&info, own).unwrap().is_none());
}
