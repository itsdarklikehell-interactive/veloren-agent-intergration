// Veloren Agent Dashboard — vanilla JS, polling only (no WebSockets)
(function () {
  const POLL_STATUS = 2000;
  const POLL_LOGS = 3000;
  const POLL_METRICS = 5000;
  const POLL_ANALYTICS = 5000;

  const el = (id) => document.getElementById(id);
  let latencyHistory = []; // {t: epochMs, v: avg seconds} rolling window for the sparkline
  let analyticsWindow = "all"; // selected lookback window for /api/analytics
  let apiKeyPrompted = false;

  function apiKey() {
    return window.DASHBOARD_API_KEY || localStorage.getItem("veloren_dashboard_api_key") || "";
  }

  function promptForApiKey() {
    if (apiKeyPrompted) return false;
    const key = prompt("Veloren dashboard API key required — paste your X-API-Key value (it is stored in localStorage only).");
    if (!key) return false;
    try { localStorage.setItem("veloren_dashboard_api_key", key); } catch (e) {}
    apiKeyPrompted = true;
    return true;
  }

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
    const key = apiKey();
    if (key) headers["X-API-Key"] = key;
    return fetch(url, Object.assign({}, opts, { headers: headers })).then(function (resp) {
      if (resp.status === 401 && promptForApiKey()) {
        return request(url, opts);
      }
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

  // ---- tiny vanilla-SVG chart helpers (no external dependencies) ----
  function svgPath(points) {
    return points.map(function (p, i) {
      return (i === 0 ? "M" : "L") + p.x + "," + p.y;
    }).join(" ");
  }

  function chartCoords(series, valueFn, w, h, pad) {
    var vals = series.map(valueFn);
    var vmin = Math.min.apply(null, vals);
    var vmax = Math.max.apply(null, vals);
    var range = vmax - vmin || 1;
    return series.map(function (s, i) {
      return {
        x: pad.l + (i / Math.max(1, series.length - 1)) * (w - pad.l - pad.r),
        y: h - pad.b - ((valueFn(s) - vmin) / range) * (h - pad.t - pad.b),
      };
    });
  }

  function renderLineChart(id, series, valueKey, color, labelId, labelFn) {
    var svg = el(id);
    if (!svg) return;
    var w = 100, h = 100, pad = { l: 6, r: 3, t: 4, b: 12 };
    var pts = series.filter(function (s) { return typeof s[valueKey] === "number"; });
    if (pts.length < 2) {
      svg.innerHTML = "";
      if (labelId) el(labelId).textContent = "no data";
      return;
    }
    var coords = chartCoords(pts, function (s) { return s[valueKey]; }, w, h, pad);
    if (labelId && labelFn) el(labelId).textContent = labelFn(pts);
    svg.innerHTML = '<path d="' + escapeHtml(svgPath(coords)) + '" class="line" stroke="' + color + '"/>';
  }

  function renderHealthTrend(id, series) {
    var svg = el(id);
    if (!svg) return;
    var w = 100, h = 100, pad = { l: 6, r: 3, t: 4, b: 12 };
    var pts = series.filter(function (s) { return typeof s.current === "number"; });
    if (pts.length < 2) {
      svg.innerHTML = "";
      return;
    }
    var now = chartCoords(pts, function (s) { return s.current; }, w, h, pad);
    var max = chartCoords(pts, function (s) { return typeof s.maximum === "number" ? s.maximum : s.current; }, w, h, pad);
    svg.innerHTML =
      '<path d="' + escapeHtml(svgPath(now)) + '" class="line" stroke="var(--accent)"/>' +
      '<path d="' + escapeHtml(svgPath(max)) + '" class="line line-secondary" stroke="var(--warn)"/>';
  }

  function renderHistogram(id, series) {
    var svg = el(id);
    if (!svg) return;
    var w = 100, h = 100, pad = { l: 8, r: 4, t: 4, b: 14 };
    var vals = series.map(function (s) { return s.avg_ms; }).filter(function (v) { return typeof v === "number"; });
    if (vals.length < 2) {
      svg.innerHTML = "";
      return;
    }
    var vmin = Math.min.apply(null, vals);
    var vmax = Math.max.apply(null, vals);
    var range = vmax - vmin || 1;
    var bins = 10, counts = new Array(bins).fill(0);
    vals.forEach(function (v) {
      var b = Math.min(bins - 1, Math.floor(((v - vmin) / range) * bins));
      counts[b]++;
    });
    var cmax = Math.max.apply(null, counts);
    var bw = (w - pad.l - pad.r) / bins;
    var html = "";
    counts.forEach(function (c, i) {
      var hh = cmax ? (c / cmax) * (h - pad.t - pad.b) : 0;
      html += '<rect x="' + (pad.l + i * bw) + '" y="' + (h - pad.b - hh) +
        '" width="' + (bw - 1) + '" height="' + hh + '" class="bar"/>';
    });
    svg.innerHTML = html;
  }

  function renderDonut(id, presence) {
    var svg = el(id);
    if (!svg) return;
    var items = Object.entries(presence || {}).filter(function (e) { return e[1] > 0; });
    var total = items.reduce(function (s, e) { return s + e[1]; }, 0);
    if (!total) {
      svg.innerHTML = "";
      return;
    }
    var cx = 50, cy = 50, r = 36, ir = 24;
    var colors = ["#3b82f6", "#22c55e", "#eab308", "#ef4444", "#a5f3fc", "#8b97a5"];
    var start = -Math.PI / 2, html = "";
    items.forEach(function (e, i) {
      var sweep = e[1] / total;
      var end = start + sweep * 2 * Math.PI;
      var large = sweep > 0.5 ? 1 : 0;
      var x1 = cx + r * Math.cos(start), y1 = cy + r * Math.sin(start);
      var x2 = cx + r * Math.cos(end), y2 = cy + r * Math.sin(end);
      html += '<path d="M ' + x1 + ' ' + y1 + ' A ' + r + ' ' + r + ' 0 ' + large + ' 1 ' + x2 + ' ' + y2 +
        ' L ' + (cx + ir * Math.cos(end)) + ' ' + (cy + ir * Math.sin(end)) +
        ' A ' + ir + ' ' + ir + ' 0 ' + large + ' 0 ' + (cx + ir * Math.cos(start)) + ' ' + (cy + ir * Math.sin(start)) + ' Z" ' +
        'fill="' + colors[i % colors.length] + '" class="slice"/>';
      start = end;
    });
    svg.innerHTML = html;
  }

  function fmtPct(n) {
    return typeof n === "number" ? n.toFixed(1) + "%" : "—";
  }

  function renderAnalytics(data) {
    if (!data) return;
    var obs = data.observations || {};
    var hl = data.health || {};
    setText("an-total", fmtCount(obs.total));
    setText("an-registered", obs.total ? fmtPct(obs.registered_pct) : "—");
    setText("an-health", (hl.avg_current !== null ? hl.avg_current : "—") + " / " + (hl.avg_maximum !== null ? hl.avg_maximum : "—"));
    setText("an-peak", fmtCount(obs.peak_per_minute));

    var series = data.series || {};
    renderLineChart("an-line", series.per_minute, "n", "#3b82f6", "an-line-labels", function (pts) {
      var vals = pts.map(function (p) { return p.n; });
      var sum = vals.reduce(function (a, b) { return a + b; }, 0);
      return "min: " + fmtCount(Math.min.apply(null, vals)) +
        " · avg: " + (vals.length ? (sum / vals.length).toFixed(1) : "—") +
        " · max: " + fmtCount(Math.max.apply(null, vals));
    });
    renderHealthTrend("an-health-trend", series.health);
    renderHistogram("an-latency-hist", (series && series.latency) || []);
    renderDonut("an-presence", data.presence);
  }

  function pollAnalytics() {
    // Analytics requires the API key; skip silently until one is configured.
    if (!apiKey()) return;
    request("/api/analytics?window=" + analyticsWindow).then(renderAnalytics).catch(function () {});
  }

  function downloadExport(fmt) {
    var headers = {};
    var key = apiKey();
    if (key) headers["X-API-Key"] = key;
    fetch("/api/observations/export?format=" + fmt + "&window=" + analyticsWindow, { headers: headers })
      .then(function (resp) {
        if (resp.status === 401 && promptForApiKey()) {
          return downloadExport(fmt);
        }
        if (!resp.ok) throw new Error("HTTP " + resp.status);
        return resp.blob();
      })
      .then(function (blob) {
        var url = URL.createObjectURL(blob);
        var a = document.createElement("a");
        a.href = url;
        a.download = "observations-" + new Date().toISOString().slice(0, 16) + (fmt === "csv" ? ".csv" : ".json");
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
        show("msg-export", true, "Export " + fmt.toUpperCase() + " download started.");
      })
      .catch(function (err) {
        show("msg-export", false, err.message || "export failed");
      });
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
    pollAnalytics();
    pollMetrics();
    setInterval(pollStatus, POLL_STATUS);
    setInterval(pollServerLogs, POLL_LOGS);
    setInterval(pollAgentLogs, POLL_LOGS);
    setInterval(pollObservations, POLL_LOGS);
    setInterval(pollAnalytics, POLL_ANALYTICS);
    setInterval(pollMetrics, POLL_METRICS);
  }

  document.addEventListener("DOMContentLoaded", function () {
    initTabs();
    wireButton("btn-server-start", "/api/server/start", null, "msg-server");
    wireButton("btn-server-stop", "/api/server/stop", null, "msg-server");
    wireButton("btn-agent-start", "/api/agent/start", null, "msg-agent");
    wireButton("btn-agent-stop", "/api/agent/stop", null, "msg-agent");
    wireSendCommand();
    el("btn-export-csv").addEventListener("click", function () { downloadExport("csv"); });
    el("btn-export-json").addEventListener("click", function () { downloadExport("json"); });
    var winSelect = el("an-window");
    if (winSelect) {
      winSelect.addEventListener("change", function () {
        analyticsWindow = winSelect.value;
        pollAnalytics();
      });
    }
    startPolling();
    initViewport();

  });
})();

// ===========================================================================
// Live viewport — SVG telemetry charts, agent status badges, zoom indicator.
// Only initializes on /viewport (canvas element present). Public endpoints only.

var VP_POLL = 1000;
var VP_TRAIL = 2000;
var VP_MAX_POINTS = 200;

var vpHistory = { health: [], speed: [], entities: [] };
var vpLastPos = null;   // {x, y, t}
var vpLastSpeed = null;

function vpSetBadge(id, text, state) {
  var node = el(id);
  if (!node) return;
  node.className = "badge" + (state ? " " + state : "");
  node.textContent = text;
}

function vpZoomUpdate() {
  vpSetBadge("vp-badge-zoom", "zoom: " + camScale.toFixed(2) + "\u00d7", "");
}

function vpChartSingle(id, series, getY, colorClass, dash) {
  var svg = el(id);
  if (!svg) return;
  var pad = { l: 22, r: 6, t: 8, b: 16 };
  var w = 100, h = 100;
  var pts = series.filter(function (s) { return typeof getY(s) === "number"; });
  if (pts.length < 2) { svg.innerHTML = ""; return; }
  var vals = pts.map(getY);
  var vmin = Math.min.apply(null, vals);
  var vmax = Math.max.apply(null, vals);
  var range = vmax - vmin || 1;
  var top = pad.t, bottom = h - pad.b, left = pad.l, right = w - pad.r;
  var parts = [];
  [vmin, vmin + range / 2, vmax].forEach(function (v) {
    var y = bottom - ((v - vmin) / range) * (bottom - top);
    parts.push('<line x1="' + left + '" y1="' + y + '" x2="' + right + '" y2="' + y + '" class="gridline"/>');
    parts.push('<text x="' + (left - 6) + '" y="' + (y + 3) + '" text-anchor="end" class="chart-axis">' + v.toFixed(2) + '</text>');
  });
  var path = pts.map(function (p, i) {
    return (i === 0 ? "M" : "L") +
      (left + (i / Math.max(1, pts.length - 1)) * (right - left)) + "," +
      (bottom - ((getY(p) - vmin) / range) * (bottom - top));
  }).join(" ");
  parts.push('<path d="' + path + '" class="line ' + colorClass + '"' +
    (dash ? ' stroke-dasharray="' + dash + '"' : '') + '/>');
  svg.innerHTML = parts.join("");
}

function vpChartHealth() {
  var svg = el("vp-chart-health");
  if (!svg) return;
  var pad = { l: 22, r: 6, t: 8, b: 16 };
  var w = 100, h = 100;
  var cur = vpHistory.health.filter(function (p) { return typeof p.c === "number"; });
  var mx = vpHistory.health.filter(function (p) { return typeof p.m === "number"; });
  if (cur.length < 2) { svg.innerHTML = ""; return; }
  var all = cur.map(function (p) { return p.c; }).concat(mx.map(function (p) { return p.m; }));
  var vmin = Math.min.apply(null, all);
  var vmax = Math.max.apply(null, all);
  var range = vmax - vmin || 1;
  var top = pad.t, bottom = h - pad.b, left = pad.l, right = w - pad.r;
  var parts = [];
  [vmin, vmin + range / 2, vmax].forEach(function (v) {
    var y = bottom - ((v - vmin) / range) * (bottom - top);
    parts.push('<line x1="' + left + '" y1="' + y + '" x2="' + right + '" y2="' + y + '" class="gridline"/>');
    parts.push('<text x="' + (left - 6) + '" y="' + (y + 3) + '" text-anchor="end" class="chart-axis">' + v.toFixed(0) + '</text>');
  });
  var addLine = function (getY, cls, dash) {
    var path = cur.map(function (p, i) {
      var v = getY(p);
      if (typeof v !== "number") return null;
      return (i === 0 ? "M" : "L") +
        (left + (i / Math.max(1, cur.length - 1)) * (right - left)) + "," +
        (bottom - ((v - vmin) / range) * (bottom - top));
    }).filter(function (p) { return p !== null; }).join(" ");
    if (!path) return;
    parts.push('<path d="' + path + '" class="line ' + cls + '"' +
      (dash ? ' stroke-dasharray="' + dash + '"' : '') + '/>');
  };
  addLine(function (p) { return p.c; }, "line-health");
  addLine(function (p) { return p.m; }, "line-max", "4 3");
  svg.innerHTML = parts.join("");
}

function vpRenderCharts(obs) {
  var now = Date.now();
  var health = typeof obs.health_current === "number" ? obs.health_current : null;
  var hmax = typeof obs.health_maximum === "number" ? obs.health_maximum : null;
  vpHistory.health.push({ t: now, c: health, m: hmax });
  vpHistory.speed.push({ t: now, s: vpLastSpeed });
  vpHistory.entities.push({ t: now, e: (obs.registered && !obs.is_dead) ? 1 : 0 });
  [vpHistory.health, vpHistory.speed, vpHistory.entities].forEach(function (arr) {
    if (arr.length > VP_MAX_POINTS) arr.shift();
  });
  vpChartHealth();
  vpChartSingle("vp-chart-speed", vpHistory.speed, function (p) { return p.s; }, "line-speed");
  vpChartSingle("vp-chart-entities", vpHistory.entities, function (p) { return p.e; }, "line-entities");
  var vHealth = el("vp-chart-health-vals");
  var vSpeed = el("vp-chart-speed-vals");
  var vEnt = el("vp-chart-entities-vals");
  if (vHealth) {
    vHealth.innerHTML = health === null ? "— / —" :
      health.toFixed(1) + " <span class=\"viewport-chart-unit\">/</span> " +
      (hmax === null ? "—" : hmax.toFixed(1)) + " <span class=\"viewport-chart-unit\">HP</span>";
  }
  if (vSpeed) vSpeed.innerHTML = vpLastSpeed === null ? "—" : vpLastSpeed.toFixed(1) + " <span class=\"viewport-chart-unit\">units/s</span>";
  if (vEnt) vEnt.innerHTML = ((obs.registered && !obs.is_dead) ? "1" : "0") + " <span class=\"viewport-chart-unit\">tracked</span>";
}

function vpRenderBadges(obs) {
  var registered = !!obs.registered;
  var dead = !!obs.is_dead;
  vpSetBadge("vp-badge-server", registered ? "server: connected" : "server: disconnected",
    registered ? "up" : "down");
  vpSetBadge("vp-badge-agent", registered ? (dead ? "agent: dead" : "agent: in-game") : "agent: connecting",
    registered ? (dead ? "down" : "up") : "unknown");
  if (typeof obs.health_current === "number" && typeof obs.health_maximum === "number") {
    vpSetBadge("vp-badge-health", "health: " + obs.health_current.toFixed(0) + "/" + obs.health_maximum.toFixed(0),
      dead ? "down" : "up");
  } else {
    vpSetBadge("vp-badge-health", "health: —", "");
  }
  vpSetBadge("vp-badge-reg", registered ? "registered: yes" : "registered: no", registered ? "up" : "down");
}

function formatCoord(n) {
  return typeof n === "number" ? n.toFixed(2) : "—";
}

// ---- viewport canvas/map state ----
var vCanvas, vMinimap, vCtx, vMmCtx, vEmpty;
var vElTick, vElX, vElY, vElZ, vElHealth, vElRegistered, vElPresence, vElUpdated;
var vElEvents, vElChat, vElBtnFollow;
var vTrail = [];
var vLatest = null;
var vShowTrail = true;
var vCamX = 0, vCamY = 0, vCamScale = 1;
var vFollowAgent = false;
var vCanvasW = 0, vCanvasH = 0;
var vDead = false;

var vIsDragging = false;
var vDragStartClient = { x: 0, y: 0 };
var vTouchLast = null;
var vLastTouchDist = 0;

var V_MIN_ZOOM = 0.05;
var V_MAX_ZOOM = 16;

function vClamp(v, a, b) { return Math.max(a, Math.min(b, v)); }
function vDpr() { return window.devicePixelRatio || 1; }
function vCanvasRect() { return vCanvas.getBoundingClientRect(); }

function vScreenToWorld(cx, cy) {
  var r = vCanvasRect();
  var d = vDpr();
  var ix = (cx - r.left) * d;
  var iy = (cy - r.top) * d;
  return {
    x: vCamX + (ix - vCanvasW / 2) / vCamScale,
    y: vCamY + (iy - vCanvasH / 2) / vCamScale,
  };
}

function vResize() {
  var rect = vCanvas.parentElement.getBoundingClientRect();
  var d = vDpr();
  vCanvasW = rect.width * d;
  vCanvasH = rect.height * d;
  vCanvas.width = vCanvasW;
  vCanvas.height = vCanvasH;
  vCanvas.style.width = rect.width + "px";
  vCanvas.style.height = rect.height + "px";
  vCtx.setTransform(d, 0, 0, d, 0, 0);
  vCamScale = 1;
  vCamX = 0;
  vCamY = 0;
  var mr = vMinimap.getBoundingClientRect();
  vMinimap.width = mr.width * d;
  vMinimap.height = mr.height * d;
  vMmCtx.setTransform(d, 0, 0, d, 0, 0);
  vpZoomUpdate();
}

function vWorldToCanvas(wx, wy) {
  return {
    x: (wx - vCamX) * vCamScale + vCanvasW / 2,
    y: (wy - vCamY) * vCamScale + vCanvasH / 2,
  };
}

function vCenterOn(wx, wy) {
  vCamX = wx;
  vCamY = wy;
}

function vFitToTrail() {
  if (vTrail.length < 2) return;
  var minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  for (var i = 0; i < vTrail.length; i++) {
    var p = vTrail[i];
    minX = Math.min(minX, p.x);
    maxX = Math.max(maxX, p.x);
    minY = Math.min(minY, p.y);
    maxY = Math.max(maxY, p.y);
  }
  var pad = 40;
  var spanX = maxX - minX + pad * 2;
  var spanY = maxY - minY + pad * 2;
  var scale = Math.min(vCanvasW / spanX, vCanvasH / spanY) * 0.9;
  vCamScale = Math.max(0.05, Math.min(scale, 5));
  vCamX = (minX + maxX) / 2;
  vCamY = (minY + maxY) / 2;
  vpZoomUpdate();
}

function vObservedBounds() {
  var minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  for (var i = 0; i < vTrail.length; i++) {
    var p = vTrail[i];
    minX = Math.min(minX, p.x);
    maxX = Math.max(maxX, p.x);
    minY = Math.min(minY, p.y);
    maxY = Math.max(maxY, p.y);
  }
  if (!vTrail.length && vLatest) {
    minX = maxX = vLatest.x;
    minY = maxY = vLatest.y;
  }
  if (!vTrail.length && !vLatest) {
    return { minX: -200, minY: -200, maxX: 200, maxY: 200 };
  }
  var pad = 60;
  return { minX: minX - pad, minY: minY - pad, maxX: maxX + pad, maxY: maxY + pad };
}

function vDrawGrid() {
  vCtx.save();
  vCtx.strokeStyle = "#1f2937";
  vCtx.lineWidth = 1 / vDpr();
  vCtx.beginPath();
  var grid = 100;
  var startX = Math.floor((vCamX - vCanvasW / (2 * vCamScale)) / grid) * grid;
  var startY = Math.floor((vCamY - vCanvasH / (2 * vCamScale)) / grid) * grid;
  var endX = Math.ceil((vCamX + vCanvasW / (2 * vCamScale)) / grid) * grid;
  var endY = Math.ceil((vCamY + vCanvasH / (2 * vCamScale)) / grid) * grid;
  var ch = vCanvasH / vDpr();
  var cw = vCanvasW / vDpr();
  for (var gx = startX; gx <= endX; gx += grid) {
    var p = vWorldToCanvas(gx, 0);
    vCtx.moveTo(p.x, 0);
    vCtx.lineTo(p.x, ch);
  }
  for (var gy = startY; gy <= endY; gy += grid) {
    var p = vWorldToCanvas(0, gy);
    vCtx.moveTo(0, p.y);
    vCtx.lineTo(cw, p.y);
  }
  vCtx.stroke();
  vCtx.restore();
}

function vDrawTrail() {
  if (!vShowTrail || vTrail.length < 2) return;
  vCtx.save();
  vCtx.lineWidth = 3 / vDpr();
  vCtx.lineCap = "round";
  vCtx.lineJoin = "round";
  for (var i = 1; i < vTrail.length; i++) {
    var a = vWorldToCanvas(vTrail[i - 1].x, vTrail[i - 1].y);
    var b = vWorldToCanvas(vTrail[i].x, vTrail[i].y);
    var alpha = i / vTrail.length;
    vCtx.strokeStyle = "rgba(59, 130, 246, " + (0.15 + 0.55 * alpha) + ")";
    vCtx.beginPath();
    vCtx.moveTo(a.x, a.y);
    vCtx.lineTo(b.x, b.y);
    vCtx.stroke();
  }
  vCtx.restore();
}

function vDrawAgent(wx, wy, heading) {
  var d = vDpr();
  var size = 12 / d;
  var p = vWorldToCanvas(wx, wy);
  vCtx.save();
  vCtx.translate(p.x, p.y);
  if (heading) vCtx.rotate(heading);
  vCtx.fillStyle = "#3b82f6";
  vCtx.beginPath();
  vCtx.moveTo(0, -size * 1.5);
  vCtx.lineTo(size * 0.8, size * 0.8);
  vCtx.lineTo(size * 0.3, size * 0.3);
  vCtx.lineTo(-size * 0.3, size * 0.3);
  vCtx.lineTo(-size * 0.8, size * 0.8);
  vCtx.closePath();
  vCtx.fill();
  vCtx.fillStyle = "#ffffff";
  vCtx.beginPath();
  vCtx.arc(0, 0, size * 0.5, 0, Math.PI * 2);
  vCtx.fill();
  if (vDead) {
    vCtx.strokeStyle = "#ef4444";
    vCtx.lineWidth = 2 / d;
    vCtx.beginPath();
    vCtx.arc(0, 0, size * 1.4, 0, Math.PI * 2);
    vCtx.stroke();
  }
  vCtx.restore();
}

function vDrawMinimap() {
  var d = vDpr();
  var mw = vMinimap.clientWidth;
  var mh = vMinimap.clientHeight;
  if (!mw || !mh) return;
  vMmCtx.clearRect(0, 0, mw, mh);
  var bounds = vObservedBounds();
  var spanX = bounds.maxX - bounds.minX;
  var spanY = bounds.maxY - bounds.minY;
  var mmScale = Math.min(mw / spanX, mh / spanY) * 0.92;
  var mmOx = (bounds.minX + bounds.maxX) / 2;
  var mmOy = (bounds.minY + bounds.maxY) / 2;
  function vMmPos(wx, wy) {
    return {
      x: (wx - mmOx) * mmScale + mw / 2,
      y: (wy - mmOy) * mmScale + mh / 2,
    };
  }
  vMmCtx.strokeStyle = "#334155";
  vMmCtx.lineWidth = 1 / d;
  vMmCtx.strokeRect(
    (bounds.minX - mmOx) * mmScale + mw / 2,
    (bounds.minY - mmOy) * mmScale + mh / 2,
    spanX * mmScale,
    spanY * mmScale
  );
  vMmCtx.fillStyle = "#64748b";
  vMmCtx.font = "10px " + (getComputedStyle(document.documentElement).fontFamily || "system-ui");
  vMmCtx.fillText("world bounds", 6, 14);
  var vwW = vCanvasW / vCamScale;
  var vwH = vCanvasH / vCamScale;
  var vwX = vCamX - vwW / 2;
  var vwY = vCamY - vwH / 2;
  var vp = vMmPos(vwX, vwY);
  vMmCtx.strokeStyle = "#3b82f6";
  vMmCtx.lineWidth = 1.5 / d;
  vMmCtx.setLineDash([4 / d, 2 / d]);
  vMmCtx.strokeRect(vp.x, vp.y, vwW * mmScale, vwH * mmScale);
  vMmCtx.setLineDash([]);
  if (vLatest) {
    var a = vMmPos(vLatest.x, vLatest.y);
    vMmCtx.fillStyle = "#3b82f6";
    vMmCtx.beginPath();
    vMmCtx.arc(a.x, a.y, 3 / d, 0, Math.PI * 2);
    vMmCtx.fill();
    vMmCtx.strokeStyle = "#ffffff";
    vMmCtx.lineWidth = 1 / d;
    vMmCtx.stroke();
  }
}

function vDraw() {
  if (vFollowAgent && vLatest) {
    vCenterOn(vLatest.x, vLatest.y);
  }
  vCtx.clearRect(0, 0, vCanvasW / vDpr(), vCanvasH / vDpr());
  vDrawGrid();
  vDrawTrail();
  if (vLatest) {
    vDrawAgent(vLatest.x, vLatest.y, vLatest.heading || 0);
  }
  vDrawMinimap();
  requestAnimationFrame(vDraw);
}

function vStartDrag(cx, cy) {
  vIsDragging = true;
  vDragStartClient = { x: cx, y: cy };
  vCanvas.style.cursor = "grabbing";
}

function vEndDrag() {
  vIsDragging = false;
  vCanvas.style.cursor = "crosshair";
}

vCanvas.addEventListener("mousedown", function (e) {
  if (e.button !== 0) return;
  vStartDrag(e.clientX, e.clientY);
});
window.addEventListener("mousemove", function (e) {
  if (!vIsDragging) return;
  var d = vDpr();
  var dx = (e.clientX - vDragStartClient.x) * d / vCamScale;
  var dy = (e.clientY - vDragStartClient.y) * d / vCamScale;
  vCamX -= dx;
  vCamY -= dy;
  vDragStartClient = { x: e.clientX, y: e.clientY };
  vFollowAgent = false;
  vElBtnFollow.setAttribute("aria-pressed", "false");
  vElBtnFollow.classList.remove("on");
});
window.addEventListener("mouseup", vEndDrag);
vCanvas.addEventListener("mouseleave", vEndDrag);

vCanvas.addEventListener("wheel", function (e) {
  e.preventDefault();
  var d = vDpr();
  var r = vCanvasRect();
  var before = vScreenToWorld(e.clientX, e.clientY);
  var factor = Math.pow(0.996, e.deltaY);
  var s = vCamScale * factor;
  s = vClamp(s, V_MIN_ZOOM, V_MAX_ZOOM);
  vCamScale = s;
  vCamX = before.x - (e.clientX - r.left - r.width / 2) * d / vCamScale;
  vCamY = before.y - (e.clientY - r.top - r.height / 2) * d / vCamScale;
  vpZoomUpdate();
}, { passive: false });

vCanvas.addEventListener("touchstart", function (e) {
  e.preventDefault();
  var t = e.touches;
  if (t.length === 2) {
    vLastTouchDist = Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY);
    vTouchLast = null;
  } else if (t.length === 1) {
    vTouchLast = { x: t[0].clientX, y: t[0].clientY };
    vLastTouchDist = 0;
  }
}, { passive: false });

vCanvas.addEventListener("touchmove", function (e) {
  e.preventDefault();
  var t = e.touches;
  var d = vDpr();
  var r = vCanvasRect();
  if (t.length === 2 && vLastTouchDist > 0) {
    var dist = Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY);
    var mx = (t[0].clientX + t[1].clientX) / 2;
    var my = (t[0].clientY + t[1].clientY) / 2;
    var before = vScreenToWorld(mx, my);
    var s = vCamScale * (dist / vLastTouchDist);
    s = vClamp(s, V_MIN_ZOOM, V_MAX_ZOOM);
    vCamScale = s;
    vCamX = before.x - (mx - r.left - r.width / 2) * d / vCamScale;
    vCamY = before.y - (my - r.top - r.height / 2) * d / vCamScale;
    vLastTouchDist = dist;
    vFollowAgent = false;
    vElBtnFollow.setAttribute("aria-pressed", "false");
    vElBtnFollow.classList.remove("on");
    vpZoomUpdate();
  } else if (t.length === 1 && vTouchLast) {
    var dx = (vTouchLast.x - t[0].clientX) * d / vCamScale;
    var dy = (vTouchLast.y - t[0].clientY) * d / vCamScale;
    vCamX += dx;
    vCamY += dy;
    vTouchLast = { x: t[0].clientX, y: t[0].clientY };
    vFollowAgent = false;
    vElBtnFollow.setAttribute("aria-pressed", "false");
    vElBtnFollow.classList.remove("on");
  }
}, { passive: false });

