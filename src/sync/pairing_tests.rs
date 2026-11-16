use super::{
    network::{Event, SessionState},
    pairing::*,
};
use automerge::AutoCommit;
use iroh::{Endpoint, SecretKey, endpoint::presets};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[test]
fn fingerprints_are_symmetric_and_bind_both_identities_and_nonces() {
    let a = SecretKey::generate().public();
    let b = SecretKey::generate().public();
    let left = verification(a, &"a".repeat(64), b, &"b".repeat(64));
    assert_eq!(left, verification(b, &"b".repeat(64), a, &"a".repeat(64)));
    assert_ne!(left, verification(a, &"c".repeat(64), b, &"b".repeat(64)));
    assert_ne!(
        left,
        verification(
            a,
            &"a".repeat(64),
            SecretKey::generate().public(),
            &"b".repeat(64)
        )
    );
}

#[test]
fn nonce_commitments_bind_the_endpoint_and_reject_adaptive_or_replayed_reveals() {
    let a = SecretKey::generate().public();
    let b = SecretKey::generate().public();
    let nonce = "a".repeat(64);
    let commitment = nonce_commitment(a, &nonce);
    assert!(verify_nonce(a, &nonce, &commitment).is_ok());
    assert!(verify_nonce(b, &nonce, &commitment).is_err());
    assert!(verify_nonce(a, &"b".repeat(64), &commitment).is_err());
    assert!(verify_nonce(a, "short", &commitment).is_err());
    assert!(verify_nonce(a, &"z".repeat(64), &commitment).is_err());
}

#[test]
fn a_nearby_grant_revokes_any_invitation_that_the_ui_will_hide() {
    let state = Arc::new(Mutex::new(SessionState {
        doc: AutoCommit::new(),
        auth: Default::default(),
        name: "Host".into(),
    }));
    state.lock().unwrap().auth.invite = Some(super::network::Invitation {
        token: "old-code".into(),
        expires: Instant::now() + Duration::from_secs(600),
    });
    let (tx, _) = mpsc::channel();
    super::network::grant(
        &state,
        super::network::Peer {
            addr: SecretKey::generate().public().into(),
            name: "Approved nearby".into(),
        },
        &tx,
    )
    .unwrap();
    assert!(
        state.lock().unwrap().auth.invite.is_none(),
        "a hidden outstanding invitation must not remain redeemable"
    );
    assert!(
        !state
            .lock()
            .unwrap()
            .auth
            .admit(SecretKey::generate().public(), Some("old-code"))
    );
}

#[test]
fn stopped_expired_and_replayed_decisions_cannot_grant_access() {
    let gate = Pairing::new(true);
    let peer = super::network::Peer {
        addr: SecretKey::generate().public().into(),
        name: "Laptop".into(),
    };
    let request = Request {
        id: "first".into(),
        peer: peer.clone(),
        fingerprint: "ABCD EFGH IJKL".into(),
        outgoing: false,
        expires: Instant::now() + Duration::from_secs(90),
    };
    let mut approval = gate.register(request.clone()).unwrap();
    gate.decide("first", true).unwrap();
    assert!(approval.try_recv().unwrap());
    assert!(gate.decide("first", true).is_err());
    let mut expired = request.clone();
    expired.id = "expired".into();
    expired.expires = Instant::now() - Duration::from_secs(1);
    assert!(gate.register(expired).is_err());
    let mut approval = gate
        .register(Request {
            id: "second".into(),
            ..request
        })
        .unwrap();
    assert!(
        gate.decide("first", true).is_err(),
        "old approval cannot target new request"
    );
    gate.stop();
    assert!(gate.decide("second", true).is_err());
    assert!(approval.try_recv().is_err());
    assert!(
        gate.register(Request {
            id: "third".into(),
            peer,
            fingerprint: String::new(),
            outgoing: false,
            expires: Instant::now() + Duration::from_secs(90)
        })
        .is_err()
    );
}

