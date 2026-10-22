//! Which hosts scripts may connect to, and which environment variables they see: deny by
//! default, from `net.allow` and `env` in `bevy-ts.json` (or a shipped game's `game.json`).

use bevy::prelude::*;
use std::collections::BTreeMap;
use std::fmt;

/// One `net.allow` entry: a host, and a port unless any port is allowed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Allowed {
    /// Lower-case; an IPv6 address without brackets.
    host: String,
    port: Option<u16>,
}

/// The network allowlist and the visible environment, fixed when the engine starts.
#[derive(Resource, Clone, Debug, Default)]
pub struct NetPolicy {
    allow: Vec<Allowed>,
    /// What `process.env` shows.
    pub env: BTreeMap<String, String>,
}

/// Why an open was refused; its `Display` is the log line's reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    EditMode,
    BadUrl(String),
    Wss,
    NotAllowed { origin: String },
    HeadersNotLoopback,
    HeadersOrigin,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::EditMode => f.write_str("networking runs only in play; open it from a system or once the game plays"),
            Refusal::BadUrl(why) => write!(f, "not a WebSocket URL: {why}"),
            Refusal::Wss => f.write_str("wss: is not supported yet; use ws: to a loopback host"),
            Refusal::NotAllowed { origin } => write!(f, "{origin} is not in net.allow; add it to net.allow in bevy-ts.json"),
            Refusal::HeadersNotLoopback => f.write_str("headers are sent only over loopback until wss: is supported"),
            Refusal::HeadersOrigin => f.write_str("the engine sends no Origin header; remove it from headers"),
        }
    }
}

/// A `ws:`/`wss:` URL's parts the policy reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub secure: bool,
    /// Lower-case; an IPv6 address without brackets.
    pub host: String,
    pub port: u16,
}

impl Target {
    /// `host:port`, brackets around IPv6, as logs name it.
    pub fn origin(&self) -> String {
        if self.host.contains(':') { format!("[{}]:{}", self.host, self.port) } else { format!("{}:{}", self.host, self.port) }
    }
}

/// Splits `scheme://authority/...` into the scheme and the authority's host and port.
fn split_url(url: &str) -> Option<(String, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    Some((scheme.to_ascii_lowercase(), authority))
}

/// `host`, `host:port`, `[v6]` or `[v6]:port` to its lower-case host and port.
fn host_port(authority: &str) -> Result<(String, Option<u16>), String> {
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (host, after) = rest.split_once(']').ok_or("an IPv6 host without its closing ]")?;
        (host, after.strip_prefix(':'))
    } else if authority.matches(':').count() > 1 {
        // A bare IPv6 address, with no port.
        (authority, None)
    } else {
        match authority.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return Err("no host".into());
    }
    let port = match port {
        None | Some("") => None,
        Some(p) => Some(p.parse::<u16>().map_err(|_| format!("bad port {p}"))?),
    };
    Ok((host.to_ascii_lowercase(), port))
}

fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "ws" | "http" => Some(80),
        "wss" | "https" => Some(443),
        _ => None,
    }
}

/// Parses a `ws:` or `wss:` URL.
pub fn parse_target(url: &str) -> Result<Target, String> {
    let (scheme, authority) = split_url(url).ok_or("no scheme")?;
    let secure = match scheme.as_str() {
        "ws" => false,
        "wss" => true,
        other => return Err(format!("the scheme is {other}:, not ws:")),
    };
    let (host, port) = host_port(authority)?;
    Ok(Target { secure, port: port.or(default_port(&scheme)).unwrap_or(80), host })
}