vCanvas.addEventListener("touchend", function (e) {
  e.preventDefault();
  var t = e.touches;
  if (t.length === 0) {
    vTouchLast = null;
    vLastTouchDist = 0;
  } else if (t.length === 1) {
    vTouchLast = { x: t[0].clientX, y: t[0].clientY };
    vLastTouchDist = 0;
  }
}, { passive: false });

vCanvas.addEventListener("touchcancel", function () {
  vTouchLast = null;
  vLastTouchDist = 0;
});

vMinimap.addEventListener("mousedown", function (e) {
  if (e.button !== 0) return;
  var d = vDpr();
  var r = vMinimap.getBoundingClientRect();
  var bounds = vObservedBounds();
  var spanX = bounds.maxX - bounds.minX;
  var spanY = bounds.maxY - bounds.minY;
  var mmScale = Math.min(r.width / spanX, r.height / spanY) * 0.92;
  var mmOx = (bounds.minX + bounds.maxX) / 2;
  var mmOy = (bounds.minY + bounds.maxY) / 2;
  var wx = mmOx + ((e.clientX - r.left) * d - r.width / 2) / mmScale;
  var wy = mmOy + ((e.clientY - r.top) * d - r.height / 2) / mmScale;
  vCenterOn(wx, wy);
  vFollowAgent = false;
  vElBtnFollow.setAttribute("aria-pressed", "false");
  vElBtnFollow.classList.remove("on");
});