async fn request(events: &mpsc::Receiver<Event>) -> Request {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::PairRequest(r)) = events.try_recv() {
                return r;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn nearby_pairing_requires_both_approvals_and_never_exchanges_documents() {
    let a = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let b = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let left = Arc::new(Mutex::new(SessionState {
        doc: AutoCommit::new(),
        auth: Default::default(),
        name: "Laptop".into(),
    }));
    let right = Arc::new(Mutex::new(SessionState {
        doc: AutoCommit::new(),
        auth: Default::default(),
        name: "Desktop".into(),
    }));
    let (tx_a, rx_a) = mpsc::channel();
    let (tx_b, rx_b) = mpsc::channel();
    let gate_a = Pairing::new(true);
    let gate_b = Pairing::new(true);
    let server = async {
        let c = a.accept().await.unwrap().await.unwrap();
        serve(&a, c, left.clone(), gate_a.clone(), tx_a).await
    };
    let client = dial(&b, a.addr(), right.clone(), gate_b.clone(), tx_b);
    let consent = async {
        let ra = request(&rx_a).await;
        let rb = request(&rx_b).await;
        assert_eq!(ra.fingerprint, rb.fingerprint);
        assert_eq!(ra.peer.name, "Desktop");
        assert_eq!(rb.peer.name, "Laptop");
        assert!(!ra.outgoing && rb.outgoing);
        assert!(left.lock().unwrap().auth.peers.is_empty());
        assert!(right.lock().unwrap().auth.peers.is_empty());
        gate_a.decide(&ra.id, true).unwrap();
        tokio::task::yield_now().await;
        assert!(
            left.lock().unwrap().auth.peers.is_empty(),
            "one approval is insufficient"
        );
        assert!(right.lock().unwrap().auth.peers.is_empty());
        gate_b.decide(&rb.id, true).unwrap();
    };
    let (s, c, _) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(server, client, consent)
    })
    .await
    .unwrap();
    s.unwrap();
    c.unwrap();
    assert!(left.lock().unwrap().auth.peers.contains_key(&b.id()));
    assert!(right.lock().unwrap().auth.peers.contains_key(&a.id()));
    assert!(
        !rx_a
            .try_iter()
            .chain(rx_b.try_iter())
            .any(|e| matches!(e, Event::Document(_) | Event::Synced(_))),
        "pairing-only ALPN must never carry notes or claim a sync"
    );
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn declining_nearby_request_cancels_both_sides_without_trust() {
    rejected(false).await;
}
#[tokio::test]
async fn stopping_discovery_after_one_approval_still_cancels_both_sides() {
    rejected(true).await;
}
async fn rejected(stop_after_accept: bool) {
    let a = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let b = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let left = Arc::new(Mutex::new(SessionState {
        doc: AutoCommit::new(),
        auth: Default::default(),
        name: "Laptop".into(),
    }));
    let right = Arc::new(Mutex::new(SessionState {
        doc: AutoCommit::new(),
        auth: Default::default(),
        name: "Desktop".into(),
    }));
    let (tx_a, rx_a) = mpsc::channel();
    let (tx_b, rx_b) = mpsc::channel();
    let gate_a = Pairing::new(true);
    let gate_b = Pairing::new(true);
    let server = async {
        let c = a.accept().await.unwrap().await.unwrap();
        serve(&a, c, left.clone(), gate_a.clone(), tx_a).await
    };
    let client = dial(&b, a.addr(), right.clone(), gate_b.clone(), tx_b);
    let reject = async {
        let ra = request(&rx_a).await;
        let _ = request(&rx_b).await;
        if stop_after_accept {
            gate_a.decide(&ra.id, true).unwrap();
            gate_b.stop();
        } else {
            gate_a.decide(&ra.id, false).unwrap();
        }
    };
    let (s, c, _) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(server, client, reject)
    })
    .await
    .unwrap();
    assert!(s.is_err() && c.is_err());
    assert!(left.lock().unwrap().auth.peers.is_empty());
    assert!(right.lock().unwrap().auth.peers.is_empty());
    a.close().await;
    b.close().await;
}
