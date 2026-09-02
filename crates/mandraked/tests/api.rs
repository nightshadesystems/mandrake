//! End-to-end tests against the router with an in-memory database.
//!
//! The root socket is simulated by attaching the `SocketPeer` extension a
//! real socket connection would carry.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines
)]

use std::time::Duration;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use mandraked::{
    app::{self, AppState},
    auth::{SocketPeer, ratelimit::LoginLimiter},
    db::Db,
};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn app() -> Router {
    app_with(LoginLimiter::new(10_000, Duration::from_secs(60))).await
}

async fn app_with(limiter: LoginLimiter) -> Router {
    let db = Db::open_in_memory().expect("in-memory db");
    let state = AppState::with_limiter(db, limiter).await.expect("state");
    app::router(state)
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    json: Value,
}

async fn send(app: &Router, req: Request<Body>) -> Reply {
    let response = app.clone().oneshot(req).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.expect("body");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    Reply {
        status,
        headers,
        json,
    }
}

fn build(method: Method, path: &str, body: Option<Value>) -> axum::http::request::Builder {
    let b = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"));
    if body.is_some() {
        b.header(header::CONTENT_TYPE, "application/json")
    } else {
        b
    }
}

fn body_of(body: Option<Value>) -> Body {
    body.map_or_else(Body::empty, |v| Body::from(v.to_string()))
}

/// A request as root over the socket.
fn as_root(method: Method, path: &str, body: Option<Value>) -> Request<Body> {
    let mut req = build(method, path, body.clone())
        .body(body_of(body))
        .expect("request");
    req.extensions_mut().insert(SocketPeer(Some(0)));
    req
}

/// A request with a session cookie and the CSRF header.
fn as_session(cookie: &str, method: Method, path: &str, body: Option<Value>) -> Request<Body> {
    build(method, path, body.clone())
        .header(header::COOKIE, format!("mandrake_session={cookie}"))
        .header("x-mandrake-request", "1")
        .body(body_of(body))
        .expect("request")
}

fn as_bearer(token: &str, method: Method, path: &str, body: Option<Value>) -> Request<Body> {
    build(method, path, body.clone())
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(body_of(body))
        .expect("request")
}

fn anonymous(method: Method, path: &str, body: Option<Value>) -> Request<Body> {
    build(method, path, body.clone())
        .body(body_of(body))
        .expect("request")
}

