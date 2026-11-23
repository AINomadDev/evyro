//! Remote access for Evyro: `evyro serve`.
//!
//! A small HTTP server that lets a paired device (a phone, another computer)
//! follow the work and act on it:
//!
//! - a JSON API under `/api/v1` (projects, tasks, sessions, handoffs, diffs,
//!   review comments, chats with coding agents and their permission requests);
//! - an event feed (`/api/v1/feed` long-poll, `/api/v1/stream` Server-Sent Events);
//! - MCP over HTTP at `/mcp/<project>` for agents on other machines;
//! - a phone-sized web companion at `/`.
//!
//! Security model:
//! - Off unless started. Binds to loopback by default. A Tailscale address
//!   (100.64.0.0/10 or fd7a:115c:a1e0::/48) is allowed; any other address
//!   needs `allow_lan`, because plain HTTP is not encrypted.
//! - Every call except `health` and `pair` needs a device token
//!   (`Authorization: Bearer evr_...`). Tokens come from one-time pairing codes
//!   made on the computer and are stored only as hashes (`evyro_core::remote`).
//! - Device scopes: `read`, `approve` (answer permission requests, comment),
//!   `control` (start and message chats, MCP write tools).
//! - An approval names the exact request and tool call it answers, and must
//!   pick one of the options the agent offered.
//! - Failed sign-ins are rate limited per address and overall. Every request
//!   is written to `logs/remote-audit.log` (no query strings, no bodies).

use evyro_acp::host::{Change, Host, OpenOpts};
use evyro_core::remote::Device;
use evyro_core::review::NewReviewComment;
use evyro_core::{brand, git, redact, util, Error, Store};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub const API_VERSION: u32 = 1;
const MAX_BODY: usize = 1024 * 1024;
const FEED_KEEP: usize = 2000;

// ---------- event feed ----------

/// A numbered list of recent changes that clients follow with `since`.
pub struct Feed {
    inner: Mutex<(u64, VecDeque<(u64, Value)>)>,
    cv: Condvar,
}

impl Feed {
    pub fn new() -> Arc<Self> {
        Arc::new(Feed {
            inner: Mutex::new((0, VecDeque::new())),
            cv: Condvar::new(),
        })
    }

    pub fn push(&self, mut v: Value) {
        let mut f = Vec::new();
        redact::redact_json(&mut v, &mut f);
        if let Ok(mut g) = self.inner.lock() {
            g.0 += 1;
            let seq = g.0;
            if let Some(o) = v.as_object_mut() {
                o.insert("seq".into(), json!(seq));
            }
            g.1.push_back((seq, v));
            while g.1.len() > FEED_KEEP {
                g.1.pop_front();
            }
        }
        self.cv.notify_all();
    }

    pub fn latest(&self) -> u64 {
        self.inner.lock().map(|g| g.0).unwrap_or(0)
    }

    /// Items after `since`, waiting up to `wait` for one to arrive.
    pub fn after(&self, since: u64, wait: Duration) -> (u64, Vec<Value>) {
        let deadline = Instant::now() + wait;
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return (since, vec![]),
        };
        loop {
            let items: Vec<Value> =
                g.1.iter()
                    .filter(|(s, _)| *s > since)
                    .map(|(_, v)| v.clone())
                    .collect();
            let now = Instant::now();
            if !items.is_empty() || now >= deadline {
                return (g.0, items);
            }
            g = match self.cv.wait_timeout(g, deadline - now) {
                Ok((g, _)) => g,
                Err(_) => return (since, vec![]),
            };
        }
    }

    /// Wake every waiting client (used when stopping).
    pub fn wake(&self) {
        self.cv.notify_all();
    }
}

/// Turn a chat host change into a feed item.
pub fn feed_change(feed: &Feed, c: &Change) {
    match c {
        Change::Item { id, seq, item } => {
            feed.push(json!({"type": "chat-item", "chat": id, "item_seq": seq, "item": item}))
        }
        Change::State(info) => feed.push(json!({"type": "chat-state", "info": info})),
    }
}

// ---------- binding ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reach {
    /// Only this computer.
    Loopback,
    /// Devices on your Tailscale network (encrypted by WireGuard).
    Tailscale,
    /// Any device that can reach the address, over plain HTTP.
    Lan,
}

