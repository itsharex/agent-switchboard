use super::*;
use crate::provider_request::tests::record;
use asb_core::contracts::UpstreamProtocol;
use std::sync::atomic::{AtomicBool, Ordering};

fn issued(requests: &ProviderRequests) -> String {
    let record = record(UpstreamProtocol::Responses);
    let connection = ProviderRequestConnection::try_from(&record.profile).unwrap();
    requests
        .issue(
            &connection,
            PreparedSource::Saved {
                profile_id: record.profile.id,
                file_hash: record.file_hash,
            },
            "http://127.0.0.1:1/v1/responses".to_string(),
        )
        .unwrap()
        .request_id
}

struct Dropped(Arc<AtomicBool>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn cancellation_before_execution_removes_the_only_token() {
    let requests = ProviderRequests::default();
    let id = issued(&requests);
    assert!(tauri::async_runtime::block_on(requests.cancel(&id)).unwrap());
    assert!(requests.inspect(&id).is_err());
    assert!(requests.start(&id, std::future::pending()).is_err());
    assert!(!tauri::async_runtime::block_on(requests.cancel(&id)).unwrap());
}

#[test]
fn cancellation_waits_until_the_request_future_has_been_dropped() {
    let requests = ProviderRequests::default();
    let id = issued(&requests);
    let dropped = Arc::new(AtomicBool::new(false));
    let marker = Dropped(dropped.clone());
    let (task, active) = requests
        .start(&id, async move {
            let _marker = marker;
            std::future::pending().await
        })
        .unwrap();
    assert!(tauri::async_runtime::block_on(requests.cancel(&id)).unwrap());
    assert!(dropped.load(Ordering::SeqCst));
    assert!(tauri::async_runtime::block_on(join(&task)).is_none());
    assert!(active.finish().unwrap());
    assert!(requests.0.lock().unwrap().is_empty());
}

#[test]
fn one_preparation_can_start_only_one_request() {
    let requests = ProviderRequests::default();
    let id = issued(&requests);
    let (_, active) = requests.start(&id, std::future::pending()).unwrap();
    assert!(requests.start(&id, std::future::pending()).is_err());
    assert!(requests.inspect(&id).is_err());
    assert!(tauri::async_runtime::block_on(requests.cancel(&id)).unwrap());
    assert!(active.finish().unwrap());
}

#[test]
fn expired_preparations_are_rejected_and_reclaimed() {
    let requests = ProviderRequests::default();
    let id = issued(&requests);
    let other = issued(&requests);
    for entry in requests.0.lock().unwrap().values_mut() {
        if let Entry::Prepared(prepared) = entry {
            prepared.created_at -= PREPARATION_TTL + Duration::from_secs(1);
        }
    }
    assert!(requests.start(&id, std::future::pending()).is_err());
    let fresh = issued(&requests);
    assert!(requests.inspect(&other).is_err());
    assert!(requests.inspect(&fresh).is_ok());
    assert_eq!(requests.0.lock().unwrap().len(), 1);
}

#[test]
fn dropping_execution_aborts_network_work_and_removes_state() {
    let requests = ProviderRequests::default();
    let id = issued(&requests);
    let dropped = Arc::new(AtomicBool::new(false));
    let marker = Dropped(dropped.clone());
    let (task, active) = requests
        .start(&id, async move {
            let _marker = marker;
            std::future::pending().await
        })
        .unwrap();
    drop(active);
    assert!(tauri::async_runtime::block_on(join(&task))
        .unwrap()
        .is_err());
    assert!(dropped.load(Ordering::SeqCst));
    assert!(requests.0.lock().unwrap().is_empty());
}
