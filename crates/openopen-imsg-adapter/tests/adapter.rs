use openopen_imsg_adapter::{
    AdapterConfig, AdapterError, ChatId, HISTORY_SUPPORTS_SINCE_ROWID, HistoryRequest, ImsgAdapter,
    ImsgEvent, InboundPairing, InboundRejection, ListChatsRequest, Message, MessageCursor,
    OPENOPEN_IMESSAGE_PREFIX, OutboundRecovery, OutboundRecoveryRequest,
    SEND_HAS_CALLER_IDEMPOTENCY_KEY, SendRequest, SendState, WATCH_CURSOR_IS_EXCLUSIVE,
    WatchRequest, normalize_inbound,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const _: () = {
    assert!(!SEND_HAS_CALLER_IDEMPOTENCY_KEY);
    assert!(!HISTORY_SUPPORTS_SINCE_ROWID);
    assert!(WATCH_CURSOR_IS_EXCLUSIVE);
};

static NEXT_FAKE_ID: AtomicU64 = AtomicU64::new(1);

struct FakeImsg {
    root: PathBuf,
    executable: PathBuf,
    log: PathBuf,
}

impl FakeImsg {
    fn new(mode: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let fake_id = NEXT_FAKE_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "openopen-imsg-adapter-{}-{nonce}-{fake_id}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let executable = root.join("imsg-fake");
        let log = root.join("requests.log");
        let script = fake_script(mode, &log);
        fs::write(&executable, script).unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&executable, permissions).unwrap();
        Self {
            root,
            executable,
            log,
        }
    }

    fn config(&self) -> AdapterConfig {
        AdapterConfig::new(&self.executable)
            .with_request_timeout(Duration::from_secs(2))
            .with_shutdown_timeout(Duration::from_millis(500))
    }
}

impl Drop for FakeImsg {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fake_script(mode: &str, log: &std::path::Path) -> String {
    let mode = serde_json::to_string(mode).unwrap();
    let log = serde_json::to_string(&log.display().to_string()).unwrap();
    format!(
        r"#!/usr/bin/env python3
import json
import sys
import time

MODE = {mode}
LOG = {log}

def emit(value):
    sys.stdout.write(json.dumps(value, separators=(',', ':')) + '\n')
    sys.stdout.flush()

if len(sys.argv) != 2 or sys.argv[1] != 'rpc':
    sys.exit(7)

open(LOG, 'w').close()
for line in sys.stdin:
    with open(LOG, 'a') as output:
        output.write(line)
    request = json.loads(line)
    request_id = request['id']
    method = request['method']
    if MODE == 'timeout':
        time.sleep(2)
        continue
    if MODE == 'oversized':
        sys.stdout.write('x' * (2 * 1024 * 1024) + '\n')
        sys.stdout.flush()
        continue
    if MODE == 'rpc_error':
        emit({{'jsonrpc':'2.0','id':request_id,'error':{{'code':-32602,'message':'Invalid params','data':'denied'}}}})
        continue
    if MODE == 'recovery' and method == 'send':
        continue
    if MODE == 'recovery' and method == 'messages.history':
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'messages':[{{'id':101,'chat_id':42,'chat_identifier':'pairing','chat_guid':'iMessage;+;chat','chat_name':'Owner','participants':['+15551234567'],'is_group':False,'guid':'recovered-guid','sender':'','is_from_me':True,'text':'OpenOpen · AI\nWorking on it','created_at':'2026-07-14T00:02:00Z'}}]}}}})
        continue
    if MODE == 'ambiguous' and method == 'messages.history':
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'messages':[{{'id':102,'chat_id':42,'chat_identifier':'pairing','chat_guid':'iMessage;+;chat','chat_name':'Owner','participants':['+15551234567'],'is_group':False,'guid':'duplicate-2','sender':'','is_from_me':True,'text':'OpenOpen · AI\nWorking on it','created_at':'2026-07-14T00:03:00Z'}},{{'id':101,'chat_id':42,'chat_identifier':'pairing','chat_guid':'iMessage;+;chat','chat_name':'Owner','participants':['+15551234567'],'is_group':False,'guid':'duplicate-1','sender':'','is_from_me':True,'text':'OpenOpen · AI\nWorking on it','created_at':'2026-07-14T00:02:00Z'}}]}}}})
        continue
    if method == 'chats.list':
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'chats':[{{'id':42,'identifier':'pairing','guid':'iMessage;+;chat','name':'Owner','service':'iMessage','last_message_at':'2026-07-14T00:00:00Z','participants':['+15551234567'],'is_group':False}}]}}}})
    elif method == 'messages.history':
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'messages':[{{'id':90,'chat_id':42,'chat_identifier':'pairing','chat_guid':'iMessage;+;chat','chat_name':'Owner','participants':['+15551234567'],'is_group':False,'guid':'inbound-guid','sender':'+15551234567','is_from_me':False,'text':'plan my day','created_at':'2026-07-14T00:01:00Z'}}]}}}})
    elif method == 'watch.subscribe':
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'subscription':1}}}})
        emit({{'jsonrpc':'2.0','method':'message','params':{{'subscription':1,'message':{{'id':91,'chat_id':42,'chat_identifier':'pairing','chat_guid':'iMessage;+;chat','chat_name':'Owner','participants':['+15551234567'],'is_group':False,'guid':'watch-guid','sender':'+15551234567','is_from_me':False,'text':'@OpenOpen help','created_at':'2026-07-14T00:02:00Z'}}}}}})
    elif method == 'send':
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'ok':True,'id':92,'guid':'sent-guid','message_id':'sent-guid','chat_guid':'iMessage;+;chat','service':'iMessage','transport':'applescript'}}}})
    elif method == 'message.send_status':
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'ok':True,'guid':'sent-guid','send_state':'delivered','service':'iMessage','checked_at':'2026-07-14T00:03:00Z','delivered_at':'2026-07-14T00:02:59Z','status_fields':{{'is_sent':True,'is_delivered':True,'is_finished':True,'error':0,'date_delivered':'2026-07-14T00:02:59Z','date_read':None,'is_delayed':False,'is_prepared':False,'is_pending_satellite_send':False,'was_downgraded':False}}}}}})
    elif method == 'watch.unsubscribe':
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'ok':True}}}})
"
    )
}

