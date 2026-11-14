// The `kanso` API, evaluated inside the sandbox before the plugin's own code.
// The sandbox has no network, files, timers or DOM of its own: what a plugin can do is what
// this file exposes, and the two host functions below are checked on the Rust side.
(function () {
  "use strict";

  var hostHttp = globalThis.__kanso_http;
  var hostLog = globalThis.__kanso_log;
  var boot = JSON.parse(globalThis.__kanso_boot);
  delete globalThis.__kanso_http;
  delete globalThis.__kanso_log;
  delete globalThis.__kanso_boot;

  var definition = null;
  var storage = boot.storage;
  var storageDirty = false;
  var outcome = null;

  function describe(value) {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.name + ": " + value.message;
    try {
      var json = JSON.stringify(value);
      return json === undefined ? String(value) : json;
    } catch (_) {
      return String(value);
    }
  }

  function log() {
    hostLog(Array.prototype.map.call(arguments, describe).join(" "));
  }

  function hasHeader(headers, wanted) {
    return Object.keys(headers).some(function (name) {
      return name.toLowerCase() === wanted;
    });
  }

  function request(options) {
    if (typeof options === "string") options = { url: options };
    options = options || {};
    var payload = {
      method: String(options.method || "GET").toUpperCase(),
      url: String(options.url || ""),
      headers: {},
      body: null,
    };
    var headers = options.headers || {};
    Object.keys(headers).forEach(function (name) {
      payload.headers[name] = String(headers[name]);
    });
    if (options.json !== undefined) {
      payload.body = JSON.stringify(options.json);
      if (!hasHeader(payload.headers, "content-type")) {
        payload.headers["Content-Type"] = "application/json";
      }
    } else if (options.body !== undefined && options.body !== null) {
      payload.body = String(options.body);
    }

    var reply = JSON.parse(hostHttp(JSON.stringify(payload)));
    if (reply.error) throw new Error(reply.error);
    return {
      status: reply.status,
      ok: reply.status >= 200 && reply.status < 300,
      headers: reply.headers,
      body: reply.body,
      json: function () {
        return JSON.parse(reply.body);
      },
    };
  }

  function withUrl(method, url, options) {
    var merged = {};
    Object.keys(options || {}).forEach(function (key) {
      merged[key] = options[key];
    });
    merged.url = url;
    merged.method = method;
    return merged;
  }

  var http = Object.freeze({
    request: request,
    get: function (url, options) {
      return request(withUrl("GET", url, options));
    },
    post: function (url, options) {
      return request(withUrl("POST", url, options));
    },
    getJson: function (url, options) {
      var response = request(withUrl("GET", url, options));
      if (!response.ok) throw new Error("HTTP " + response.status + " from " + url);
      return response.json();
    },
  });

  var store = Object.freeze({
    get: function (key, fallback) {
      var raw = storage[String(key)];
      if (raw === undefined) return fallback === undefined ? null : fallback;
      try {
        return JSON.parse(raw);
      } catch (_) {
        return fallback === undefined ? null : fallback;
      }
    },
    set: function (key, value) {
      storage[String(key)] = JSON.stringify(value === undefined ? null : value);
      storageDirty = true;
    },
    remove: function (key) {
      if (Object.prototype.hasOwnProperty.call(storage, String(key))) {
        delete storage[String(key)];
        storageDirty = true;
      }
    },
  });

  // Picks the text for the interface language from {en: "...", it: "..."}.
  function pick(texts) {
    if (texts === null || texts === undefined) return "";
    if (typeof texts !== "object") return String(texts);
    if (texts[boot.locale] !== undefined) return String(texts[boot.locale]);
    if (texts.en !== undefined) return String(texts.en);
    var first = Object.keys(texts)[0];
    return first === undefined ? "" : String(texts[first]);
  }

  globalThis.kanso = Object.freeze({
    apiVersion: 1,
    appVersion: boot.appVersion,
    locale: boot.locale,
    settings: Object.freeze(boot.settings),
    http: http,
    storage: store,
    log: log,
    t: pick,
    plugin: function (hooks) {
      definition = hooks;
    },
  });
  globalThis.console = Object.freeze({ log: log, info: log, warn: log, error: log, debug: log });

  function finish(ok, value) {
    if (ok) {
      outcome = { ok: true, value: value === undefined ? null : value };
      return;
    }
    // Where it happened, for the plugin's author
    if (value instanceof Error && value.stack) hostLog(describe(value) + "\n" + String(value.stack).replace(/\s+$/, ""));
    outcome = { ok: false, error: describe(value) };
  }

  globalThis.__kanso_call = function (hook, argsJson) {
    outcome = null;
    var owner = null;
    var target = definition;
    var parts = hook.split(".");
    for (var i = 0; i < parts.length && target; i++) {
      owner = target;
      target = Object.prototype.hasOwnProperty.call(target, parts[i]) ? target[parts[i]] : undefined;
    }
    if (!definition) return finish(false, "the plugin never called kanso.plugin({...})");
    if (typeof target !== "function") return finish(false, "the plugin does not implement " + hook);
    try {
      Promise.resolve(target.call(owner, JSON.parse(argsJson))).then(
        function (value) {
          finish(true, value);
        },
        function (error) {
          finish(false, error);
        }
      );
    } catch (error) {
      finish(false, error);
    }
  };

  globalThis.__kanso_result = function () {
    if (!outcome) {
      return JSON.stringify({ ok: false, error: "the plugin did not finish: a promise never settled" });
    }
    var reply = outcome.ok ? { ok: true, value: outcome.value } : { ok: false, error: outcome.error };
    if (storageDirty) reply.storage = storage;
    try {
      return JSON.stringify(reply);
    } catch (error) {
      return JSON.stringify({ ok: false, error: "the result is not plain data: " + describe(error) });
    }
  };
})();
