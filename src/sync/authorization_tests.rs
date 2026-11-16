use super::network::*;
use automerge::{AutoCommit, ROOT, transaction::Transactable};
use iroh::{Endpoint, endpoint::presets};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[tokio::test]
async fn consumed_invitation_device_is_visible_even_if_its_document_is_invalid() {
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
    let (tx_a, rx_a) = mpsc::channel();
    let (tx_b, _) = mpsc::channel();
    let left = Arc::new(Mutex::new(SessionState {
        doc: AutoCommit::new(),
        auth: Authorization {
            invite: Some(Invitation {
                token: "secret".into(),
                expires: Instant::now() + Duration::from_secs(600),
            }),
            ..Default::default()
        },
        name: "Host".into(),
    }));
    let mut malformed = AutoCommit::new();
    malformed
        .put(ROOT, "invalid-note", "not a note record")
        .unwrap();
    let right = Arc::new(Mutex::new(SessionState {
        doc: malformed,
        auth: Authorization::default(),
        name: "Guest".into(),
    }));
    let server = async {
        let conn = a.accept().await.unwrap().await.unwrap();
        assert!(serve(&a, conn, left.clone(), tx_a).await.is_err());
    };
    let client = dial(&b, a.addr(), Some("secret".into()), right, tx_b);
    tokio::time::timeout(Duration::from_secs(10), async {
        let _ = tokio::join!(server, client);
    })
    .await
    .unwrap();
    assert!(
        rx_a.try_iter()
            .any(|e| matches!(e, Event::Peer(p) if p.addr.id == b.id())),
        "an admitted endpoint must be removable from settings, even after a failed exchange"
    );
    assert!(
        super::store::document_notes(&left.lock().unwrap().doc)
            .unwrap()
            .is_empty()
    );
    a.close().await;
    b.close().await;
}