/// Is it safe to listen on `ip`? Loopback and Tailscale are; anything else needs `allow_lan`.
pub fn check_bind(ip: IpAddr, allow_lan: bool) -> Result<Reach, String> {
    if ip.is_loopback() {
        return Ok(Reach::Loopback);
    }
    let tailscale = match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 100 && (64..=127).contains(&o[1])
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    };
    if tailscale {
        return Ok(Reach::Tailscale);
    }
    if allow_lan {
        return Ok(Reach::Lan);
    }
    Err(format!(
        "{ip} is not this computer or a Tailscale address. Plain HTTP on other networks is not encrypted, so {} refuses it unless you pass --allow-lan. Prefer Tailscale: run on your Tailscale IP, or keep 127.0.0.1 and use `tailscale serve`.",
        brand::PRODUCT_NAME
    ))
}

// ---------- rate limiting and audit ----------

#[derive(Default)]
struct Limiter {
    by_addr: HashMap<String, Vec<Instant>>,
    all: Vec<Instant>,
}

impl Limiter {
    const PER_ADDR: usize = 10;
    const ADDR_WINDOW: Duration = Duration::from_secs(600);
    const GLOBAL: usize = 60;
    const GLOBAL_WINDOW: Duration = Duration::from_secs(60);

    fn prune(&mut self) {
        let now = Instant::now();
        for v in self.by_addr.values_mut() {
            v.retain(|t| now.duration_since(*t) < Self::ADDR_WINDOW);
        }
        self.by_addr.retain(|_, v| !v.is_empty());
        self.all
            .retain(|t| now.duration_since(*t) < Self::GLOBAL_WINDOW);
    }
    fn blocked(&mut self, addr: &str) -> bool {
        self.prune();
        self.all.len() >= Self::GLOBAL
            || self
                .by_addr
                .get(addr)
                .is_some_and(|v| v.len() >= Self::PER_ADDR)
    }
    fn fail(&mut self, addr: &str) {
        let now = Instant::now();
        self.by_addr.entry(addr.to_string()).or_default().push(now);
        self.all.push(now);
    }
}

// ---------- server ----------

pub struct ServeOpts {
    pub host: String,
    pub port: u16,
    pub allow_lan: bool,
    /// The `evyro` program agents in chats get as their MCP server.
    pub evyro_mcp: Option<PathBuf>,
}

struct McpSession {
    server: Mutex<evyro_mcp::Server>,
    device: String,
    last_used: Mutex<Instant>,
}

struct Ctx {
    host: Host,
    feed: Arc<Feed>,
    limiter: Mutex<Limiter>,
    mcp: Mutex<HashMap<String, Arc<McpSession>>>,
    audit: Mutex<Option<std::fs::File>>,
    stop: Arc<AtomicBool>,
}

pub struct Running {
    pub addr: SocketAddr,
    pub reach: Reach,
    server: Arc<tiny_http::Server>,
    stop: Arc<AtomicBool>,
    feed: Arc<Feed>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Running {
    pub fn url(&self) -> String {
        match self.addr {
            SocketAddr::V6(a) => format!("http://[{}]:{}", a.ip(), a.port()),
            SocketAddr::V4(a) => format!("http://{a}"),
        }
    }
    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
    /// Stop accepting requests and end open event streams.
    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        self.server.unblock();
        self.feed.wake();
        for t in self.threads {
            let _ = t.join();
        }
    }
    /// A flag that, once set, stops the server (for Ctrl-C handlers).
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }
    /// Block until the stop flag is set, then stop.
    pub fn wait(self) {
        while !self.stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(200));
        }
        self.stop();
    }
}

