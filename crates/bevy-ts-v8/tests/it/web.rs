//! The web globals web.js installs: URL (WPT cases), AbortSignal, timers on real time, and the
//! WebSocket constructor's validation and refusal. End-to-end socket tests live with the net
//! layer's (FakeTransport accepting opens and pushing messages).

use crate::common;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_ts::log::LogLevel;
use common::*;
use std::time::Duration;

/// An app whose `Time<Real>` advances `ms` a frame.
fn timed_app(ms: u64) -> App {
    let mut app = app();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(ms)));
    app
}

/// Loads JS, then runs one frame: console output reaches the ScriptLog when the backend pumps.
fn load_run(app: &mut App, files: &[(&str, &str)]) {
    load(app, files);
    app.update();
}

/// The messages that start with `prefix`, with it stripped.
fn tagged(app: &App, prefix: &str) -> Vec<String> {
    messages(app).into_iter().filter_map(|m| m.strip_prefix(prefix).map(str::to_string)).collect()
}

/// WPT `urltestdata.json` cases for `http(s)`, `ws(s)` and `file`: `[input, base]` must throw;
/// a longer row is `[input, base, href, protocol, username, password, host, hostname, port,
/// pathname, search, hash]`.
const URL_CASES: &str = r###"
[
  ["ws://a@b\\c",null,"ws://a@b/c","ws:","a","","b","b","","/c","",""],
  ["ws:/example.com/","http://example.org/foo/bar","ws://example.com/","ws:","","","example.com","example.com","","/","",""],
  ["wss:/example.com/","http://example.org/foo/bar","wss://example.com/","wss:","","","example.com","example.com","","/","",""],
  ["ws:example.com/","http://example.org/foo/bar","ws://example.com/","ws:","","","example.com","example.com","","/","",""],
  ["wss:example.com/","http://example.org/foo/bar","wss://example.com/","wss:","","","example.com","example.com","","/","",""],
  ["ws://foo:80/",null,"ws://foo/","ws:","","","foo","foo","","/","",""],
  ["ws://foo:81/",null,"ws://foo:81/","ws:","","","foo:81","foo","81","/","",""],
  ["ws://foo:443/",null,"ws://foo:443/","ws:","","","foo:443","foo","443","/","",""],
  ["ws://foo:815/",null,"ws://foo:815/","ws:","","","foo:815","foo","815","/","",""],
  ["wss://foo:80/",null,"wss://foo:80/","wss:","","","foo:80","foo","80","/","",""],
  ["file:/example.com/","http://example.org/foo/bar","file:///example.com/","file:","","","","","","/example.com/","",""],
  ["/C|\\foo\\bar","file:///tmp/mock/path","file:///C:/foo/bar","file:","","","","","","/C:/foo/bar","",""],
  ["/\\server/file","file:///tmp/mock/path","file://server/file","file:","","","server","server","","/file","",""],
  ["///","file:///tmp/mock/path","file:///","file:","","","","","","/","",""],
  ["file://localhost/","file:///tmp/mock/path","file:///","file:","","","","","","/","",""],
  ["file:///w|m",null,"file:///w|m","file:","","","","","","/w|m","",""],
  ["file:C||/m/",null,"file:///C||/m/","file:","","","","","","/C||/m/","",""],
  ["file:a","http://www.example.com/test","file:///a","file:","","","","","","/a","",""],
  ["file:///C%7C/",null,"file:///C%7C/","file:","","","","","","/C%7C/","",""],
  ["/","file:///C:/a/b","file:///C:/","file:","","","","","","/C:/","",""],
  ["//d:/..","file:///C:/a/b","file:///d:/","file:","","","","","","/d:/","",""],
  ["file:","file:///test?test#test","file:///test?test","file:","","","","","","/test","?test",""],
  ["file:#x","file:///test?test#test","file:///test?test#x","file:","","","","","","/test","?test","#x"],
  ["file:\\\\\\\\#guppy",null,"file:////#guppy","file:","","","","","","//","","#guppy"],
  ["file://\\/localhost//cat",null,"file:////localhost//cat","file:","","","","","","//localhost//cat","",""],
  ["\\/localhost//pig","file://lion/","file:////pig","file:","","","","","","//pig","",""],
  ["/rooibos","file://tea/","file://tea/rooibos","file:","","","tea","tea","","/rooibos","",""],
  ["C|#","file://host/dir/file","file://host/C:#","file:","","","host","host","","/C:","",""],
  ["C","file://host/dir/file","file://host/dir/C","file:","","","host","host","","/dir/C","",""],
  ["file:\\c:\\foo\\bar","file:///c:/baz/qux","file:///c:/foo/bar","file:","","","","","","/c:/foo/bar","",""],
  ["file://1.2.3.4/C:/",null,"file://1.2.3.4/C:/","file:","","","1.2.3.4","1.2.3.4","","/C:/","",""],
  ["file:C:/","file://host/","file://host/C:/","file:","","","host","host","","/C:/","",""],
  ["///C:/","file://host/","file:///C:/","file:","","","","","","/C:/","",""],
  ["file:",null,"file:///","file:","","","","","","/","",""],
  ["file:///Y:/",null,"file:///Y:/","file:","","","","","","/Y:/","",""],
  ["http://f:b/c","http://example.org/foo/bar"],
  ["http://[1::2]:3:4","http://example.org/foo/bar"],
  ["file://example:test/",null],
  ["http://user@/www.example.com",null],
  ["http:/a:b@/www.example.com",null],
  ["http://example example.com","http://other.com/"],
  ["http://%ef%bc%85%ef%bc%94%ef%bc%91.com","http://other.com/"],
  ["http://%3g%78%63%30%2e%30%32%35%30%2E.01","http://other.com/"],
  ["http://[::1.2.3.]","http://other.com/"],
  ["http://%5B::1]","http://other.com/"],
  ["http://a]b/",null],
  ["http://ho%03st/",null],
  ["http://ho%09st/",null],
  ["http://ho%0Fst/",null],
  ["http://ho%15st/",null],
  ["http://ho%1Bst/",null],
  ["http://ho%23st/",null],
  ["http://ho%3Fst/",null],
  ["http://ho%7Fst/",null],
  ["http://18446744073709551616","http://other.com/"],
  ["file://%43%3A",null],
  ["https://[0:1:2:3:4:5:192.0.02.1]/",null],
  ["https://[0:1:2:3:4:5:6:7.0.0.0.1]",null],
  ["http://f:4294967377/c","http://example.org/"],
  ["http://1.2.3.4.5.","http://other.com/"],
  ["http://user:pass@foo:21/bar;par?b#c","http://example.org/foo/bar","http://user:pass@foo:21/bar;par?b#c","http:","user","pass","foo:21","foo","21","/bar;par","?b","#c"],
  [" foo.com  ","http://example.org/foo/bar","http://example.org/foo/foo.com","http:","","","example.org","example.org","","/foo/foo.com","",""],
  ["http://f:00000000000000/c","http://example.org/foo/bar","http://f:0/c","http:","","","f:0","f","0","/c","",""],
  [":foo.com\\","http://example.org/foo/bar","http://example.org/foo/:foo.com/","http:","","","example.org","example.org","","/foo/:foo.com/","",""],
  [":\\","http://example.org/foo/bar","http://example.org/foo/:/","http:","","","example.org","example.org","","/foo/:/","",""],
  ["#\\","http://example.org/foo/bar","http://example.org/foo/bar#\\","http:","","","example.org","example.org","","/foo/bar","","#\\"],
  [":23","http://example.org/foo/bar","http://example.org/foo/:23","http:","","","example.org","example.org","","/foo/:23","",""],
  ["::","http://example.org/foo/bar","http://example.org/foo/::","http:","","","example.org","example.org","","/foo/::","",""],
  ["http://&a:foo(b]c@d:2/","http://example.org/foo/bar","http://&a:foo(b%5Dc@d:2/","http:","&a","foo(b%5Dc","d:2","d","2","/","",""],
  ["http:\\\\foo.com\\","http://example.org/foo/bar","http://foo.com/","http:","","","foo.com","foo.com","","/","",""],
  ["http://foo/path;a??e#f#g","http://example.org/foo/bar","http://foo/path;a??e#f#g","http:","","","foo","foo","","/path;a","??e","#f#g"],
  ["http:[61:27]/:foo","http://example.org/foo/bar","http://example.org/foo/[61:27]/:foo","http:","","","example.org","example.org","","/foo/[61:27]/:foo","",""],
  ["http://[2001::1]:80","http://example.org/foo/bar","http://[2001::1]/","http:","","","[2001::1]","[2001::1]","","/","",""],
  ["http:example.com/","http://example.org/foo/bar","http://example.org/foo/example.com/","http:","","","example.org","example.org","","/foo/example.com/","",""],
  ["/a%2fc","http://example.org/foo/bar","http://example.org/a%2fc","http:","","","example.org","example.org","","/a%2fc","",""],
  ["http://example.com/foo/.",null,"http://example.com/foo/","http:","","","example.com","example.com","","/foo/","",""],
  ["http://example.com/foo/..bar",null,"http://example.com/foo/..bar","http:","","","example.com","example.com","","/foo/..bar","",""],
  ["http://example.com/foo/../../../ton",null,"http://example.com/ton","http:","","","example.com","example.com","","/ton","",""],
  ["http://example.com////../..",null,"http://example.com//","http:","","","example.com","example.com","","//","",""],
  ["http://example.com/%20foo",null,"http://example.com/%20foo","http:","","","example.com","example.com","","/%20foo","",""],
  ["http://example.com/foo%41%7a",null,"http://example.com/foo%41%7a","http:","","","example.com","example.com","","/foo%41%7a","",""],
  ["http://example.com\\\\foo\\\\bar",null,"http://example.com//foo//bar","http:","","","example.com","example.com","","//foo//bar","",""],
  ["http://www.google.com",null,"http://www.google.com/","http:","","","www.google.com","www.google.com","","/","",""],
  ["http://%25DOMAIN:foobar@foodomain.com/",null,"http://%25DOMAIN:foobar@foodomain.com/","http:","%25DOMAIN","foobar","foodomain.com","foodomain.com","","/","",""],
  ["https://foo:443/",null,"https://foo/","https:","","","foo","foo","","/","",""],
  ["http:example.com/",null,"http://example.com/","http:","","","example.com","example.com","","/","",""],
  ["http:/@www.example.com",null,"http://www.example.com/","http:","","","www.example.com","www.example.com","","/","",""],
  ["http://a:b@www.example.com",null,"http://a:b@www.example.com/","http:","a","b","www.example.com","www.example.com","","/","",""],
  ["http://:b@www.example.com",null,"http://:b@www.example.com/","http:","","b","www.example.com","www.example.com","","/","",""],
  ["http://www.@pple.com",null,"http://www.@pple.com/","http:","www.","","pple.com","pple.com","","/","",""],
  [".","http://www.example.com/test","http://www.example.com/","http:","","","www.example.com","www.example.com","","/","",""],
  ["../test.txt","http://www.example.com/test","http://www.example.com/test.txt","http:","","","www.example.com","www.example.com","","/test.txt","",""],
  ["//www.example2.com","http://www.example.com/test","http://www.example2.com/","http:","","","www.example2.com","www.example2.com","","/","",""],
  ["http://a.b.c.XN--pokxncvks",null,"http://a.b.c.xn--pokxncvks/","http:","","","a.b.c.xn--pokxncvks","a.b.c.xn--pokxncvks","","/","",""],
  ["http://%30%78%63%30%2e%30%32%35%30.01","http://other.com/","http://192.168.0.1/","http:","","","192.168.0.1","192.168.0.1","","/","",""],
  ["https://@test@test@example:800/","http://doesnotmatter/","https://%40test%40test@example:800/","https:","%40test%40test","","example:800","example","800","/","",""],
  ["/some/path","http://user@example.org/smth","http://user@example.org/some/path","http:","user","","example.org","example.org","","/some/path","",""],
  ["https://%e2%98%83",null,"https://xn--n3h/","https:","","","xn--n3h","xn--n3h","","/","",""],
  ["?a=b&c=d","http://example.org/foo/bar","http://example.org/foo/bar?a=b&c=d","http:","","","example.org","example.org","","/foo/bar","?a=b&c=d",""],
  ["http://foo.bar/baz?qux#foo<bar",null,"http://foo.bar/baz?qux#foo%3Cbar","http:","","","foo.bar","foo.bar","","/baz","?qux","#foo%3Cbar"],
  ["http://1.2.3.4./","http://other.com/","http://1.2.3.4/","http:","","","1.2.3.4","1.2.3.4","","/","",""],
  ["http://256","http://other.com/","http://0.0.1.0/","http:","","","0.0.1.0","0.0.1.0","","/","",""],
  ["http://999999999.com","http://other.com/","http://999999999.com/","http:","","","999999999.com","999999999.com","","/","",""],
  ["https://0x.0x.0",null,"https://0.0.0.0/","https:","","","0.0.0.0","0.0.0.0","","/","",""],
  ["http://[1:0::]","http://example.net/","http://[1::]/","http:","","","[1::]","[1::]","","/","",""],
  ["http://example\t.\norg","http://example.org/foo/bar","http://example.org/","http:","","","example.org","example.org","","/","",""],
  ["\t   :foo.com   \n","http://example.org/foo/bar","http://example.org/foo/:foo.com","http:","","","example.org","example.org","","/foo/:foo.com","",""],
  ["http://f:\n/c","http://example.org/foo/bar","http://f/c","http:","","","f","f","","/c","",""],
  ["  \t","http://example.org/foo/bar","http://example.org/foo/bar","http:","","","example.org","example.org","","/foo/bar","",""],
  ["http://é@é",null,"http://%C3%A9@xn--9ca/","http:","%C3%A9","","xn--9ca","xn--9ca","","/","",""]
]"###;

