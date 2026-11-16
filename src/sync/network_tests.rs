use super::network::*;
use automerge::{AutoCommit, ROOT, ReadDoc, transaction::Transactable};
use iroh::{Endpoint, SecretKey, endpoint::presets};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[test]
fn invitation_validation_expiry_and_single_use_authorization() {
    let key = SecretKey::generate();
    let ticket = Ticket {
        version: 1,
        addr: key.public().into(),
        token: "a".repeat(64),
    };
    assert_eq!(
        Ticket::parse(&ticket.encode().unwrap()).unwrap().addr.id,
        key.public()
    );
    assert!(Ticket::parse("garbage").is_err());
    assert!(Ticket::parse(&"x".repeat(20_000)).is_err());
    let mut auth = Authorization::default();
    let outsider = SecretKey::generate().public();
    assert!(!auth.admit(outsider, None));
    auth.invite = Some(Invitation {
        token: ticket.token.clone(),
        expires: Instant::now() - Duration::from_secs(1),
    });
    assert!(!auth.admit(outsider, Some(&ticket.token)));
    auth.invite = Some(Invitation {
        token: ticket.token.clone(),
        expires: Instant::now() + Duration::from_secs(600),
    });
    assert!(!auth.admit(outsider, Some("wrong")));
    assert!(auth.admit(outsider, Some(&ticket.token)));
    assert!(auth.admit(outsider, None));
    assert!(!auth.admit(SecretKey::generate().public(), Some(&ticket.token)));
    auth.peers.remove(&outsider);
    assert!(!auth.admit(outsider, None));
}

#[tokio::test]
async fn real_iroh_pair_and_incremental_exchange_rejects_unapproved_device() {
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
    let c = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let (tx_a, rx_a) = mpsc::channel();
    let (tx_b, rx_b) = mpsc::channel();
    let mut doc = AutoCommit::new();
    let record = doc
        .put_object(
            ROOT,
            "0123456789abcdef0123456789abcdef",
            automerge::ObjType::Map,
        )
        .unwrap();
    doc.put(&record, "path", "hello.md").unwrap();
    doc.put(&record, "deleted", false).unwrap();
    let text = doc
        .put_object(&record, "text", automerge::ObjType::Text)
        .unwrap();
    doc.update_text(&text, "hello").unwrap();
    let left = Arc::new(Mutex::new(SessionState {
        doc,
        auth: Authorization {
            peers: BTreeMap::new(),
            invite: Some(Invitation {
                token: "secret".into(),
                expires: Instant::now() + Duration::from_secs(600),
            }),
        },
        name: "Laptop".into(),
    }));
    let right = Arc::new(Mutex::new(SessionState {
        doc: AutoCommit::new(),
        auth: Authorization::default(),
        name: "Desktop".into(),
    }));
    let ticket = Ticket {
        version: 1,
        addr: a.addr(),
        token: "secret".into(),
    };
    let server = async {
        let conn = a.accept().await.unwrap().await.unwrap();
        serve(&a, conn, left.clone(), tx_a.clone()).await.unwrap();
    };
    let client = dial(
        &b,
        ticket.addr.clone(),
        Some(ticket.token.clone()),
        right.clone(),
        tx_b.clone(),
    );
    let (_, result) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    result.unwrap();
    assert_eq!(
        super::store::document_notes(&right.lock().unwrap().doc).unwrap()["hello.md"].text,
        "hello"
    );
    assert!(rx_a.try_iter().any(|e| matches!(e, Event::Peer(_))));
    assert!(rx_b.try_iter().any(|e| matches!(e, Event::Peer(_))));
    // A subsequent session carries edits in both directions without re-pairing.
    {
        let mut state = left.lock().unwrap();
        state.doc.update_text(&text, "hello from laptop").unwrap();
    }
    {
        let mut state = right.lock().unwrap();
        state.doc.update_text(&text, "desktop says hello").unwrap();
    }
    let server = async {
        let conn = a.accept().await.unwrap().await.unwrap();
        serve(&a, conn, left.clone(), tx_a.clone()).await.unwrap();
    };
    let client = dial(&b, a.addr(), None, right.clone(), tx_b.clone());
    let (_, result) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    result.unwrap();
    let left_text = super::store::document_notes(&left.lock().unwrap().doc).unwrap()["hello.md"]
        .text
        .clone();
    let right_text = super::store::document_notes(&right.lock().unwrap().doc).unwrap()["hello.md"]
        .text
        .clone();
    assert_eq!(left_text, "desktop says hello from laptop");
    assert_eq!(right_text, "desktop says hello from laptop");
    // A separate authenticated endpoint still cannot read the document, even with a consumed invite.
    let stranger = Arc::new(Mutex::new(SessionState {
        doc: AutoCommit::new(),
        auth: Authorization::default(),
        name: "Stranger".into(),
    }));
    let serve_rejected = async {
        let conn = a.accept().await.unwrap().await.unwrap();
        assert!(serve(&a, conn, left.clone(), tx_a).await.is_err());
    };
    let connect_rejected = dial(&c, a.addr(), Some("secret".into()), stranger.clone(), tx_b);
    let (_, result) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(serve_rejected, connect_rejected)
    })
    .await
    .unwrap();
    assert!(result.is_err());
    assert!(stranger.lock().unwrap().doc.keys(ROOT).next().is_none());
    a.close().await;
    b.close().await;
    c.close().await;
}