/// Start serving. With `chats`, the server shares an existing chat host (the
/// desktop app's), so its chats can be followed and approved remotely; the
/// host's changes must be fed into the given feed.
pub fn start(opts: ServeOpts, chats: Option<(Host, Arc<Feed>)>) -> Result<Running, String> {
    let ip: IpAddr = opts
        .host
        .parse()
        .map_err(|_| format!("'{}' is not an IP address", opts.host))?;
    let reach = check_bind(ip, opts.allow_lan)?;
    let server = tiny_http::Server::http(SocketAddr::new(ip, opts.port))
        .map_err(|e| format!("cannot listen on {}:{}: {e}", opts.host, opts.port))?;
    let addr = server
        .server_addr()
        .to_ip()
        .ok_or("the server has no IP address")?;
    let server = Arc::new(server);
    let (host, feed) = match chats {
        Some(c) => c,
        None => {
            let feed = Feed::new();
            let f2 = feed.clone();
            let host = Host::new(
                Arc::new(move |c: Change| feed_change(&f2, &c)),
                opts.evyro_mcp.clone(),
            );
            (host, feed)
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    let audit = Store::open_default().ok().and_then(|s| {
        let _ = std::fs::create_dir_all(s.paths.logs_dir());
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(s.paths.logs_dir().join("remote-audit.log"))
            .ok()
    });
    let ctx = Arc::new(Ctx {
        host,
        feed: feed.clone(),
        limiter: Default::default(),
        mcp: Default::default(),
        audit: Mutex::new(audit),
        stop: stop.clone(),
    });
    let mut threads = Vec::new();

    // Project events (checkpoints, sessions, handoffs) into the feed.
    let (c2, s2) = (ctx.clone(), stop.clone());
    threads.push(std::thread::spawn(move || {
        let Ok(store) = Store::open_default() else {
            return;
        };
        let mut last = store.last_event_seq().unwrap_or(0);
        while !s2.load(Ordering::SeqCst) {
            if let Ok(evs) = store.events_after(last, 200) {
                for e in evs {
                    last = e.seq;
                    c2.feed.push(json!({"type": "event", "event": e}));
                }
            }
            for _ in 0..10 {
                if s2.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // Forget MCP sessions nobody used for an hour.
            if let Ok(mut m) = c2.mcp.lock() {
                m.retain(|_, s| {
                    s.last_used
                        .lock()
                        .map(|t| t.elapsed() < Duration::from_secs(3600))
                        .unwrap_or(false)
                });
            }
        }
    }));

    let (c3, s3, srv) = (ctx.clone(), stop.clone(), server.clone());
    threads.push(std::thread::spawn(move || {
        while !s3.load(Ordering::SeqCst) {
            match srv.recv_timeout(Duration::from_millis(250)) {
                Ok(Some(req)) => {
                    let c = c3.clone();
                    std::thread::spawn(move || handle(&c, req));
                }
                Ok(None) => {}
                Err(_) => break,
            }
        }
    }));

    Ok(Running {
        addr,
        reach,
        server,
        stop,
        feed,
        threads,
    })
}

// ---------- responses ----------

enum Out {
    Json(u16, Value),
    Bytes(u16, &'static str, Vec<u8>),
    Stream(Box<dyn Read + Send>),
    Empty(u16),
    WithHeader(Box<Out>, String, String),
}

fn err(code: u16, msg: impl Into<String>) -> Out {
    Out::Json(
        code,
        json!({"error": {"code": code, "message": msg.into()}}),
    )
}

fn from_core(e: Error) -> Out {
    let code = match &e {
        Error::NotFound(_) => 404,
        Error::Invalid(_) | Error::Transition(_) | Error::Unsupported(_) => 400,
        Error::Conflict(_) | Error::Drift(_) => 409,
        Error::VerificationRequired(_) => 422,
        _ => 500,
    };
    err(code, e.to_string())
}

fn ok(v: Value) -> Out {
    let mut v = v;
    let mut f = Vec::new();
    redact::redact_json(&mut v, &mut f);
    Out::Json(200, v)
}

fn header(k: &str, v: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("valid header")
}

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

fn respond(req: tiny_http::Request, out: Out) -> u16 {
    let mut extra: Vec<(String, String)> = Vec::new();
    let mut out = out;
    while let Out::WithHeader(inner, k, v) = out {
        extra.push((k, v));
        out = *inner;
    }
    let common = |r: &mut Vec<tiny_http::Header>| {
        r.push(header("Cache-Control", "no-store"));
        r.push(header("X-Content-Type-Options", "nosniff"));
        r.push(header("Referrer-Policy", "no-referrer"));
        r.push(header("X-Frame-Options", "DENY"));
        r.push(header("Content-Security-Policy", CSP));
    };
    let mut hs = Vec::new();
    common(&mut hs);
    for (k, v) in &extra {
        hs.push(header(k, v));
    }
    let (code, res) = match out {
        Out::Json(code, v) => {
            hs.push(header("Content-Type", "application/json; charset=utf-8"));
            let body = serde_json::to_vec(&v).unwrap_or_default();
            let len = body.len();
            (
                code,
                tiny_http::Response::new(
                    code.into(),
                    hs,
                    Box::new(std::io::Cursor::new(body)) as Box<dyn Read + Send>,
                    Some(len),
                    None,
                ),
            )
        }
        Out::Bytes(code, ct, b) => {
            hs.push(header("Content-Type", ct));
            let len = b.len();
            (
                code,
                tiny_http::Response::new(
                    code.into(),
                    hs,
                    Box::new(std::io::Cursor::new(b)) as Box<dyn Read + Send>,
                    Some(len),
                    None,
                ),
            )
        }
        Out::Stream(r) => {
            hs.push(header("Content-Type", "text/event-stream"));
            hs.push(header("X-Accel-Buffering", "no"));
            (200, tiny_http::Response::new(200.into(), hs, r, None, None))
        }
        Out::Empty(code) => (
            code,
            tiny_http::Response::new(
                code.into(),
                hs,
                Box::new(std::io::empty()) as Box<dyn Read + Send>,
                Some(0),
                None,
            ),
        ),
        Out::WithHeader(..) => unreachable!(),
    };
    let _ = req.respond(res);
    code
}

/// Server-Sent Events from the feed, with a comment line every 15 seconds.
struct Sse {
    feed: Arc<Feed>,
    since: u64,
    buf: Vec<u8>,
    pos: usize,
    stop: Arc<AtomicBool>,
    until: Instant,
}

impl Read for Sse {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.pos >= self.buf.len() {
            if self.stop.load(Ordering::SeqCst) || Instant::now() > self.until {
                return Ok(0);
            }
            let (_, items) = self.feed.after(self.since, Duration::from_secs(15));
            self.buf.clear();
            self.pos = 0;
            if items.is_empty() {
                self.buf.extend_from_slice(b": ping\n\n");
            }
            for v in items {
                let seq = v["seq"].as_u64().unwrap_or(self.since);
                self.since = self.since.max(seq);
                let kind = v["type"].as_str().unwrap_or("message").to_string();
                let _ = write!(self.buf, "id: {seq}\nevent: {kind}\ndata: {v}\n\n");
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

// ---------- web companion ----------

const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const STYLE_CSS: &str = include_str!("../web/style.css");
const MANIFEST: &str = include_str!("../web/manifest.webmanifest");
const ICON: &[u8] = include_bytes!("../../evyro-desktop/ui/logo.png");

fn static_file(path: &str) -> Option<Out> {
    Some(match path {
        "/" | "/index.html" => Out::Bytes(
            200,
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes().to_vec(),
        ),
        "/app.js" => Out::Bytes(
            200,
            "text/javascript; charset=utf-8",
            APP_JS.as_bytes().to_vec(),
        ),
        "/style.css" => Out::Bytes(
            200,
            "text/css; charset=utf-8",
            STYLE_CSS.as_bytes().to_vec(),
        ),
        "/manifest.webmanifest" => Out::Bytes(
            200,
            "application/manifest+json",
            MANIFEST.as_bytes().to_vec(),
        ),
        "/icon.png" => Out::Bytes(200, "image/png", ICON.to_vec()),
        _ => return None,
    })
}

// ---------- requests ----------

struct Req<'a> {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: &'a [tiny_http::Header],
    body: Vec<u8>,
    addr: String,
}

impl Req<'_> {
    fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str().to_string())
    }
    fn json(&self) -> Result<Value, Out> {
        if self.body.is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_slice(&self.body)
            .map_err(|e| err(400, format!("the body is not JSON: {e}")))
    }
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn handle(ctx: &Ctx, mut req: tiny_http::Request) {
    let addr = req
        .remote_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();
    let method = req.method().as_str().to_ascii_uppercase();
    let url = req.url().to_string();
    let (path, qs) = url.split_once('?').unwrap_or((&url, ""));
    let path = path.to_string();
    let query: HashMap<String, String> = qs
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k), decode(v))
        })
        .collect();
    let mut body = Vec::new();
    let too_big = req
        .as_reader()
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut body)
        .map(|n| n > MAX_BODY)
        .unwrap_or(true);
    let headers = req.headers().to_vec();
    let r = Req {
        method: method.clone(),
        path: path.clone(),
        query,
        headers: &headers,
        body,
        addr: addr.clone(),
    };
    let mut who: Option<Device> = None;
    let out = if too_big {
        err(413, "the request body is larger than 1 MB")
    } else {
        route(ctx, &r, &mut who)
    };
    let code = respond(req, out);
    if let Ok(mut a) = ctx.audit.lock() {
        if let Some(f) = a.as_mut() {
            let line = json!({
                "at": util::now(), "address": addr, "device": who.as_ref().map(|d| &d.id),
                "device_name": who.as_ref().map(|d| &d.name), "method": method, "path": path, "status": code,
            });
            let _ = writeln!(f, "{line}");
        }
    }
}