#[test]
fn wpt_url_cases() {
    let mut app = app();
    let js = format!(
        r#"
        const cases = {URL_CASES};
        const keys = ["href", "protocol", "username", "password", "host", "hostname", "port", "pathname", "search", "hash"];
        const failed = [];
        for (const c of cases) {{
          const [input, base] = c;
          let u;
          try {{
            u = base === null ? new URL(input) : new URL(input, base);
          }} catch (e) {{
            if (c.length !== 2 || !(e instanceof TypeError)) failed.push([input, base, String(e)]);
            continue;
          }}
          if (c.length === 2) {{
            failed.push([input, base, "parsed as " + u.href]);
            continue;
          }}
          keys.forEach((k, i) => {{
            if (u[k] !== c[i + 2]) failed.push([input, base, k, u[k], c[i + 2]]);
          }});
          if (URL.canParse(input, base ?? undefined) !== true) failed.push([input, base, "canParse"]);
        }}
        console.log("url " + cases.length + " " + JSON.stringify(failed));
        "#
    );
    load_run(&mut app, &[("url.js", &js)]);
    let got = tagged(&app, "url ");
    assert_eq!(got.len(), 1, "{:?}", messages(&app));
    let (n, failed) = got[0].split_once(' ').unwrap();
    assert!(n.parse::<usize>().unwrap() > 90, "{n} cases");
    assert_eq!(failed, "[]");
}