#[test]
fn spawned_child_exposes_identity_without_receiving_rpc_bytes() {
    let fake = FakeImsg::new("normal");
    let adapter = ImsgAdapter::spawn(&fake.config()).unwrap();
    assert!(adapter.process_identifier().unwrap() > 0);
    for _ in 0..500 {
        if fake.log.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        fake.log.exists(),
        "spawned child never reached its stdio request loop"
    );
    assert_eq!(fs::read_to_string(&fake.log).unwrap(), "");
    let _ = adapter
        .list_chats(ListChatsRequest {
            limit: 1,
            unread_only: false,
        })
        .unwrap();
    assert!(
        fs::read_to_string(&fake.log)
            .unwrap()
            .contains("chats.list")
    );
    adapter.shutdown().unwrap();
}

#[test]
fn typed_basic_surface_is_scoped_and_forces_non_bridge_send() {
    let fake = FakeImsg::new("normal");
    let adapter = ImsgAdapter::spawn(&fake.config()).unwrap();
    let chat_id = ChatId::new(42).unwrap();

    let chats = adapter
        .list_chats(ListChatsRequest {
            limit: 20,
            unread_only: false,
        })
        .unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].id, chat_id);

    let history = adapter
        .history(HistoryRequest { chat_id, limit: 50 })
        .unwrap();
    assert_eq!(history[0].guid, "inbound-guid");

    let subscription = adapter
        .subscribe(WatchRequest {
            chat_id,
            since_rowid: Some(MessageCursor::new(90).unwrap()),
        })
        .unwrap();
    let event = adapter
        .recv_event_timeout(Duration::from_millis(250))
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        ImsgEvent::Message {
            subscription: event_subscription,
            ref message,
        } if event_subscription == subscription && message.guid == "watch-guid"
    ));

    let sent = adapter
        .execute_send(&SendRequest {
            chat_id,
            body: "Working on it".into(),
        })
        .unwrap();
    assert_eq!(sent.guid.as_deref(), Some("sent-guid"));
    let status = adapter.send_status("sent-guid").unwrap();
    assert_eq!(status.send_state, SendState::Delivered);
    adapter.unsubscribe(subscription).unwrap();
    adapter.shutdown().unwrap();

    let requests = fs::read_to_string(&fake.log).unwrap();
    assert!(requests.contains("\"chat_id\":42"));
    assert!(requests.contains("\"since_rowid\":90"));
    assert!(requests.contains("\"transport\":\"applescript\""));
    assert!(requests.contains(&format!("{OPENOPEN_IMESSAGE_PREFIX}\\nWorking on it")));
    assert!(!requests.contains("bridge"));
    assert!(!requests.contains("\"file\""));
}