fn store() -> Result<Store, Out> {
    Store::open_default().map_err(from_core)
}

fn need(d: &Device, scope: &str) -> Result<(), Out> {
    if d.allows(scope) {
        Ok(())
    } else {
        Err(err(
            403,
            format!("this device has '{}' access; this needs '{scope}'", d.scope),
        ))
    }
}

fn authenticate(ctx: &Ctx, r: &Req, allow_query: bool) -> Result<Device, Out> {
    if ctx
        .limiter
        .lock()
        .map(|mut l| l.blocked(&r.addr))
        .unwrap_or(true)
    {
        return Err(err(429, "too many failed sign-ins; wait a few minutes"));
    }
    let token = r
        .header("Authorization")
        .and_then(|h| h.strip_prefix("Bearer ").map(|t| t.trim().to_string()))
        .or_else(|| {
            if allow_query {
                r.query.get("token").cloned()
            } else {
                None
            }
        });
    let Some(token) = token else {
        return Err(err(
            401,
            "sign in: pair this device with a code from `evyro remote pair`",
        ));
    };
    let s = store()?;
    match s.authenticate_device(&token, &r.addr) {
        Ok(d) => Ok(d),
        Err(e) => {
            if let Ok(mut l) = ctx.limiter.lock() {
                l.fail(&r.addr);
            }
            Err(err(401, e.to_string()))
        }
    }
}