#[test]
fn url_api_and_search_params() {
    let mut app = app();
    load_run(
        &mut app,
        &[(
            "api.js",
            r#"
            const out = {};
            out.stream = new URL("/api/stream", "http://127.0.0.1:4477").toString();
            const u = new URL("http://User:Pw@EXAMPLE.com:80/a/./b/../c?x=1&y=a+b#frag");
            out.parts = [u.href, u.origin, u.host, u.port, u.pathname, u.search, u.hash, u.username, u.password];
            u.protocol = "ws:";
            u.pathname = "/api/stream";
            out.ws = u.href;
            out.wsOrigin = new URL("ws://127.0.0.1:4477/x").origin;
            out.y = u.searchParams.get("y");
            u.searchParams.append("k", "a b&c/é");
            out.linked = u.search;
            u.search = "?z=9";
            out.relinked = [u.searchParams.get("z"), u.searchParams.has("x")];
            u.searchParams.delete("z");
            out.emptied = u.href;
            const p = new URLSearchParams({ b: "2", a: "1" });
            p.append("a", "0");
            p.sort();
            out.sorted = p.toString();
            out.pairs = [...new URLSearchParams([["q", "1"], ["q", "2"]]).getAll("q")];
            out.json = JSON.stringify({ u: new URL("http://x") });
            out.canParse = [URL.canParse("a", "http://h/"), URL.canParse("a"), URL.canParse("http://[::1")];
            try { new URL("not a url"); out.bad = "parsed"; } catch (e) { out.bad = e.constructor.name; }
            out.file = new URL("../d", "file:///C:/a/b/c").href;
            out.ipv6 = new URL("http://[0:0:0:0:0:0:0:1]:8080/").host;
            out.ipv4 = new URL("http://0x7f.1/").hostname;
            console.log("api " + JSON.stringify(out));
            "#,
        )],
    );
    let got: serde_json::Value = serde_json::from_str(&tagged(&app, "api ")[0]).unwrap();
    assert_eq!(got["stream"], "http://127.0.0.1:4477/api/stream");
    assert_eq!(
        got["parts"],
        serde_json::json!([
            "http://User:Pw@example.com/a/c?x=1&y=a+b#frag",
            "http://example.com",
            "example.com",
            "",
            "/a/c",
            "?x=1&y=a+b",
            "#frag",
            "User",
            "Pw"
        ])
    );
    assert_eq!(got["ws"], "ws://User:Pw@example.com/api/stream?x=1&y=a+b#frag");
    assert_eq!(got["wsOrigin"], "ws://127.0.0.1:4477");
    assert_eq!(got["y"], "a b");
    assert_eq!(got["linked"], "?x=1&y=a+b&k=a+b%26c%2F%C3%A9");
    assert_eq!(got["relinked"], serde_json::json!(["9", false]));
    assert_eq!(got["emptied"], "ws://User:Pw@example.com/api/stream#frag");
    assert_eq!(got["sorted"], "a=1&a=0&b=2");
    assert_eq!(got["pairs"], serde_json::json!(["1", "2"]));
    assert_eq!(got["json"], r#"{"u":"http://x/"}"#);
    assert_eq!(got["canParse"], serde_json::json!([true, false, false]));
    assert_eq!(got["bad"], "TypeError");
    assert_eq!(got["file"], "file:///C:/a/d");
    assert_eq!(got["ipv6"], "[::1]:8080");
    assert_eq!(got["ipv4"], "127.0.0.1");
}

#[test]
fn globals_shape_and_what_is_missing() {
    let mut app = app();
    load_run(
        &mut app,
        &[(
            "g.js",
            r#"
            const names = ["WebSocket", "setTimeout", "clearTimeout", "setInterval", "clearInterval", "URL", "URLSearchParams",
              "AbortController", "AbortSignal", "EventTarget", "Event", "CloseEvent", "MessageEvent", "DOMException", "process"];
            const out = {
              present: names.filter((n) => typeof globalThis[n] !== "undefined"),
              enumerable: names.filter((n) => Object.keys(globalThis).includes(n)),
              fetch: typeof fetch,
              blob: typeof Blob,
              argv: typeof process.argv,
              hooks: typeof globalThis.__bevytsWeb,
              envFrozen: Object.isFrozen(process.env),
              envSame: process.env === globalThis.process.env,
            };
            const d = new DOMException("gone", "InvalidStateError");
            out.dom = [d instanceof Error, d.name, d.code, d.message, String(d.stack).startsWith("InvalidStateError: gone"), DOMException.ABORT_ERR];
            const e = new CloseEvent("close", { code: 4001, reason: "r", wasClean: true });
            const m = new MessageEvent("message", { data: "x", origin: "ws://h" });
            out.events = [e instanceof Event, e.type, e.code, e.reason, e.wasClean, m.data, m.origin, new Event("x").cancelable];
            const t = new EventTarget();
            const seen = [];
            const once = () => seen.push("once");
            t.addEventListener("ping", once, { once: true });
            t.addEventListener("ping", once, { once: true });
            t.addEventListener("ping", { handleEvent: (ev) => seen.push("object:" + (ev.target === t)) });
            t.addEventListener("ping", () => { throw new Error("listener boom"); });
            t.addEventListener("ping", () => seen.push("after throw"));
            t.dispatchEvent(new Event("ping"));
            t.dispatchEvent(new Event("ping"));
            out.dispatch = seen;
            console.log("g " + JSON.stringify(out));
            "#,
        )],
    );
    let got: serde_json::Value = serde_json::from_str(&tagged(&app, "g ")[0]).unwrap();
    assert_eq!(got["present"].as_array().unwrap().len(), 15, "{got}");
    assert_eq!(got["enumerable"], serde_json::json!([]));
    assert_eq!(got["fetch"], "undefined");
    assert_eq!(got["blob"], "undefined");
    assert_eq!(got["argv"], "undefined");
    assert_eq!(got["hooks"], "undefined", "core.js takes the hooks off globalThis");
    assert_eq!(got["envFrozen"], true);
    assert_eq!(got["envSame"], true);
    assert_eq!(got["dom"], serde_json::json!([true, "InvalidStateError", 11, "gone", true, 20]));
    assert_eq!(got["events"], serde_json::json!([true, "close", 4001, "r", true, "x", "ws://h", false]));
    assert_eq!(
        got["dispatch"],
        serde_json::json!(["once", "object:true", "after throw", "object:true", "after throw"])
    );
    let errors: Vec<_> = logs(&app).into_iter().filter(|e| e.level == LogLevel::Error).collect();
    assert!(errors.iter().any(|e| e.message.contains("listener boom") && e.message.contains("g.js")), "{errors:?}");
}

#[test]
fn abort_controller_and_signal() {
    let mut app = timed_app(16);
    load_run(
        &mut app,
        &[(
            "abort.js",
            r#"
            const out = {};
            const c = new AbortController();
            let heard = 0;
            let handler = 0;
            c.signal.addEventListener("abort", () => heard++, { once: true });
            c.signal.onabort = () => handler++;
            c.abort(new Error("stop"));
            c.abort(new Error("again"));
            out.abort = [heard, handler, c.signal.aborted, c.signal.reason.message];
            out.instance = [c.signal instanceof AbortSignal, c.signal instanceof EventTarget];
            try { c.signal.throwIfAborted(); out.thrown = null; } catch (e) { out.thrown = e.message; }
            const fresh = new AbortController().signal;
            fresh.throwIfAborted();
            out.fresh = [fresh.aborted, fresh.reason];
            const a = AbortSignal.abort();
            out.defaultReason = [a.aborted, a.reason instanceof DOMException, a.reason.name];
            try { new AbortSignal(); out.ctor = "made"; } catch (e) { out.ctor = e.constructor.name; }
            out.any = AbortSignal.any([fresh, c.signal]).reason.message;
            const removed = new AbortController();
            let removedHeard = 0;
            const off = new AbortController();
            removed.signal.addEventListener("abort", () => removedHeard++, { signal: off.signal });
            off.abort();
            removed.abort();
            out.removedBySignal = removedHeard;
            console.log("abort " + JSON.stringify(out));
            const t = AbortSignal.timeout(50);
            t.addEventListener("abort", () => console.log("timeout " + t.reason.name + " " + (t.reason instanceof DOMException)));
            "#,
        )],
    );
    let got: serde_json::Value = serde_json::from_str(&tagged(&app, "abort ")[0]).unwrap();
    assert_eq!(got["abort"], serde_json::json!([1, 1, true, "stop"]));
    assert_eq!(got["instance"], serde_json::json!([true, true]));
    assert_eq!(got["thrown"], "stop");
    assert_eq!(got["fresh"], serde_json::json!([false, null]));
    assert_eq!(got["defaultReason"], serde_json::json!([true, true, "AbortError"]));
    assert_eq!(got["ctor"], "TypeError");
    assert_eq!(got["any"], "stop");
    assert_eq!(got["removedBySignal"], 0);

    updates(&mut app, 1);
    assert!(tagged(&app, "timeout ").is_empty(), "not before 50 ms of real time");
    updates(&mut app, 6);
    assert_eq!(tagged(&app, "timeout "), vec!["TimeoutError true"]);
}

#[test]
fn timers_order_ids_args_and_clearing() {
    let mut app = timed_app(16);
    load_run(
        &mut app,
        &[(
            "timers.js",
            r#"
            const log = (s) => console.log("t " + s);
            const a = setTimeout((x, y) => log(`a ${x} ${y}`), 40, 1, 2);
            const b = setTimeout(() => log("b"), 20);
            setTimeout(() => log("negative"), -10);
            setTimeout(() => log("missing"));
            setTimeout(() => log("nan"), NaN);
            const c = setTimeout(() => log("cleared"), 10);
            clearTimeout(c);
            clearTimeout(c);
            clearTimeout(123456);
            clearTimeout(undefined);
            clearInterval("nope");
            setTimeout(() => log("tie 1"), 30);
            setTimeout(() => log("tie 2"), 30);
            setTimeout(() => { throw new Error("timer boom"); }, 5);
            setTimeout(() => log("after boom"), 5);
            let threw = null;
            try { setTimeout("log('x')", 0); } catch (e) { threw = e.constructor.name; }
            let n = 0;
            const iv = setInterval((tag) => { n++; log(`${tag} ${n}`); if (n === 3) clearInterval(iv); }, 16, "tick");
            const done = setTimeout(() => log("fired"), 0);
            setTimeout(() => clearTimeout(done), 100);
            console.log("ids " + JSON.stringify({ ids: [a, b, c, iv].every((i) => Number.isInteger(i) && i > 0), next: b === a + 1, threw }));
            "#,
        )],
    );
    assert_eq!(tagged(&app, "ids "), vec![r#"{"ids":true,"next":true,"threw":"TypeError"}"#]);
    updates(&mut app, 20);
    let t = tagged(&app, "t ");
    let at = |s: &str| t.iter().position(|m| m == s).unwrap_or_else(|| panic!("{s} missing from {t:?}"));
    assert!(at("negative") < at("missing") && at("missing") < at("nan") && at("nan") < at("fired"), "{t:?}");
    assert!(at("after boom") < at("b") && at("b") < at("tie 1") && at("tie 1") < at("tie 2") && at("tie 2") < at("a 1 2"), "{t:?}");
    assert!(!t.contains(&"cleared".to_string()), "{t:?}");
    assert_eq!(t.iter().filter(|m| m.starts_with("tick ")).count(), 3, "{t:?}");
    assert!(at("tick 1") < at("tick 2") && at("tick 2") < at("tick 3"));
    let errors: Vec<_> = logs(&app).into_iter().filter(|e| e.level == LogLevel::Error).map(|e| e.message).collect();
    assert!(errors.iter().any(|m| m.contains("timer boom") && m.contains("timers.js")), "{errors:?}");
}

#[test]
fn timer_fires_after_its_delay_of_real_time() {
    let mut app = timed_app(16);
    load(&mut app, &[("delay.js", r#"setTimeout(() => console.log("due"), 50);"#)]);
    updates(&mut app, 2);
    assert!(!messages(&app).contains(&"due".to_string()), "fired before 50 ms");
    updates(&mut app, 6);
    assert_eq!(messages(&app).iter().filter(|m| *m == "due").count(), 1);
}

#[test]
fn intervals_do_not_catch_up_or_spin() {
    let mut app = timed_app(16);
    load_run(
        &mut app,
        &[(
            "interval.js",
            r#"
            let slow = 0;
            let zero = 0;
            setInterval(() => console.log("slow " + ++slow), 10);
            setInterval(() => console.log("zero " + ++zero), 0);
            "#,
        )],
    );
    updates(&mut app, 4);
    let count = |app: &App, tag: &str| tagged(app, tag).len();
    let zero = count(&app, "zero ");
    updates(&mut app, 10);
    assert_eq!(count(&app, "zero "), zero + 10, "a zero interval fires once a delivery");
    let (slow, zero) = (count(&app, "slow "), count(&app, "zero "));
    // One long frame: the interval fires once and reschedules from then, with no catch-up.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(500)));
    app.update();
    assert_eq!(count(&app, "slow "), slow + 1, "{:?}", messages(&app));
    assert_eq!(count(&app, "zero "), zero + 1);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(16)));
    app.update();
    assert_eq!(count(&app, "slow "), slow + 2);
}

#[test]
fn websocket_constructor_send_and_close_rules() {
    let mut app = timed_app(16);
    let _fake = bevy_ts::net::install_fake(&mut app);
    load_run(
        &mut app,
        &[(
            "ws.js",
            r#"
            const out = {};
            const err = (f) => { try { f(); return "ok"; } catch (e) { return e instanceof DOMException ? e.name : e.constructor.name; } };
            out.ctor = [
              err(() => new WebSocket("ftp://127.0.0.1/")),
              err(() => new WebSocket("ws://127.0.0.1/#frag")),
              err(() => new WebSocket("ws://127.0.0.1/#")),
              err(() => new WebSocket("/relative")),
              err(() => new WebSocket("ws://127.0.0.1/", ["a", "a"])),
              err(() => new WebSocket("ws://127.0.0.1/", "not a token")),
              err(() => new WebSocket()),
            ];
            const ws = new WebSocket("http://127.0.0.1:4477/api/stream", ["one", "two"]);
            out.url = [ws.url, new WebSocket("https://127.0.0.1/x", "p").url];
            out.deno = new WebSocket("ws://127.0.0.1:4477/", { protocols: "p", headers: { authorization: "Bearer k" } }).url;
            out.consts = [WebSocket.CONNECTING, WebSocket.OPEN, WebSocket.CLOSING, WebSocket.CLOSED, ws.OPEN, ws.CLOSED];
            out.initial = [ws.readyState, ws.protocol, ws.extensions, ws.bufferedAmount, ws.binaryType];
            ws.binaryType = "blob";
            out.binaryType = ws.binaryType;
            out.sendConnecting = err(() => ws.send("x"));
            out.badCodes = [1001, 999, 2999, 5000, 0, 1005].map((code) => err(() => ws.close(code)));
            out.stillConnecting = ws.readyState;
            out.longReason = err(() => ws.close(1000, "x".repeat(124)));
            out.longUtf8Reason = err(() => ws.close(1000, "\u00e9".repeat(62)));
            out.okReason = err(() => ws.close(4000, "x".repeat(123)));
            out.closing = ws.readyState;
            out.again = [err(() => ws.close()), err(() => ws.send("ignored"))];
            console.log("ws " + JSON.stringify(out));

            // Not in net.allow: the constructor succeeds, then error and close 1006 on delivery.
            const refused = new WebSocket("ws://example.com/");
            refused.onerror = (e) => { console.log("refused error " + e.constructor.name); throw new Error("handler boom"); };
            refused.addEventListener("error", () => console.log("refused error listener"));
            refused.onclose = (e) => console.log(`refused close ${e.constructor.name} ${e.code} ${e.wasClean} ${refused.readyState}`);
            "#,
        )],
    );
    let got: serde_json::Value = serde_json::from_str(&tagged(&app, "ws ")[0]).unwrap();
    assert_eq!(
        got["ctor"],
        serde_json::json!(["SyntaxError", "SyntaxError", "SyntaxError", "SyntaxError", "SyntaxError", "SyntaxError", "TypeError"])
    );
    assert_eq!(got["url"], serde_json::json!(["ws://127.0.0.1:4477/api/stream", "wss://127.0.0.1/x"]));
    assert_eq!(got["deno"], "ws://127.0.0.1:4477/");
    assert_eq!(got["consts"], serde_json::json!([0, 1, 2, 3, 1, 3]));
    assert_eq!(got["initial"], serde_json::json!([0, "", "", 0, "arraybuffer"]));
    assert_eq!(got["binaryType"], "arraybuffer");
    assert_eq!(got["sendConnecting"], "InvalidStateError");
    assert_eq!(got["badCodes"], serde_json::json!(vec!["InvalidAccessError"; 6]));
    assert_eq!(got["stillConnecting"], 0);
    assert_eq!(got["longReason"], "SyntaxError");
    assert_eq!(got["longUtf8Reason"], "SyntaxError");
    assert_eq!(got["okReason"], "ok");
    assert_eq!(got["closing"], 2);
    assert_eq!(got["again"], serde_json::json!(["ok", "ok"]));
    let warns: Vec<_> = logs(&app).into_iter().filter(|e| e.level == LogLevel::Warn).map(|e| e.message).collect();
    assert!(warns.iter().any(|m| m.contains("blob")), "{warns:?}");

    updates(&mut app, 3);
    assert_eq!(
        tagged(&app, "refused "),
        vec!["error Event", "error listener", "close CloseEvent 1006 false 3"],
        "{:?}",
        messages(&app)
    );
    let errors: Vec<_> = logs(&app).into_iter().filter(|e| e.level == LogLevel::Error).map(|e| e.message).collect();
    assert!(errors.iter().any(|m| m.contains("handler boom")), "{errors:?}");
}

#[test]
fn process_env_shows_only_the_named_variables_frozen() {
    let mut app = app();
    let vars = |n: &str| match n {
        "ABYME_URL" => Some("http://127.0.0.1:5000".to_string()),
        "HOME" => Some("/home/x".to_string()),
        _ => None,
    };
    let (policy, _) = bevy_ts::net::NetPolicy::from_config(&[], &["ABYME_URL".into(), "ABYME_KEY".into()], &vars);
    app.insert_resource(policy);
    load_run(
        &mut app,
        &[(
            "env.js",
            r#"
            const env = process.env;
            let write;
            try { env.ABYME_URL = "changed"; write = "allowed"; } catch (e) { write = e.constructor.name; }
            console.log("env " + JSON.stringify([env.ABYME_URL, env.ABYME_KEY ?? null, env.HOME ?? null, Object.keys(env), write]));
            globalThis.firstEnv = env;
            "#,
        )],
    );
    assert_eq!(tagged(&app, "env "), vec![r#"["http://127.0.0.1:5000",null,null,["ABYME_URL"],"TypeError"]"#]);
    load_run(&mut app, &[("env2.js", r#"console.log("same " + (process.env === globalThis.firstEnv));"#)]);
    assert_eq!(tagged(&app, "same "), vec!["true"]);
}
