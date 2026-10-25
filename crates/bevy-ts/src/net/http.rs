//! One HTTP/1.1 exchange per fetch, on a connection of its own: DNS off the main thread, TCP, TLS
//! for `https:`, hyper's client over it, the body read whole up to the 16 MiB bound. The task
//! sends one event; dropping it (a cancel) drops the connection.

use super::tls::{self, NetTls};
use super::{FetchId, HttpRequest, HttpResponse, MAX_MESSAGE_BYTES, NetEvent};
use async_channel::Sender;
use bevy::tasks::futures_lite::future;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::header::{HeaderName, HeaderValue};
use smol_hyper::rt::FuturesIo;

/// Headers the engine sets itself; a script's values for them are ignored. A fetch has no origin.
const OWN_HEADERS: [&str; 7] = ["host", "content-length", "connection", "transfer-encoding", "accept-encoding", "keep-alive", "origin"];

/// The over-the-bound reason core and this module share.
pub const TOO_BIG: &str = "the body was over the 16 MiB limit";

/// The most hyper buffers from the connection at once (its floor is 8 KiB).
const READ_BUFFER: usize = 16 * 1024;

pub async fn exchange(id: FetchId, request: HttpRequest, tls: NetTls, events: Sender<NetEvent>) {
    let event = match run(request, &tls).await {
        Ok(response) => NetEvent::Response { id, response },
        Err(reason) => NetEvent::FetchFailed { id, reason },
    };
    let _ = events.send(event).await;
}

async fn run(request: HttpRequest, tls: &NetTls) -> Result<HttpResponse, String> {
    let url = url::Url::parse(&request.url).map_err(|e| format!("not a URL: {e}"))?;
    let secure = match url.scheme() {
        "http" => false,
        "https" => true,
        other => return Err(format!("the scheme is {other}:")),
    };
    let host = url.host_str().ok_or("no host")?.to_string();
    let port = url.port_or_known_default().ok_or("no port")?;
    let mut target = url.path().to_string();
    if let Some(q) = url.query() {
        target.push('?');
        target.push_str(q);
    }

    let bad = |e: &dyn std::fmt::Display| format!("a request the engine cannot send: {e}");
    let mut req = hyper::Request::builder().method(request.method.as_str()).uri(target);
    let headers = req.headers_mut().ok_or("a bad method or path")?;
    let default_port = if secure { 443 } else { 80 };
    let host_header = if port == default_port { host.clone() } else { format!("{host}:{port}") };
    headers.insert("host", HeaderValue::from_str(&host_header).map_err(|e| bad(&e))?);
    for (name, value) in &request.headers {
        if OWN_HEADERS.iter().any(|h| h.eq_ignore_ascii_case(name)) {
            continue;
        }
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|e| bad(&e))?;
        headers.append(name, HeaderValue::from_str(value).map_err(|e| bad(&e))?);
    }
    if !headers.contains_key("accept") {
        headers.insert("accept", HeaderValue::from_static("*/*"));
    }
    if !headers.contains_key("user-agent") {
        headers.insert("user-agent", HeaderValue::from_static(concat!("bevy-ts/", env!("CARGO_PKG_VERSION"))));
    }
    headers.insert("accept-encoding", HeaderValue::from_static("identity"));
    headers.insert("connection", HeaderValue::from_static("close"));
    if !request.body.is_empty() || matches!(request.method.as_str(), "POST" | "PUT" | "PATCH") {
        headers.insert("content-length", HeaderValue::from(request.body.len()));
    }
    let req = req.body(Full::new(Bytes::from(request.body))).map_err(|e| bad(&e))?;

    let stream = tls::connect(&host, port, secure.then_some(tls)).await?;
    // hyper's read buffer grows to about 400 KiB unless capped: capped, the body read past the
    // bound is at most this much.
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .max_buf_size(READ_BUFFER)
        .handshake(FuturesIo::new(stream))
        .await
        .map_err(|e| format!("the HTTP exchange failed: {e}"))?;
    let read = async move {
        let response = sender.send_request(req).await.map_err(|e| format!("the HTTP exchange failed: {e}"))?;
        let (parts, mut body) = response.into_parts();
        // A length over the bound fails before any of the body is read.
        let length = parts.headers.get("content-length").and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
        if length.is_some_and(|n| n > MAX_MESSAGE_BYTES as u64) {
            return Err(TOO_BIG.to_string());
        }
        let mut bytes = Vec::new();
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|e| format!("the body was cut off: {e}"))?;
            if let Some(data) = frame.data_ref() {
                if bytes.len() + data.len() > MAX_MESSAGE_BYTES {
                    return Err(TOO_BIG.to_string());
                }
                bytes.extend_from_slice(data);
            }
        }
        let reason = parts
            .extensions
            .get::<hyper::ext::ReasonPhrase>()
            .map(|r| String::from_utf8_lossy(r.as_bytes()).into_owned())
            .or_else(|| parts.status.canonical_reason().map(str::to_string))
            .unwrap_or_default();
        let headers = parts.headers.iter().map(|(k, v)| (k.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect();
        Ok(HttpResponse { status: parts.status.as_u16(), reason, headers, body: bytes, ..Default::default() })
    };
    // The connection runs beside the read; past the bound, the read returns and the connection
    // is dropped with it, so no more of the body is read.
    future::or(read, async move {
        match conn.await {
            // The connection ended first: the read finishes from what it buffered, or fails.
            Ok(()) => future::pending().await,
            Err(e) => Err(format!("the HTTP exchange failed: {e}")),
        }
    })
    .await
}