function vRenderHud(obs) {
  if (!obs) return;
  vElTick.textContent = obs.tick !== undefined ? obs.tick : "—";
  vElX.textContent = formatCoord(obs.x);
  vElY.textContent = formatCoord(obs.y);
  vElZ.textContent = formatCoord(obs.z);

  var hc = obs.health_current;
  var hm = obs.health_maximum;
  if (hc !== undefined && hm !== undefined) {
    vElHealth.textContent = hc.toFixed(1) + " / " + hm.toFixed(1);
    vElHealth.className = "value " + (obs.is_dead ? "bad" : "ok");
  } else {
    vElHealth.textContent = "— / —";
    vElHealth.className = "value";
  }

  vElRegistered.textContent = obs.registered ? "yes" : "no";
  vElRegistered.className = "value " + (obs.registered ? "ok" : "bad");
  vElPresence.textContent = obs.presence || "—";

  var now = new Date().toLocaleTimeString();
  vElUpdated.textContent = "Updated " + now;

  vpRenderBadges(obs);
  vpRenderCharts(obs);
}

function vRenderEvents(obs) {
  var events = obs.events || [];
  if (events.length === 0) {
    vElEvents.textContent = "no recent events";
    return;
  }
  vElEvents.innerHTML = events
    .slice()
    .reverse()
    .map(function (e) {
      return '<div class="viewport-event event"><span class="time">[event]</span> ' +
        escapeHtml(String(e)) + '</div>';
    })
    .join("");
}

