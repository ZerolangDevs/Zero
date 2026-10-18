// Zero wasm runtime glue — <STEM>
// Provides the `zero` import module (console + net HTTP bridge) and runs the
// compiled `main` entry. Works in browsers (synchronous XHR) and Node.js
// (spawns a short-lived helper process that performs fetch synchronously).
//
// Usage:
//   Browser: <script src="<STEM>.js"></script>   (loads <STEM>.wasm next to it)
//   Node:    node <STEM>.js                       (auto-loads <STEM>.wasm)
//   Manual:  Zero.init("/path/to/<STEM>.wasm").then(...)
"use strict";

(function (global) {
  function readUtf8(mem, ptr, len) {
    return new TextDecoder().decode(new Uint8Array(mem.buffer, ptr, len));
  }

  // --- synchronous HTTP: browser via sync XHR, Node via helper process ---
  function syncHttp(method, url, bodyBytes) {
    if (typeof XMLHttpRequest !== "undefined") {
      var xhr = new XMLHttpRequest();
      xhr.open(method, url, false);
      xhr.overrideMimeType("text/plain; charset=x-user-defined");
      var bodyStr = null;
      if (bodyBytes.length) {
        var parts = [];
        for (var i = 0; i < bodyBytes.length; i++) {
          parts.push(String.fromCharCode(bodyBytes[i]));
        }
        bodyStr = parts.join("");
      }
      xhr.send(bodyStr);
      var body = [];
      if (xhr.responseText) {
        for (var j = 0; j < xhr.responseText.length; j++) {
          body.push(xhr.responseText.charCodeAt(j) & 0xff);
        }
      }
      return { status: xhr.status, body: body };
    }
    // Node.js: execFileSync a helper that does async fetch and returns base64.
    var cp = require("child_process");
    var helper = [
      "(async () => {",
      "  const [json] = process.argv.slice(1);",
      "  const req = JSON.parse(json);",
      "  try {",
      "    const resp = await fetch(req.url, { method: req.method,",
      "      body: req.body && req.body.length ? Buffer.from(req.body) : undefined });",
      "    const buf = new Uint8Array(await resp.arrayBuffer());",
      '    process.stdout.write(JSON.stringify({ status: resp.status, body: Buffer.from(buf).toString("base64") }));',
      "  } catch (e) {",
      '    process.stdout.write(JSON.stringify({ status: -1, body: "", error: String(e) }));',
      "  }",
      "})();",
    ].join("\n");
    var out = cp.execFileSync(
      process.execPath,
      ["-e", helper, JSON.stringify({ method: method, url: url, body: Array.from(bodyBytes) })],
      { maxBuffer: 268435456, encoding: "utf8" }
    );
    var res = JSON.parse(out);
    return {
      status: res.status,
      body: res.body ? Array.from(Buffer.from(res.body, "base64")) : [],
    };
  }

  function makeImports(state) {
    return {
      zero: {
        zero_console_log: function (ptr, len) {
          var text = readUtf8(state.mem, ptr, len);
          if (typeof console !== "undefined" && console.log) {
            console.log(text.replace(/\n+$/, ""));
          }
        },
        zero_net_http: function (mPtr, mLen, uPtr, uLen, bPtr, bLen) {
          var method = readUtf8(state.mem, mPtr, mLen);
          var url = readUtf8(state.mem, uPtr, uLen);
          var body = [];
          if (bLen) {
            body = Array.from(new Uint8Array(state.mem.buffer, bPtr, bLen));
          }
          try {
            var res = syncHttp(method, url, body);
            state.netStatus = res.status;
            state.netBody = res.body;
            return res.status;
          } catch (e) {
            state.netStatus = -1;
            state.netBody = [];
            return -1;
          }
        },
        zero_net_body_len: function () {
          return state.netBody.length;
        },
        zero_net_read: function (dstPtr, cap) {
          var n = Math.min(cap, state.netBody.length);
          if (n > 0) {
            new Uint8Array(state.mem.buffer, dstPtr, n).set(state.netBody.slice(0, n));
          }
          return n;
        },
      },
    };
  }

  function loadWasmBytes(path) {
    if (typeof XMLHttpRequest !== "undefined") {
      // Browser: synchronous XHR (works over http(s); file:// may be blocked).
      var xhr = new XMLHttpRequest();
      xhr.open("GET", path, false);
      xhr.responseType = "arraybuffer";
      xhr.send();
      if (xhr.status !== 200 && xhr.status !== 0) {
        throw new Error("cannot load wasm '" + path + "' (status " + xhr.status + ")");
      }
      return xhr.response;
    }
    // Node.js
    return require("fs").readFileSync(path);
  }

  global.Zero = {
    init: function (wasmPath) {
      if (!wasmPath && typeof document !== "undefined" && document.currentScript) {
        wasmPath = document.currentScript.src.replace(/\.js(\?.*)?$/, ".wasm");
      }
      if (!wasmPath) {
        return Promise.reject(new Error("Zero.init: wasm path required (or place <STEM>.js next to <STEM>.wasm)"));
      }
      var bytes;
      try {
        bytes = loadWasmBytes(wasmPath);
      } catch (e) {
        return Promise.reject(e);
      }
      var state = { mem: null, netStatus: 0, netBody: [] };
      return WebAssembly.instantiate(bytes, makeImports(state)).then(function (result) {
        var instance = result.instance;
        state.mem = instance.exports.memory;
        if (typeof instance.exports.main === "function") {
          instance.exports.main();
        }
        return instance;
      });
    },
  };

  // Auto-run when executed directly: `node <STEM>.js`.
  if (typeof module !== "undefined" && typeof require === "function" && module === require.main) {
    Zero.init(__filename.replace(/\.js$/, ".wasm")).catch(function (e) {
      console.error("Zero wasm error:", e && e.message ? e.message : e);
      process.exitCode = 1;
    });
  }
})(typeof self !== "undefined" ? self : globalThis);