fn route(ctx: &Ctx, r: &Req, who: &mut Option<Device>) -> Out {
    match route_inner(ctx, r, who) {
        Ok(o) | Err(o) => o,
    }
}

fn route_inner(ctx: &Ctx, r: &Req, who: &mut Option<Device>) -> Result<Out, Out> {
    let segs: Vec<String> = r
        .path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(decode)
        .collect();
    let seg: Vec<&str> = segs.iter().map(String::as_str).collect();
    let m = r.method.as_str();

    if m == "GET" {
        if let Some(o) = static_file(&r.path) {
            return Ok(o);
        }
    }
    match (m, seg.as_slice()) {
        ("GET", ["api", "v1", "health"]) => {
            return Ok(ok(
                json!({"ok": true, "product": brand::PRODUCT_NAME, "version": brand::VERSION, "api": API_VERSION}),
            ))
        }
        ("POST", ["api", "v1", "pair"]) => {
            if ctx
                .limiter
                .lock()
                .map(|mut l| l.blocked(&r.addr))
                .unwrap_or(true)
            {
                return Err(err(429, "too many failed attempts; wait a few minutes"));
            }
            let b = r.json()?;
            let code = b["code"].as_str().unwrap_or("");
            let s = store()?;
            return match s.redeem_pairing(code, &r.addr) {
                Ok((d, token)) => {
                    *who = Some(d.clone());
                    Ok(ok(json!({"token": token, "device": d})))
                }
                Err(e) => {
                    if let Ok(mut l) = ctx.limiter.lock() {
                        l.fail(&r.addr);
                    }
                    Err(err(401, e.to_string()))
                }
            };
        }
        _ => {}
    }

    let is_stream = m == "GET" && seg.as_slice() == ["api", "v1", "stream"];
    let d = authenticate(ctx, r, is_stream)?;
    *who = Some(d.clone());
    need(&d, "read")?;

    if seg.first() == Some(&"mcp") {
        return mcp(ctx, r, &d, seg.get(1).copied());
    }
    let s = store()?;
    let q = |k: &str| r.query.get(k).map(String::as_str).filter(|v| !v.is_empty());
    let out = match (m, &seg[..]) {
        ("GET", ["api", "v1", "me"]) => {
            json!({"device": d, "api": API_VERSION, "version": brand::VERSION})
        }
        ("GET", ["api", "v1", "projects"]) => {
            json!(s.list_projects(false, None).map_err(from_core)?)
        }
        ("GET", ["api", "v1", "projects", p, "overview"]) => {
            let p = s.get_project(p).map_err(from_core)?;
            let active = s.active_task(&p.id).map_err(from_core)?;
            let owner = match &active {
                Some(t) => s.active_session_for_task(&t.id).map_err(from_core)?,
                None => None,
            };
            let latest = match &active {
                Some(t) => s.latest_checkpoint(&p.id, Some(&t.id)).map_err(from_core)?,
                None => None,
            };
            json!({
                "project": p,
                "active_task": active,
                "owner": owner,
                "git": git::snapshot(&p.root()).ok(),
                "health": s.project_health(&p.id).map_err(from_core)?,
                "pending_handoff": s.pending_handoff(&p.id, None).map_err(from_core)?,
                "active_sessions": s.list_sessions(&p.id, true).map_err(from_core)?,
                "events": s.list_events(&p.id, None, 25).map_err(from_core)?,
                "latest_checkpoint": latest,
            })
        }
        ("GET", ["api", "v1", "projects", p, "tasks"]) => {
            let p = s.get_project(p).map_err(from_core)?;
            let mut out = Vec::new();
            for t in s
                .list_tasks(&p.id, None, q("all").is_some())
                .map_err(from_core)?
            {
                let c = s.criteria(&t).map_err(from_core)?;
                let met = c.iter().filter(|c| c.satisfied()).count();
                out.push(json!({"task": t, "criteria_met": met, "criteria_total": c.len()}));
            }
            json!(out)
        }
        ("GET", ["api", "v1", "projects", p, "tasks", key]) => {
            let p = s.get_project(p).map_err(from_core)?;
            let t = s.resolve_task(&p.id, key).map_err(from_core)?;
            json!({
                "task": t,
                "criteria": s.criteria(&t).map_err(from_core)?,
                "owner": s.active_session_for_task(&t.id).map_err(from_core)?,
                "checkpoints": s.list_checkpoints(&p.id, Some(&t.id), 10).map_err(from_core)?,
                "verifications": s.list_verifications(&p.id, Some(&t.id)).map_err(from_core)?,
                "pending_handoff": s.pending_handoff_for_task(&t.id).map_err(from_core)?,
            })
        }
        ("GET", ["api", "v1", "projects", p, "sessions"]) => {
            let p = s.get_project(p).map_err(from_core)?;
            json!(s
                .list_sessions(&p.id, q("active").is_some())
                .map_err(from_core)?)
        }
        ("GET", ["api", "v1", "projects", p, "handoffs"]) => {
            let p = s.get_project(p).map_err(from_core)?;
            json!(s.list_handoffs(&p.id, 50).map_err(from_core)?)
        }
        ("GET", ["api", "v1", "projects", p, "context"]) => {
            let p = s.get_project(p).map_err(from_core)?;
            let t = match q("task") {
                Some(k) => Some(s.resolve_task(&p.id, k).map_err(from_core)?),
                None => s.active_task(&p.id).map_err(from_core)?,
            };
            let b = evyro_core::context::budget_for(q("budget").unwrap_or("small")).unwrap_or(4000);
            let c = s.context_packet(&p, t.as_ref(), b).map_err(from_core)?;
            json!({"text": c.text, "approx_tokens": c.approx_tokens, "task": c.task_key})
        }
        ("GET", ["api", "v1", "projects", p, "changes"]) => {
            let p = s.get_project(p).map_err(from_core)?;
            let root = p.root();
            let files = git::status_files(&root).map_err(from_core)?;
            let mut raw = git::diff_text(&root, 400_000);
            if !raw.contains("… diff truncated …") {
                raw.push_str(&git::untracked_diff(&root, &files, 200_000));
            }
            let (diff, found) = redact::redact(&raw);
            json!({"files": files, "diff": diff, "redactions": found.len()})
        }
        ("GET", ["api", "v1", "projects", p, "review"]) => {
            let p = s.get_project(p).map_err(from_core)?;
            json!(s
                .list_review_comments(&p.id, &["open", "sent"])
                .map_err(from_core)?)
        }
        ("POST", ["api", "v1", "projects", p, "review"]) => {
            need(&d, "approve")?;
            let p = s.get_project(p).map_err(from_core)?;
            let b = r.json()?;
            let path = b["path"].as_str().unwrap_or("");
            let text = b["body"].as_str().unwrap_or("").trim();
            if path.is_empty() || text.is_empty() {
                return Err(err(400, "a comment needs a path and a body"));
            }
            let head = git::head_sha(&p.root()).ok().flatten();
            let author = format!("remote:{}", d.name);
            json!(s
                .add_review_comment(
                    &p.id,
                    NewReviewComment {
                        task_id: None,
                        path,
                        line: b["line"].as_u64().map(|l| l as u32),
                        snippet: b["snippet"].as_str().unwrap_or(""),
                        body: text,
                        author: &author,
                        commit_sha: head.as_deref(),
                    }
                )
                .map_err(from_core)?)
        }
        ("GET", ["api", "v1", "agents"]) => json!(evyro_acp::agent_status()),
        ("GET", ["api", "v1", "chats"]) => json!(ctx.host.list()),
        ("POST", ["api", "v1", "chats"]) => {
            need(&d, "control")?;
            let b = r.json()?;
            let p = s
                .get_project(b["project"].as_str().unwrap_or(""))
                .map_err(from_core)?;
            let target = b["target"].as_str().unwrap_or("").to_string();
            if target.is_empty() {
                return Err(err(400, "say which agent to chat with (target)"));
            }
            ctx.host.open(OpenOpts {
                project_id: p.id,
                target,
                task: b["task"].as_str().map(String::from),
                handoff: b["handoff"].as_str().map(String::from),
                accept_drift: b["accept_drift"].as_bool().unwrap_or(false),
                started_by: format!("remote:{}", d.name),
            })
        }
        ("GET", ["api", "v1", "chats", id]) => {
            let after = q("after").and_then(|a| a.parse().ok());
            ctx.host.log(id, after).map_err(|e| err(404, e))?
        }
        ("POST", ["api", "v1", "chats", id, "messages"]) => {
            need(&d, "control")?;
            let b = r.json()?;
            ctx.host
                .send(id, b["text"].as_str().unwrap_or(""))
                .map_err(|e| err(409, e))?;
            json!({"ok": true})
        }
        ("POST", ["api", "v1", "chats", id, "permission"]) => {
            need(&d, "approve")?;
            let b = r.json()?;
            approve(ctx, id, &b, &d)?
        }
        ("POST", ["api", "v1", "chats", id, "cancel"]) => {
            need(&d, "approve")?;
            ctx.host.cancel(id).map_err(|e| err(409, e))?;
            json!({"ok": true})
        }
        ("POST", ["api", "v1", "chats", id, "close"]) => {
            need(&d, "control")?;
            ctx.host.close(id).map_err(|e| err(409, e))?;
            json!({"ok": true})
        }
        ("GET", ["api", "v1", "feed"]) => {
            let since = q("since")
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| ctx.feed.latest());
            let wait = q("wait")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0)
                .min(25);
            let (latest, items) = ctx.feed.after(since, Duration::from_secs(wait));
            json!({"latest": latest, "items": items})
        }
        ("GET", ["api", "v1", "stream"]) => {
            let since = q("since")
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| ctx.feed.latest());
            return Ok(Out::Stream(Box::new(Sse {
                feed: ctx.feed.clone(),
                since,
                buf: Vec::new(),
                pos: 0,
                stop: ctx.stop.clone(),
                until: Instant::now() + Duration::from_secs(1800),
            })));
        }
        _ => {
            return Err(err(
                404,
                format!("no such endpoint: {} {}", r.method, r.path),
            ))
        }
    };
    Ok(ok(out))
}