function vRenderChat(obs) {
  var chat = obs.chat || [];
  if (chat.length === 0) {
    vElChat.textContent = "no recent chat";
    return;
  }
  vElChat.innerHTML = chat
    .slice()
    .reverse()
    .map(function (c) {
      return '<div class="viewport-event viewport-chat"><span class="time">[chat]</span> ' +
        escapeHtml(String(c)) + '</div>';
    })
    .join("");
}

function pollViewportLatest() {
  request("/api/viewport/latest").then(function (data) {
    if (!data.ok) {
      vEmpty.style.display = "block";
      return;
    }
    vEmpty.style.display = "none";
    vLatest = data;
    vDead = !!data.is_dead;
    vRenderHud(data);
    vRenderEvents(data);
    vRenderChat(data);

    var now = Date.now();
    if (vpLastPos) {
      var dt = (now - vpLastPos.t) / 1000;
      var dist = Math.hypot(data.x - vpLastPos.x, data.y - vpLastPos.y);
      vpLastSpeed = (dt > 0 && dist < 5000) ? dist / dt : null;
    } else {
      vpLastSpeed = null;
    }
    vpLastPos = { x: data.x, y: data.y, t: now };

    var lastTrail = vTrail[vTrail.length - 1];
    if (!lastTrail || lastTrail.x !== data.x || lastTrail.y !== data.y) {
      vTrail.push({ x: data.x, y: data.y, z: data.z, tick: data.tick });
      if (vTrail.length > 500) vTrail.shift();
    }
  }).catch(function () {});
}