#[test]
fn rpc_errors_are_typed_and_do_not_widen_the_request() {
    let fake = FakeImsg::new("rpc_error");
    let adapter = ImsgAdapter::spawn(&fake.config()).unwrap();
    let error = adapter
        .history(HistoryRequest {
            chat_id: ChatId::new(42).unwrap(),
            limit: 50,
        })
        .unwrap_err();
    assert_eq!(
        error,
        AdapterError::Rpc {
            code: -32602,
            message: "Invalid params".into(),
            data: Some("denied".into()),
        }
    );
    adapter.shutdown().unwrap();
}

#[test]
fn oversized_stdout_fails_closed() {
    let fake = FakeImsg::new("oversized");
    let adapter = ImsgAdapter::spawn(&fake.config()).unwrap();
    let error = adapter
        .list_chats(ListChatsRequest {
            limit: 20,
            unread_only: false,
        })
        .unwrap_err();
    assert_eq!(error, AdapterError::ResponseFrameTooLarge);
    adapter.shutdown().unwrap();
}

#[test]
fn request_timeout_faults_the_adapter_until_shutdown() {
    let fake = FakeImsg::new("timeout");
    let config = fake
        .config()
        .with_request_timeout(Duration::from_millis(100));
    let adapter = ImsgAdapter::spawn(&config).unwrap();
    let error = adapter
        .list_chats(ListChatsRequest {
            limit: 20,
            unread_only: false,
        })
        .unwrap_err();
    assert_eq!(error, AdapterError::RequestTimeout);
    let second = adapter
        .list_chats(ListChatsRequest {
            limit: 20,
            unread_only: false,
        })
        .unwrap_err();
    assert_eq!(second, AdapterError::RequestTimeout);
    adapter.shutdown().unwrap();
}

#[test]
fn invalid_inputs_are_rejected_before_process_io() {
    assert_eq!(ChatId::new(0), Err(AdapterError::InvalidChatId));
    assert_eq!(MessageCursor::new(-1), Err(AdapterError::InvalidCursor));
    let fake = FakeImsg::new("normal");
    let adapter = ImsgAdapter::spawn(&fake.config()).unwrap();
    let error = adapter
        .execute_send(&SendRequest {
            chat_id: ChatId::new(42).unwrap(),
            body: format!("{OPENOPEN_IMESSAGE_PREFIX} already prefixed"),
        })
        .unwrap_err();
    assert_eq!(error, AdapterError::InvalidOutboundMessage);
    assert_eq!(adapter.send_status(" "), Err(AdapterError::InvalidGuid));
    adapter.shutdown().unwrap();
    assert!(fs::read_to_string(&fake.log).unwrap_or_default().is_empty());
}

#[test]
fn executable_validation_rejects_symlink_leaf_parent_and_alias_components() {
    let fake = FakeImsg::new("normal");

    assert_eq!(
        ImsgAdapter::spawn(&AdapterConfig::new(&fake.root)).unwrap_err(),
        AdapterError::InvalidExecutable
    );
    let non_executable = fake.root.join("not-executable");
    fs::write(&non_executable, "plain file").unwrap();
    assert_eq!(
        ImsgAdapter::spawn(&AdapterConfig::new(&non_executable)).unwrap_err(),
        AdapterError::InvalidExecutable
    );

    let leaf_link = fake.root.join("imsg-link");
    symlink(&fake.executable, &leaf_link).unwrap();
    assert_eq!(
        ImsgAdapter::spawn(&AdapterConfig::new(&leaf_link)).unwrap_err(),
        AdapterError::InvalidExecutable
    );

    let parent_link = fake.root.with_extension("link");
    symlink(&fake.root, &parent_link).unwrap();
    let linked_child = parent_link.join("imsg-fake");
    assert_eq!(
        ImsgAdapter::spawn(&AdapterConfig::new(&linked_child)).unwrap_err(),
        AdapterError::InvalidExecutable
    );
    fs::remove_file(parent_link).unwrap();

    let nested = fake.root.join("nested");
    fs::create_dir(&nested).unwrap();
    let aliased = nested.join("..").join("imsg-fake");
    assert_eq!(
        ImsgAdapter::spawn(&AdapterConfig::new(&aliased)).unwrap_err(),
        AdapterError::InvalidExecutable
    );
}

