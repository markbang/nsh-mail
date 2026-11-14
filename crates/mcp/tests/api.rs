//! The REST client against a stub inbox: every call the tools make, paging,
//! and the errors an agent will meet (bad key, missing scope, timeout).

mod support;

use std::time::Duration;

use notify_mcp::api::{Error, ListQuery, NotifyApi};

#[tokio::test]
async fn list_unread_pages_with_cursors() {
    let base = support::serve().await;
    let api = NotifyApi::new(&base, support::KEY);

    let mut query = ListQuery::unread();
    query.limit = 2;
    let page = api.list(&query).await.unwrap();
    assert_eq!(page.messages.len(), 2);
    assert_eq!(page.messages[0].id, "msg_1");
    assert_eq!(page.next.as_deref(), Some("c2"));

    query.cursor = page.next.clone();
    let rest = api.list(&query).await.unwrap();
    assert_eq!(rest.messages.len(), 1);
    assert_eq!(rest.messages[0].id, "msg_3");
    assert!(rest.next.is_none());
}

#[tokio::test]
async fn search_sends_q_source_and_limit() {
    let base = support::serve().await;
    let api = NotifyApi::new(&base, support::KEY);

    let mut query = ListQuery::search("deploy");
    query.source = Some("src_mail".to_string());
    query.limit = 5;
    let page = api.list(&query).await.unwrap();
    assert_eq!(page.messages.len(), 1);
    assert_eq!(page.messages[0].title.as_deref(), Some("Deploy finished"));
    // 400 from the stub when the search lacks source=src_mail; this only got
    // here because it carried it.
}

#[tokio::test]
async fn get_mark_read_and_sources() {
    let base = support::serve().await;
    let api = NotifyApi::new(&base, support::KEY);

    let message = api.get("msg_9").await.unwrap();
    assert_eq!(message.id, "msg_9");
    assert!(message.body_text.is_some());
    assert_eq!(message.priority, Some(7));

    let read = api.mark_read("msg_9").await.unwrap();
    assert_eq!(
        read.read_at.as_ref().and_then(serde_json::Value::as_str),
        Some("2026-11-14T11:00:00Z")
    );

    let sources = api.sources().await.unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[1].kind.as_deref(), Some("webhook"));
}

#[tokio::test]
async fn auth_and_scope_errors_keep_the_server_message() {
    let base = support::serve().await;

    let wrong_key = NotifyApi::new(&base, "nb_wrong");
    let error = wrong_key.list(&ListQuery::unread()).await.unwrap_err();
    match &error {
        Error::Status {
            status: 401, code, ..
        } => assert_eq!(code, "ERR_AUTH"),
        other => panic!("expected 401, got {other:?}"),
    }
    let text = error.to_string();
    assert!(text.contains("invalid API key"), "{text}");
    assert!(text.contains("check the API key"), "{text}");

    let api = NotifyApi::new(&base, support::KEY);
    let error = api.mark_read("locked").await.unwrap_err();
    let text = error.to_string();
    assert!(text.contains("no write scope"), "{text}");
    assert!(text.contains("lacks the scope"), "{text}");

    let error = api.get("missing").await.unwrap_err();
    assert!(error.to_string().contains("no such message: missing"));
}

#[tokio::test]
async fn timeout_is_reported_as_a_timeout() {
    let base = support::serve().await;
    let api = NotifyApi::new(&base, support::KEY).with_timeout(Duration::from_millis(100));

    let mut query = ListQuery::search("slow");
    query.source = Some("src_mail".to_string());
    let error = api.list(&query).await.unwrap_err();
    assert!(matches!(error, Error::Timeout(_)), "{error:?}");
    assert!(error.to_string().contains("did not answer within"));
}