function pollViewportTrail() {
  request("/api/viewport/trail?limit=300").then(function (data) {
    if (data.ok && data.trail && data.trail.length > 0) {
      var serverTicks = new Set(vTrail.map(function (t) { return t.tick; }));
      for (var i = 0; i < data.trail.length; i++) {
        var p = data.trail[i];
        if (!serverTicks.has(p.tick)) {
          vTrail.push(p);
        }
      }
      if (vTrail.length > 500) vTrail = vTrail.slice(-500);
      if (vCamX === 0 && vCamY === 0 && vTrail.length >= 2) vFitToTrail();
    }
  }).catch(function () {});
}

function vInitViewport() {
  if (!el("viewport-canvas")) return;
  vCanvas = document.getElementById("viewport-canvas");
  vMinimap = document.getElementById("viewport-minimap");
  vCtx = vCanvas.getContext("2d");
  vMmCtx = vMinimap.getContext("2d");
  vEmpty = document.getElementById("viewport-empty");

  vElTick = el("vp-tick");
  vElX = el("vp-x");
  vElY = el("vp-y");
  vElZ = el("vp-z");
  vElHealth = el("vp-health");
  vElRegistered = el("vp-registered");
  vElPresence = el("vp-presence");
  vElUpdated = el("vp-updated");
  vElEvents = el("vp-events");
  vElChat = el("vp-chat");
  vElBtnFollow = el("vp-btn-follow");

  vResize();
  window.addEventListener("resize", vResize);

  vCanvas.addEventListener("mousedown", function (e) {
    if (e.button !== 0) return;
    vStartDrag(e.clientX, e.clientY);
  });
  window.addEventListener("mousemove", function (e) {
    if (!vIsDragging) return;
    var d = vDpr();
    var dx = (e.clientX - vDragStartClient.x) * d / vCamScale;
    var dy = (e.clientY - vDragStartClient.y) * d / vCamScale;
    vCamX -= dx;
    vCamY -= dy;
    vDragStartClient = { x: e.clientX, y: e.clientY };
    vFollowAgent = false;
    vElBtnFollow.setAttribute("aria-pressed", "false");
    vElBtnFollow.classList.remove("on");
  });
  window.addEventListener("mouseup", vEndDrag);
  vCanvas.addEventListener("mouseleave", vEndDrag);

  vCanvas.addEventListener("wheel", function (e) {
    e.preventDefault();
    var d = vDpr();
    var r = vCanvasRect();
    var before = vScreenToWorld(e.clientX, e.clientY);
    var factor = Math.pow(0.996, e.deltaY);
    var s = vCamScale * factor;
    s = vClamp(s, V_MIN_ZOOM, V_MAX_ZOOM);
    vCamScale = s;
    vCamX = before.x - (e.clientX - r.left - r.width / 2) * d / vCamScale;
    vCamY = before.y - (e.clientY - r.top - r.height / 2) * d / vCamScale;
    vpZoomUpdate();
  }, { passive: false });

  vCanvas.addEventListener("touchstart", function (e) {
    e.preventDefault();
    var t = e.touches;
    if (t.length === 2) {
      vLastTouchDist = Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY);
      vTouchLast = null;
    } else if (t.length === 1) {
      vTouchLast = { x: t[0].clientX, y: t[0].clientY };
      vLastTouchDist = 0;
    }
  }, { passive: false });

  vCanvas.addEventListener("touchmove", function (e) {
    e.preventDefault();
    var t = e.touches;
    var d = vDpr();
    var r = vCanvasRect();
    if (t.length === 2 && vLastTouchDist > 0) {
      var dist = Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY);
      var mx = (t[0].clientX + t[1].clientX) / 2;
      var my = (t[0].clientY + t[1].clientY) / 2;
      var before = vScreenToWorld(mx, my);
      var s = vCamScale * (dist / vLastTouchDist);
      s = vClamp(s, V_MIN_ZOOM, V_MAX_ZOOM);
      vCamScale = s;
      vCamX = before.x - (mx - r.left - r.width / 2) * d / vCamScale;
      vCamY = before.y - (my - r.top - r.height / 2) * d / vCamScale;
      vLastTouchDist = dist;
      vFollowAgent = false;
      vElBtnFollow.setAttribute("aria-pressed", "false");
      vElBtnFollow.classList.remove("on");
      vpZoomUpdate();
    } else if (t.length === 1 && vTouchLast) {
      var dx = (vTouchLast.x - t[0].clientX) * d / vCamScale;
      var dy = (vTouchLast.y - t[0].clientY) * d / vCamScale;
      vCamX += dx;
      vCamY += dy;
      vTouchLast = { x: t[0].clientX, y: t[0].clientY };
      vFollowAgent = false;
      vElBtnFollow.setAttribute("aria-pressed", "false");
      vElBtnFollow.classList.remove("on");
    }
  }, { passive: false });

  vCanvas.addEventListener("touchend", function (e) {
    e.preventDefault();
    var t = e.touches;
    if (t.length === 0) {
      vTouchLast = null;
      vLastTouchDist = 0;
    } else if (t.length === 1) {
      vTouchLast = { x: t[0].clientX, y: t[0].clientY };
      vLastTouchDist = 0;
    }
  }, { passive: false });

  vCanvas.addEventListener("touchcancel", function () {
    vTouchLast = null;
    vLastTouchDist = 0;
  });

  vMinimap.addEventListener("mousedown", function (e) {
    if (e.button !== 0) return;
    var d = vDpr();
    var r = vMinimap.getBoundingClientRect();
    var bounds = vObservedBounds();
    var spanX = bounds.maxX - bounds.minX;
    var spanY = bounds.maxY - bounds.minY;
    var mmScale = Math.min(r.width / spanX, r.height / spanY) * 0.92;
    var mmOx = (bounds.minX + bounds.maxX) / 2;
    var mmOy = (bounds.minY + bounds.maxY) / 2;
    var wx = mmOx + ((e.clientX - r.left) * d - r.width / 2) / mmScale;
    var wy = mmOy + ((e.clientY - r.top) * d - r.height / 2) / mmScale;
    vCenterOn(wx, wy);
    vFollowAgent = false;
    vElBtnFollow.setAttribute("aria-pressed", "false");
    vElBtnFollow.classList.remove("on");
  });

  document.getElementById("vp-btn-center").addEventListener("click", function () {
    if (vLatest) vCenterOn(vLatest.x, vLatest.y);
  });
  vElBtnFollow.addEventListener("click", function () {
    vFollowAgent = !vFollowAgent;
    var on = vFollowAgent;
    this.setAttribute("aria-pressed", String(on));
    this.classList.toggle("on", on);
    if (on && vLatest) vCenterOn(vLatest.x, vLatest.y);
  });
  document.getElementById("vp-btn-trail").addEventListener("click", function () {
    vShowTrail = !vShowTrail;
    this.textContent = vShowTrail ? "Hide trail" : "Show trail";
  });

  vDraw();
  pollViewportLatest();
  pollViewportTrail();
  setInterval(pollViewportLatest, VP_POLL);
  setInterval(pollViewportTrail, VP_TRAIL);
}