async fn create_user(app: &Router, username: &str, role: &str, password: &str) -> Value {
    let r = send(
        app,
        as_root(
            Method::POST,
            "/users",
            Some(json!({ "username": username, "password": password, "role": role })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    r.json
}

async fn login(app: &Router, username: &str, password: &str) -> Reply {
    send(
        app,
        anonymous(
            Method::POST,
            "/auth/login",
            Some(json!({ "username": username, "password": password })),
        ),
    )
    .await
}

fn cookie_of(r: &Reply) -> String {
    let set = r
        .headers
        .get(header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .expect("ascii");
    let value = set.split(';').next().expect("cookie pair");
    value
        .strip_prefix("mandrake_session=")
        .expect("session cookie")
        .to_owned()
}

#[tokio::test]
async fn health_needs_no_auth_and_everything_else_does() {
    let app = app().await;
    let r = send(&app, anonymous(Method::GET, "/health", None)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);

    let r = send(&app, anonymous(Method::GET, "/system", None)).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        r.headers.get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
    assert_eq!(r.json["status"], 401);
    assert!(r.headers.contains_key("x-request-id"));
}

#[tokio::test]
async fn root_over_the_socket_bootstraps_an_admin() {
    let app = app().await;
    let r = send(&app, as_root(Method::GET, "/auth/session", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["actor"]["username"], "root");
    assert_eq!(r.json["actor"]["via"], "socket");

    let user = create_user(&app, "alice", "admin", "correct horse battery").await;
    assert_eq!(user["role"], "admin");

    let r = send(&app, as_root(Method::GET, "/users", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["items"].as_array().unwrap().len(), 1);
    assert!(r.json["next_cursor"].is_null());

    // A non-root peer on the socket is refused.
    let mut req = anonymous(Method::GET, "/users", None);
    req.extensions_mut().insert(SocketPeer(Some(100)));
    assert_eq!(send(&app, req).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn login_sessions_and_csrf() {
    let app = app().await;
    create_user(&app, "alice", "admin", "correct horse battery").await;

    let r = login(&app, "alice", "wrong password here").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert!(
        r.json["type"]
            .as_str()
            .unwrap()
            .ends_with("invalid-credentials")
    );

    let r = login(&app, "nobody", "correct horse battery").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);

    let r = login(&app, "alice", "correct horse battery").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["actor"]["username"], "alice");
    let set = r.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(set.contains("HttpOnly") && set.contains("Secure") && set.contains("SameSite=Strict"));
    let cookie = cookie_of(&r);

    let r = send(
        &app,
        as_session(&cookie, Method::GET, "/auth/session", None),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["actor"]["via"], "session");
    assert!(r.json["expires_at"].is_string());

    // Mutation from a session without the CSRF header is refused.
    let req = build(Method::POST, "/users", Some(json!({})))
        .header(header::COOKIE, format!("mandrake_session={cookie}"))
        .body(Body::from(
            json!({ "username": "bob", "password": "another long one", "role": "viewer" })
                .to_string(),
        ))
        .unwrap();
    let r = send(&app, req).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert!(r.json["type"].as_str().unwrap().ends_with("csrf"));

    // With the header it works.
    let r = send(
        &app,
        as_session(
            &cookie,
            Method::POST,
            "/users",
            Some(json!({ "username": "bob", "password": "another long one", "role": "viewer" })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);

    // A viewer cannot create users.
    let bob = login(&app, "bob", "another long one").await;
    let bob_cookie = cookie_of(&bob);
    let r = send(
        &app,
        as_session(
            &bob_cookie,
            Method::POST,
            "/users",
            Some(json!({ "username": "carol", "password": "yet another long", "role": "viewer" })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // Logout ends the session.
    let r = send(
        &app,
        as_session(&cookie, Method::POST, "/auth/logout", None),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = send(
        &app,
        as_session(&cookie, Method::GET, "/auth/session", None),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn lockout_after_repeated_failures() {
    let app = app().await;
    create_user(&app, "alice", "admin", "correct horse battery").await;
    for _ in 0..4 {
        assert_eq!(
            login(&app, "alice", "wrong password here").await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    let r = login(&app, "alice", "wrong password here").await;
    assert_eq!(r.status, StatusCode::LOCKED);
    let r = login(&app, "alice", "correct horse battery").await;
    assert_eq!(r.status, StatusCode::LOCKED);
    let r = send(&app, as_root(Method::GET, "/users", None)).await;
    assert!(r.json["items"][0]["locked_until"].is_string());
}

#[tokio::test]
async fn login_rate_limit_per_source() {
    let app = app_with(LoginLimiter::new(2, Duration::from_secs(60))).await;
    assert_eq!(login(&app, "x", "y").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(login(&app, "x", "y").await.status, StatusCode::UNAUTHORIZED);
    let r = login(&app, "x", "y").await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(r.headers.contains_key(header::RETRY_AFTER));
}

#[tokio::test]
async fn tokens_authenticate_until_revoked() {
    let app = app().await;
    create_user(&app, "alice", "operator", "correct horse battery").await;
    let cookie = cookie_of(&login(&app, "alice", "correct horse battery").await);

    let r = send(
        &app,
        as_session(
            &cookie,
            Method::POST,
            "/tokens",
            Some(json!({ "name": "ci" })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    let secret = r.json["secret"].as_str().unwrap().to_owned();
    assert!(secret.starts_with("mdk_"));
    let token_id = r.json["id"].as_str().unwrap().to_owned();
    assert_eq!(r.json["prefix"].as_str().unwrap().len(), 8);

    let r = send(&app, as_bearer(&secret, Method::GET, "/auth/session", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["actor"]["via"], "token");
    assert_eq!(r.json["actor"]["token_id"], token_id);

    // Bearer requests need no CSRF header for mutations.
    let r = send(
        &app,
        as_bearer(
            &secret,
            Method::POST,
            "/tokens",
            Some(json!({ "name": "second" })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED);

    let r = send(&app, as_bearer(&secret, Method::GET, "/tokens", None)).await;
    assert_eq!(r.json["items"].as_array().unwrap().len(), 2);
    assert!(r.json["items"][0].get("secret").is_none());

    let r = send(
        &app,
        as_session(
            &cookie,
            Method::DELETE,
            &format!("/tokens/{token_id}"),
            None,
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = send(&app, as_bearer(&secret, Method::GET, "/auth/session", None)).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn audit_records_actions_and_filters() {
    let app = app().await;
    let alice = create_user(&app, "alice", "admin", "correct horse battery").await;
    let cookie = cookie_of(&login(&app, "alice", "correct horse battery").await);
    let r = send(&app, as_session(&cookie, Method::GET, "/audit", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    let actions: Vec<&str> = r.json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, vec!["auth.login", "user.create"]);
    assert_eq!(r.json["items"][1]["actor"]["username"], "root");
    assert_eq!(r.json["items"][1]["object"]["id"], alice["id"]);

    let r = send(
        &app,
        as_session(
            &cookie,
            Method::GET,
            "/audit?action=user.create&limit=1",
            None,
        ),
    )
    .await;
    assert_eq!(r.json["items"].as_array().unwrap().len(), 1);
    assert_eq!(r.json["items"][0]["after"]["username"], "alice");
}

#[tokio::test]
async fn idempotency_key_replays_and_rejects_mismatch() {
    let app = app().await;
    let body = json!({ "username": "alice", "password": "correct horse battery", "role": "admin" });
    let mut req = as_root(Method::POST, "/users", Some(body.clone()));
    req.headers_mut()
        .insert("idempotency-key", "k1".parse().unwrap());
    let first = send(&app, req).await;
    assert_eq!(first.status, StatusCode::CREATED);

    let mut req = as_root(Method::POST, "/users", Some(body));
    req.headers_mut()
        .insert("idempotency-key", "k1".parse().unwrap());
    let second = send(&app, req).await;
    assert_eq!(second.status, StatusCode::CREATED);
    assert_eq!(second.json["id"], first.json["id"]);
    assert_eq!(second.headers.get("idempotent-replayed").unwrap(), "true");

    let mut req = as_root(
        Method::POST,
        "/users",
        Some(json!({ "username": "bob", "password": "correct horse battery", "role": "admin" })),
    );
    req.headers_mut()
        .insert("idempotency-key", "k1".parse().unwrap());
    let r = send(&app, req).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        r.json["type"]
            .as_str()
            .unwrap()
            .ends_with("idempotency-mismatch")
    );

    let r = send(&app, as_root(Method::GET, "/users", None)).await;
    assert_eq!(r.json["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn admin_protections_and_password_change() {
    let app = app().await;
    let alice = create_user(&app, "alice", "admin", "correct horse battery").await;
    let id = alice["id"].as_str().unwrap().to_owned();
    let cookie = cookie_of(&login(&app, "alice", "correct horse battery").await);

    let r = send(
        &app,
        as_session(&cookie, Method::DELETE, &format!("/users/{id}"), None),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    let r = send(
        &app,
        as_session(
            &cookie,
            Method::PATCH,
            &format!("/users/{id}"),
            Some(json!({ "role": "viewer" })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(r.json["type"].as_str().unwrap().ends_with("self-demotion"));

    // Root may not demote the last admin either.
    let r = send(
        &app,
        as_root(
            Method::PATCH,
            &format!("/users/{id}"),
            Some(json!({ "disabled": true })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(r.json["type"].as_str().unwrap().ends_with("last-admin"));

    let r = send(
        &app,
        as_session(
            &cookie,
            Method::PUT,
            &format!("/users/{id}/password"),
            Some(json!({ "new_password": "a brand new secret" })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    let r = send(
        &app,
        as_session(
            &cookie,
            Method::PUT,
            &format!("/users/{id}/password"),
            Some(json!({ "current_password": "not it", "new_password": "a brand new secret" })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = send(
        &app,
        as_session(
            &cookie,
            Method::PUT,
            &format!("/users/{id}/password"),
            Some(json!({ "current_password": "correct horse battery", "new_password": "a brand new secret" })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(
        login(&app, "alice", "correct horse battery").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "alice", "a brand new secret").await.status,
        StatusCode::OK
    );

    // Disabling a user ends their sessions and refuses login.
    create_user(&app, "bob", "viewer", "another long one").await;
    let bob = send(
        &app,
        as_root(Method::GET, "/users?limit=1&cursor=YWxpY2U", None),
    )
    .await;
    let bob_id = bob.json["items"][0]["id"].as_str().unwrap().to_owned();
    let bob_cookie = cookie_of(&login(&app, "bob", "another long one").await);
    let r = send(
        &app,
        as_root(
            Method::PATCH,
            &format!("/users/{bob_id}"),
            Some(json!({ "disabled": true })),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["disabled"], true);
    assert_eq!(
        send(
            &app,
            as_session(&bob_cookie, Method::GET, "/auth/session", None)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "bob", "another long one").await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn system_endpoints_answer() {
    let app = app().await;
    let r = send(&app, as_root(Method::GET, "/system", None)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["product"], "mandrake");
    assert!(r.json["hostname"].is_string());
    let r = send(&app, as_root(Method::GET, "/system/resources", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.json["cpus"].as_u64().unwrap() >= 1);
    let r = send(&app, as_root(Method::GET, "/jobs", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn unknown_api_routes_are_problems_and_console_is_served_or_503() {
    let app = app().await;
    let r = send(&app, as_root(Method::GET, "/nope", None)).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(
        r.headers.get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );

    let req = Request::builder().uri("/").body(Body::empty()).unwrap();
    let r = send(&app, req).await;
    assert!(r.status == StatusCode::OK || r.status == StatusCode::SERVICE_UNAVAILABLE);
}
