// Veloren Agent Dashboard — vanilla JS, polling only (no WebSockets)
(function () {
  const POLL_STATUS = 2000;
  const POLL_LOGS = 3000;
  const POLL_METRICS = 5000;

  const el = (id) => document.getElementById(id);
  let latencyHistory = []; // {t: epochMs, v: avg seconds} rolling window for the sparkline

  function setChip(id, state, label) {
    const chip = el(id);
    if (!chip) return;
    chip.className = "chip " + (state === "up" ? "up" : state === "down" ? "down" : "unknown");
    chip.textContent = label;
  }

  function setText(id, html) {
    const node = el(id);
    if (node) node.innerHTML = html;
  }

  function okText(text) { return '<span class="chip up">&bull;</span> ' + escapeHtml(text); }
  function badText(text) { return '<span class="chip down">&bull;</span> ' + escapeHtml(text); }

  function escapeHtml(str) {
    if (str === undefined || str === null) return "";
    return String(str)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }

  function fmtCount(n) {
    return typeof n === "number" ? String(n) : "—";
  }

  function fmtMs(seconds) {
    return typeof seconds === "number" ? (seconds * 1000).toFixed(2) + " ms" : "—";
  }

  function fmtUptime(seconds) {
    if (typeof seconds !== "number") return "—";
    const m = Math.floor(seconds / 60);
    const s = Math.floor(seconds % 60);
    return m > 0 ? m + "m " + s + "s" : s + "s";
  }

  function renderStatus(data) {
    const up = data.server_running ? "up" : "down";
    setChip("chip-server", up, data.server_running ? "server: running (tmux " + data.session + ")" : "server: not running");
    const portUp = data.port_listening ? "up" : "down";
    setChip("chip-port", portUp, data.port_listening ? "port " + data.port + ": listening" : "port " + data.port + ": not listening");

    const agentUp = data.agent_running ? "up" : "down";
    setChip("chip-agent", agentUp, data.agent_running ? "agent: running" : "agent: stopped");
    setChip("chip-registered", data.registered ? "up" : "down", data.registered ? "registered: yes" : "registered: no");

    setText("stat-server-val", data.server_running ? okText("running") : badText("not running"));
    setText("stat-port-val", data.port_listening ? okText("listening on 127.0.0.1:" + data.port) : badText("nothing listening"));
    setText("stat-agent-val", data.agent_running ? okText("running") : badText("stopped"));
    setText("stat-registered-val", data.registered ? okText("yes") : badText("no"));

    const last = data.last_observation;
    const pre = el("stat-obs");
    if (!last) {
      pre.textContent = "no observations yet — start the agent and wait for JSON-lines output.";
      return;
    }
    const summary = {
      tick: last.tick,
      registered: last.registered,
      presence: last.presence,
      position: last.position,
      health: last.health,
      chat: last.chat && last.chat.length ? last.chat.length : 0,
      events: last.events && last.events.length ? last.events.length : 0,
    };
    pre.textContent = JSON.stringify(summary, null, 2) + "\n--- raw ---\n" + JSON.stringify(last, null, 2);
  }

  function renderLogs(id, lines) {
    const pre = el(id);
    if (!pre) return;
    if (!lines || lines.length === 0) {
      pre.textContent = "(no log output)";
      return;
    }
    pre.textContent = lines.map(escapeHtml).join("\n");
    pre.scrollTop = pre.scrollHeight;
  }

  function renderObservations(obsList) {
    const container = el("obs-list");
    if (!container) return;
    if (!obsList || obsList.length === 0) {
      container.textContent = "no observations yet…";
      return;
    }
    // All values interpolated below pass through escapeHtml(), so innerHTML is XSS-safe.
    const html = obsList
      .slice()
      .reverse()
      .map(function (obs) {
        const meta =
          "tick " + escapeHtml(obs.tick) +
          " | registered: " + (obs.registered ? '<span class="obs-ok">yes</span>' : '<span class="obs-bad">no</span>') +
          " | presence: " + escapeHtml(obs.presence) +
          " | position: " + (obs.position ? "[" + obs.position.map(function (n) { return n.toFixed(2); }).join(", ") + "]" : "—") +
          " | health: " + (obs.health ? obs.health.current + "/" + obs.health.maximum : "—") +
          " | chat: " + (obs.chat && obs.chat.length ? obs.chat.length : 0) +
          " | events: " + (obs.events && obs.events.length ? obs.events.length : 0);
        return (
          '<div class="obs-item">' +
          '<div class="obs-meta">' + meta + "</div>" +
          '<div class="obs-json">' + escapeHtml(JSON.stringify(obs)) + "</div>" +
          "</div>"
        );
      })
      .join("");
    container.innerHTML = html;
  }

  // The dashboard API key is never embedded in the page. Set it once per browser
  // before calling authenticated endpoints:
  //   localStorage.setItem("veloren_dashboard_api_key", "<DASHBOARD_API_KEY>")
  function request(url, opts) {
    opts = opts || {};
    const headers = Object.assign({}, opts.headers);
    const key = localStorage.getItem("veloren_dashboard_api_key");
    if (key) headers["X-API-Key"] = key;
    return fetch(url, Object.assign({}, opts, { headers: headers })).then(function (resp) {
      if (resp.status === 401) throw new Error("HTTP 401 — dashboard API key required (send X-API-Key)");
      if (!resp.ok) throw new Error("HTTP " + resp.status);
      return resp.json();
    });
  }

  function pollStatus() {
    request("/api/status")
      .then(renderStatus)
      .catch(function () {
        setChip("chip-server", "unknown", "server: unknown");
        setChip("chip-port", "unknown", "port: unknown");
        setChip("chip-agent", "unknown", "agent: unknown");
        setChip("chip-registered", "unknown", "registered: unknown");
      });
  }

  function pollServerLogs() {
    request("/api/server-logs").then(function (data) {
      renderLogs("log-server", data.lines);
    });
  }

  function pollAgentLogs() {
    request("/api/agent-logs").then(function (data) {
      renderLogs("log-agent", data.lines);
    });
  }

  function pollObservations() {
    request("/api/observations").then(function (data) {
      renderObservations(data.observations);
    });
  }

  function renderMetrics(data) {
    if (!data) return;
    const lat = data.agent_tick_latency_seconds || {};

    setText("perf-observations", fmtCount(data.agent_observations_total));
    setText("perf-last-tick", fmtCount(data.last_observation_tick));

    const serverChip = el("perf-server");
    if (serverChip) {
      serverChip.className = "chip " + (data.server_connected ? "up" : "down");
      serverChip.textContent = data.server_connected
        ? "server: reachable"
        : "server: not reachable";
    }

    setText("perf-uptime", fmtUptime(data.agent_uptime_seconds));
    setText("perf-requests", fmtCount(data.dashboard_requests_total));
    setText("perf-lat-avg", fmtMs(lat.avg));

    const labels = el("perf-sparkline-labels");
    if (labels) {
      labels.textContent =
        "min: " +
        fmtMs(lat.min) +
        " · avg: " +
        fmtMs(lat.avg) +
        " · max: " +
        fmtMs(lat.max) +
        " (" +
        fmtCount(lat.count) +
        " samples)";
    }

    const now = Date.now();
    if (typeof lat.avg === "number") latencyHistory.push({ t: now, v: lat.avg });
    while (latencyHistory.length > 60) latencyHistory.shift();
    drawSparkline("perf-sparkline", latencyHistory);
  }

  function drawSparkline(id, history) {
    const svg = el(id);
    if (!svg) return;
    if (history.length < 2) {
      svg.innerHTML = "";
      return;
    }
    const values = history.map(function (h) { return h.v; });
    const vmin = Math.min.apply(null, values);
    const vmax = Math.max.apply(null, values);
    const range = vmax - vmin || 1;
    const points = history
      .map(function (h, i) {
        var x = 10 + (i / (history.length - 1)) * 80;
        var y = 90 - ((h.v - vmin) / range) * 80;
        return x + "," + y;
      })
      .join(" ");
    svg.innerHTML =
      '<polyline points="' + escapeHtml(points) + '" class="sparkline-line"/>';
  }

  function pollMetrics() {
    // Metrics are non-critical: fail silently (no game server required).
    request("/api/metrics").then(renderMetrics).catch(function () {});
  }

  function postJson(url, body) {
    return request(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body || {}),
    });
  }

  function show(id, ok, text) {
    const node = el(id);
    if (!node) return;
    node.className = "msg " + (ok ? "ok" : "bad");
    node.textContent = text;
  }

  function wireButton(id, url, body, msgId) {
    const btn = el(id);
    if (!btn) return;
    btn.addEventListener("click", function () {
      btn.classList.add("disabled");
      postJson(url, body)
        .then(function (data) {
          show(msgId, data.ok, data.output || data.error || (data.ok ? "ok" : "failed"));
        })
        .catch(function (err) {
          show(msgId, false, err.message || "request failed");
        })
        .finally(function () {
          btn.classList.remove("disabled");
        });
    });
  }

  function wireSendCommand() {
    const btn = el("btn-send");
    const select = el("cmd-template");
    const body = el("cmd-body");
    if (!btn) return;
    btn.addEventListener("click", function () {
      let payload;
      try {
        payload = select.value === "custom" ? JSON.parse(body.value || "{}") : JSON.parse(select.value);
      } catch (err) {
        show("msg-cmd", false, "invalid JSON: " + (err.message || err));
        return;
      }
      btn.classList.add("disabled");
      postJson("/api/agent/command", payload)
        .then(function (data) {
          show("msg-cmd", data.ok, data.output || data.error || (data.ok ? "command sent" : "failed"));
        })
        .catch(function (err) {
          show("msg-cmd", false, err.message || "request failed");
        })
        .finally(function () {
          btn.classList.remove("disabled");
        });
    });
    select.addEventListener("change", function () {
      if (select.value !== "custom") body.value = select.value;
    });
  }

  function initTabs() {
    document.querySelectorAll(".tab").forEach(function (tab) {
      tab.addEventListener("click", function (event) {
        event.preventDefault();
        document.querySelectorAll(".tab").forEach(function (t) { t.classList.remove("active"); });
        document.querySelectorAll(".panel").forEach(function (p) { p.classList.remove("active"); });
        tab.classList.add("active");
        el(tab.getAttribute("href").slice(1)).classList.add("active");
      });
    });
  }

  function startPolling() {
    pollStatus();
    pollServerLogs();
    pollAgentLogs();
    pollObservations();
    pollMetrics();
    setInterval(pollStatus, POLL_STATUS);
    setInterval(pollServerLogs, POLL_LOGS);
    setInterval(pollAgentLogs, POLL_LOGS);
    setInterval(pollObservations, POLL_LOGS);
    setInterval(pollMetrics, POLL_METRICS);
  }

  document.addEventListener("DOMContentLoaded", function () {
    initTabs();
    wireButton("btn-server-start", "/api/server/start", null, "msg-server");
    wireButton("btn-server-stop", "/api/server/stop", null, "msg-server");
    wireButton("btn-agent-start", "/api/agent/start", null, "msg-agent");
    wireButton("btn-agent-stop", "/api/agent/stop", null, "msg-agent");
    wireSendCommand();
    startPolling();
  });
})();