/// Answer a permission request, only if it is the exact request the device saw.
fn approve(ctx: &Ctx, id: &str, b: &Value, d: &Device) -> Result<Value, Out> {
    let request = &b["request"];
    let tool_call = b["tool_call_id"].as_str().unwrap_or("");
    let pending = ctx.host.pending(id).map_err(|e| err(404, e))?;
    let Some((_, params)) = pending.iter().find(|(r, _)| r == request) else {
        return Err(err(
            409,
            "that request is no longer waiting (it was answered or the turn ended)",
        ));
    };
    let actual = params
        .pointer("/toolCall/toolCallId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if actual != tool_call {
        return Err(err(
            409,
            "that approval names a different tool call than the one waiting",
        ));
    }
    let option = b["option_id"].as_str();
    let offered: Vec<&str> = params["options"]
        .as_array()
        .map(|a| a.iter().filter_map(|o| o["optionId"].as_str()).collect())
        .unwrap_or_default();
    if let Some(o) = option {
        if !offered.contains(&o) {
            return Err(err(
                400,
                format!(
                    "'{o}' is not one of the agent's options: {}",
                    offered.join(", ")
                ),
            ));
        }
    }
    ctx.host
        .answer(id, request, option, &format!("remote:{}", d.name))
        .map_err(|e| err(409, e))?;
    Ok(json!({"ok": true, "answered": option}))
}