/// `localhost`, `127.0.0.0/8` or `::1`.
pub fn is_loopback(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// Replaces each `${NAME}` with its value from `lookup`; `None` if a name is unset or empty.
fn expand(entry: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = entry;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find('}')?;
        let value = lookup(&after[..end]).filter(|v| !v.is_empty())?;
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Parses an expanded entry: `host`, `host:port`, or a URL whose host and port are taken.
fn parse_entry(entry: &str) -> Result<Allowed, String> {
    let entry = entry.trim();
    if let Some((scheme, authority)) = split_url(entry) {
        let (host, port) = host_port(authority)?;
        return Ok(Allowed { host, port: port.or(default_port(&scheme)) });
    }
    let (host, port) = host_port(entry)?;
    Ok(Allowed { host, port })
}

impl NetPolicy {
    /// Builds the policy from the project's `net.allow` and `env`, reading `${NAME}` entries and
    /// the visible variables through `lookup` (the process environment, in the engine). An unset
    /// or empty variable adds nothing. Returns the policy and warnings for entries it skipped.
    pub fn from_config(allow: &[String], env: &[String], lookup: &dyn Fn(&str) -> Option<String>) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut policy = NetPolicy::default();
        for raw in allow {
            let Some(entry) = expand(raw, lookup) else { continue };
            match parse_entry(&entry) {
                Ok(a) => policy.allow.push(a),
                Err(e) => warnings.push(format!("net.allow entry {raw:?} is skipped: {e}")),
            }
        }
        for name in env {
            if let Some(value) = lookup(name) {
                policy.env.insert(name.clone(), value);
            }
        }
        (policy, warnings)
    }

    /// From the process environment.
    pub fn from_env(allow: &[String], env: &[String]) -> (Self, Vec<String>) {
        Self::from_config(allow, env, &|name| std::env::var(name).ok())
    }

    fn allows(&self, t: &Target) -> bool {
        self.allow.iter().any(|a| a.host == t.host && a.port.is_none_or(|p| p == t.port))
    }

    /// Checks an open before any DNS lookup or socket, in order: edit mode, the URL, `wss:`, the
    /// allowlist, then headers (loopback only, never `Origin`). Returns the origin to log by.
    pub fn check(&self, url: &str, headers: &[(String, String)], edit: bool) -> Result<String, Refusal> {
        if edit {
            return Err(Refusal::EditMode);
        }
        let t = parse_target(url).map_err(Refusal::BadUrl)?;
        if t.secure {
            return Err(Refusal::Wss);
        }
        if !self.allows(&t) {
            return Err(Refusal::NotAllowed { origin: t.origin() });
        }
        if headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("origin")) {
            return Err(Refusal::HeadersOrigin);
        }
        if !headers.is_empty() && !is_loopback(&t.host) {
            return Err(Refusal::HeadersNotLoopback);
        }
        Ok(t.origin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(allow: &[&str], env: &[&str], vars: &[(&str, &str)]) -> NetPolicy {
        let vars: BTreeMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let allow: Vec<String> = allow.iter().map(|s| s.to_string()).collect();
        let env: Vec<String> = env.iter().map(|s| s.to_string()).collect();
        NetPolicy::from_config(&allow, &env, &|n| vars.get(n).cloned()).0
    }

    fn ok(p: &NetPolicy, url: &str) -> bool {
        p.check(url, &[], false).is_ok()
    }

    #[test]
    fn entry_forms() {
        let p = policy(&["127.0.0.1:4477", "example.com", "http://10.0.0.2:5000", "ws://Host.Test"], &[], &[]);
        assert!(ok(&p, "ws://127.0.0.1:4477/api/stream"));
        assert!(!ok(&p, "ws://127.0.0.1:4478/"));
        assert!(ok(&p, "ws://example.com/"));
        assert!(ok(&p, "ws://example.com:9000/"), "a bare host allows any port");
        assert!(ok(&p, "ws://10.0.0.2:5000/"));
        assert!(!ok(&p, "ws://10.0.0.2/"));
        assert!(ok(&p, "ws://host.test/"), "a URL without a port takes its scheme's");
        assert!(!ok(&p, "ws://host.test:81/"));
    }

    #[test]
    fn hosts_compare_case_insensitively_and_literally() {
        let p = policy(&["LocalHost:4477"], &[], &[]);
        assert!(ok(&p, "ws://localhost:4477/"));
        assert!(ok(&p, "ws://LOCALHOST:4477/"));
        assert_eq!(
            p.check("ws://127.0.0.1:4477/", &[], false),
            Err(Refusal::NotAllowed { origin: "127.0.0.1:4477".into() }),
            "localhost does not allow 127.0.0.1"
        );
        let p = policy(&["[::1]:4477"], &[], &[]);
        assert!(ok(&p, "ws://[::1]:4477/"));
    }

    #[test]
    fn variables_expand_and_unset_ones_add_nothing() {
        let p = policy(&["127.0.0.1:4477", "${ABYME_URL}", "${UNSET}", "${EMPTY}"], &["ABYME_URL", "ABYME_KEY"], &[
            ("ABYME_URL", "http://127.0.0.1:5000"),
            ("HOME", "/home/me"),
            ("EMPTY", ""),
        ]);
        assert_eq!(p.allow.len(), 2);
        assert!(ok(&p, "ws://127.0.0.1:4477/"));
        assert!(ok(&p, "ws://127.0.0.1:5000/api/stream"));
        assert_eq!(p.env.get("ABYME_URL").map(String::as_str), Some("http://127.0.0.1:5000"));
        assert!(!p.env.contains_key("ABYME_KEY"));
        assert!(!p.env.contains_key("HOME"));
    }

    #[test]
    fn every_refusal() {
        let p = policy(&["127.0.0.1:4477", "10.0.0.5:4477"], &[], &[]);
        let h = |k: &str| vec![(k.to_string(), "x".to_string())];
        assert_eq!(p.check("ws://127.0.0.1:4477/", &[], true), Err(Refusal::EditMode));
        assert_eq!(p.check("wss://127.0.0.1:4477/", &[], false), Err(Refusal::Wss));
        assert!(matches!(p.check("ftp://127.0.0.1:4477/", &[], false), Err(Refusal::BadUrl(_))));
        assert_eq!(p.check("ws://example.com/", &[], false), Err(Refusal::NotAllowed { origin: "example.com:80".into() }));
        assert_eq!(p.check("ws://10.0.0.5:4477/", &h("authorization"), false), Err(Refusal::HeadersNotLoopback));
        assert_eq!(p.check("ws://127.0.0.1:4477/", &h("Origin"), false), Err(Refusal::HeadersOrigin));
        assert_eq!(p.check("ws://127.0.0.1:4477/", &h("authorization"), false), Ok("127.0.0.1:4477".into()));
        assert!(p.check("ws://10.0.0.5:4477/", &[], false).is_ok());
        assert!(NetPolicy::default().check("ws://127.0.0.1/", &[], false).is_err(), "an empty allowlist refuses everything");
        // The log lines say what to do.
        assert!(Refusal::NotAllowed { origin: "example.com:80".into() }.to_string().contains("net.allow in bevy-ts.json"));
        assert!(Refusal::Wss.to_string().contains("wss: is not supported yet"));
        assert!(Refusal::HeadersNotLoopback.to_string().contains("only over loopback"));
        assert!(Refusal::HeadersOrigin.to_string().contains("sends no Origin"));
        assert!(Refusal::EditMode.to_string().contains("only in play"));
    }

    #[test]
    fn loopback() {
        assert!(is_loopback("localhost"));
        assert!(is_loopback("127.3.2.1"));
        assert!(is_loopback("::1"));
        assert!(!is_loopback("10.0.0.5"));
        assert!(!is_loopback("localhost.example"));
    }
}