#[test]
fn restart_recovery_is_read_only_and_never_reissues_send() {
    let fake = FakeImsg::new("recovery");
    let chat_id = ChatId::new(42).unwrap();
    let send = SendRequest {
        chat_id,
        body: "Working on it".into(),
    };
    let first = ImsgAdapter::spawn(&fake.config()).unwrap();
    first
        .list_chats(ListChatsRequest {
            limit: 1,
            unread_only: false,
        })
        .unwrap();
    assert_eq!(first.execute_send(&send), Err(AdapterError::RequestTimeout));
    first.shutdown().unwrap();
    let first_requests = fs::read_to_string(&fake.log).unwrap();
    assert_eq!(first_requests.matches("\"method\":\"send\"").count(), 1);

    let restarted = ImsgAdapter::spawn(&fake.config()).unwrap();
    let recovered = restarted
        .recover_outbound(&OutboundRecoveryRequest {
            chat_id,
            body: send.body,
            after_rowid: MessageCursor::new(100).unwrap(),
            history_limit: 50,
        })
        .unwrap();
    assert!(matches!(
        recovered,
        OutboundRecovery::SingleLocalCandidate(ref observation)
            if observation.rowid == 101
                && observation.guid.as_deref() == Some("recovered-guid")
    ));
    restarted.shutdown().unwrap();
    let recovery_requests = fs::read_to_string(&fake.log).unwrap();
    assert!(recovery_requests.contains("\"method\":\"messages.history\""));
    assert!(!recovery_requests.contains("\"method\":\"send\""));
    assert!(!recovery_requests.contains("since_rowid"));
}

#[test]
fn ambiguous_history_never_collapses_to_a_delivery_claim() {
    let fake = FakeImsg::new("ambiguous");
    let adapter = ImsgAdapter::spawn(&fake.config()).unwrap();
    let result = adapter
        .recover_outbound(&OutboundRecoveryRequest {
            chat_id: ChatId::new(42).unwrap(),
            body: "Working on it".into(),
            after_rowid: MessageCursor::new(100).unwrap(),
            history_limit: 50,
        })
        .unwrap();
    assert!(matches!(
        result,
        OutboundRecovery::AmbiguousLocalCandidates(ref observations)
            if observations.iter().map(|item| item.rowid).collect::<Vec<_>>() == [101, 102]
    ));
    adapter.shutdown().unwrap();
    assert!(
        !fs::read_to_string(&fake.log)
            .unwrap()
            .contains("\"method\":\"send\"")
    );
}

#[test]
fn inbound_normalization_requires_exact_pairing_owner_direction_and_address() {
    let pairing = InboundPairing::new(ChatId::new(42).unwrap(), "+15551234567").unwrap();
    let message = inbound_message("@OpenOpen: plan my day");
    let normalized = normalize_inbound(&pairing, &message).unwrap();
    assert_eq!(normalized.chat_id, pairing.chat_id);
    assert_eq!(normalized.sender, pairing.owner_sender);
    assert_eq!(normalized.source_rowid, 91);
    assert_eq!(normalized.source_guid, "watch-guid");
    assert_eq!(normalized.body, "plan my day");

    let mut rejected = message.clone();
    rejected.chat_id = ChatId::new(43).unwrap();
    assert_eq!(
        normalize_inbound(&pairing, &rejected),
        Err(InboundRejection::ChatNotPaired)
    );
    rejected = message.clone();
    rejected.sender = "+15550000000".into();
    assert_eq!(
        normalize_inbound(&pairing, &rejected),
        Err(InboundRejection::SenderNotOwner)
    );
    rejected = message.clone();
    rejected.is_from_me = true;
    assert_eq!(
        normalize_inbound(&pairing, &rejected),
        Err(InboundRejection::FromLocalUser)
    );
    assert_eq!(
        normalize_inbound(&pairing, &inbound_message("@OpenOpenFake do it")),
        Err(InboundRejection::NotAddressed)
    );
    assert_eq!(
        normalize_inbound(&pairing, &inbound_message("@OpenOpen   ")),
        Err(InboundRejection::EmptyBody)
    );
}

fn inbound_message(text: &str) -> Message {
    Message {
        id: 91,
        chat_id: ChatId::new(42).unwrap(),
        chat_identifier: "pairing".into(),
        chat_guid: "iMessage;+;chat".into(),
        chat_name: "Owner".into(),
        participants: vec!["+15551234567".into()],
        is_group: false,
        guid: "watch-guid".into(),
        sender: "+15551234567".into(),
        sender_name: None,
        is_from_me: false,
        text: text.into(),
        created_at: "2026-07-14T00:02:00Z".into(),
        reply_to_guid: None,
        reply_to_text: None,
        reply_to_sender: None,
        destination_caller_id: None,
        is_read: None,
        date_read: None,
    }
}