/// MCP over HTTP (Streamable HTTP, JSON responses). One session per
/// `Mcp-Session-Id`, bound to the device that started it. Devices without
/// `control` get the read-only tool set.
fn mcp(ctx: &Ctx, r: &Req, d: &Device, project: Option<&str>) -> Result<Out, Out> {
    let Some(project) = project else {
        return Err(err(404, "use /mcp/<project id or alias>"));
    };
    let sid = r.header("Mcp-Session-Id");
    match r.method.as_str() {
        "GET" => {
            return Ok(Out::WithHeader(
                Box::new(err(
                    405,
                    "this server does not stream; POST JSON-RPC messages",
                )),
                "Allow".into(),
                "POST, DELETE".into(),
            ))
        }
        "DELETE" => {
            if let Some(sid) = sid {
                if let Ok(mut m) = ctx.mcp.lock() {
                    if m.get(&sid).is_some_and(|s| s.device == d.id) {
                        m.remove(&sid);
                    }
                }
            }
            return Ok(Out::Empty(204));
        }
        "POST" => {}
        _ => return Err(err(405, "use POST")),
    }
    let msg: Value =
        serde_json::from_slice(&r.body).map_err(|e| err(400, format!("not JSON-RPC: {e}")))?;
    if msg.is_array() {
        return Err(err(400, "send one JSON-RPC message per request"));
    }
    let is_init = msg["method"] == "initialize";
    let (session, sid) = if is_init {
        let s = store()?;
        let p = s.get_project(project).map_err(from_core)?;
        let server = evyro_mcp::Server::new(
            s,
            p,
            evyro_mcp::ServerOptions {
                read_only: !d.allows("control"),
                agent: r.query.get("agent").cloned().filter(|a| !a.is_empty()),
            },
        );
        let sid = util::new_id("mcps");
        let sess = Arc::new(McpSession {
            server: Mutex::new(server),
            device: d.id.clone(),
            last_used: Mutex::new(Instant::now()),
        });
        ctx.mcp
            .lock()
            .map_err(|_| err(500, "lock"))?
            .insert(sid.clone(), sess.clone());
        (sess, sid)
    } else {
        let Some(sid) = sid else {
            return Err(err(400, "missing Mcp-Session-Id: send initialize first"));
        };
        let sess = ctx
            .mcp
            .lock()
            .map_err(|_| err(500, "lock"))?
            .get(&sid)
            .cloned();
        match sess {
            Some(s) if s.device == d.id => (s, sid),
            _ => return Err(err(404, "unknown MCP session; initialize again")),
        }
    };
    if let Ok(mut t) = session.last_used.lock() {
        *t = Instant::now();
    }
    let reply = session
        .server
        .lock()
        .map_err(|_| err(500, "lock"))?
        .handle(&msg);
    let out = match reply {
        Some(v) => Out::Json(200, v),
        None => Out::Empty(202),
    };
    Ok(Out::WithHeader(Box::new(out), "Mcp-Session-Id".into(), sid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_policy() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert_eq!(check_bind(ip("127.0.0.1"), false), Ok(Reach::Loopback));
        assert_eq!(check_bind(ip("::1"), false), Ok(Reach::Loopback));
        assert_eq!(
            check_bind(ip("100.101.102.103"), false),
            Ok(Reach::Tailscale)
        );
        assert_eq!(
            check_bind(ip("fd7a:115c:a1e0::1"), false),
            Ok(Reach::Tailscale)
        );
        assert!(check_bind(ip("100.128.0.1"), false).is_err());
        assert!(check_bind(ip("0.0.0.0"), false).is_err());
        assert!(check_bind(ip("192.168.1.5"), false).is_err());
        assert_eq!(check_bind(ip("0.0.0.0"), true), Ok(Reach::Lan));
    }

    #[test]
    fn limiter_blocks_after_repeated_failures() {
        let mut l = Limiter::default();
        for _ in 0..Limiter::PER_ADDR {
            assert!(!l.blocked("a"));
            l.fail("a");
        }
        assert!(l.blocked("a"));
        assert!(!l.blocked("b"));
    }

    #[test]
    fn feed_waits_and_numbers_items() {
        let f = Feed::new();
        let f2 = f.clone();
        let t = std::thread::spawn(move || f2.after(0, Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(50));
        f.push(json!({"type": "x", "secret": "api_key=sk_live_abcdefghijklmnop"}));
        let (latest, items) = t.join().unwrap();
        assert_eq!(latest, 1);
        assert_eq!(items[0]["seq"], 1);
        assert!(
            !items[0].to_string().contains("sk_live"),
            "feed items are redacted"
        );
        assert!(f.after(1, Duration::from_millis(10)).1.is_empty());
    }

    #[test]
    fn decodes_query_values() {
        assert_eq!(decode("a%20b+c%2Fd"), "a b c/d");
        assert_eq!(decode("100%"), "100%");
    }
}
