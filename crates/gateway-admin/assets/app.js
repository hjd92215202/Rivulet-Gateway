async function loadOverview() {
  const response = await fetch("/__admin/api/overview", {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw new Error(`overview request failed with ${response.status}`);
  }
  return response.json();
}

function renderStats(containerId, entries) {
  const container = document.getElementById(containerId);
  container.innerHTML = entries
    .map(
      ([label, value]) => `
        <div class="stat">
          <div class="stat-label">${label}</div>
          <div class="stat-value">${value}</div>
        </div>
      `
    )
    .join("");
}

function renderPills(items) {
  if (!items || items.length === 0) {
    return `<span class="warning">none</span>`;
  }
  return items.map((item) => `<span class="pill">${item}</span>`).join("");
}

function renderTable(containerId, columns, rows) {
  const container = document.getElementById(containerId);
  const head = columns.map((column) => `<th>${column.label}</th>`).join("");
  const body = rows
    .map((row) => {
      const cells = columns
        .map((column) => `<td data-label="${column.label}">${column.render(row)}</td>`)
        .join("");
      return `<tr>${cells}</tr>`;
    })
    .join("");

  container.innerHTML = `
    <table>
      <thead><tr>${head}</tr></thead>
      <tbody>${body}</tbody>
    </table>
  `;
}

async function refresh() {
  const overview = await loadOverview();

  renderStats("summary-grid", [
    ["listeners", overview.summary.listeners],
    ["routes", overview.summary.routes],
    ["upstreams", overview.summary.upstreams],
    ["worker threads", overview.summary.worker_threads],
  ]);

  renderStats("stats-grid", [
    ["total requests", overview.stats.total_requests],
    ["completed", overview.stats.completed_requests],
    ["active connections", overview.stats.active_connections],
    ["2xx/3xx", overview.stats.successful_responses],
    ["4xx", overview.stats.client_error_responses],
    ["5xx", overview.stats.server_error_responses],
    ["retries", overview.stats.upstream_retries],
  ]);

  renderStats("runtime-grid", [
    ["shutdown secs", overview.runtime.graceful_shutdown_secs],
    ["downstream read ms", overview.runtime.downstream_read_timeout_ms],
    ["upstream connect ms", overview.runtime.upstream_connect_timeout_ms],
    ["upstream read ms", overview.runtime.upstream_read_timeout_ms],
    ["retry attempts", overview.runtime.upstream_retry_attempts],
    ["idle pool size", overview.runtime.upstream_idle_pool_size],
  ]);

  renderTable(
    "listeners-table",
    [
      { label: "name", render: (row) => row.name },
      { label: "address", render: (row) => row.address },
      { label: "protocol", render: (row) => row.protocol },
    ],
    overview.listeners
  );

  renderTable(
    "routes-table",
    [
      { label: "name", render: (row) => row.name },
      { label: "listener", render: (row) => row.listener },
      { label: "hosts", render: (row) => renderPills(row.hosts) },
      { label: "paths", render: (row) => renderPills(row.path_prefixes) },
      { label: "methods", render: (row) => renderPills(row.methods) },
      { label: "upstream", render: (row) => row.upstream },
    ],
    overview.routes
  );

  renderTable(
    "upstreams-table",
    [
      { label: "name", render: (row) => row.name },
      { label: "load balance", render: (row) => row.load_balance },
      { label: "endpoints", render: (row) => renderPills(row.endpoints) },
    ],
    overview.upstreams
  );
}

document.getElementById("refresh-button").addEventListener("click", () => {
  refresh().catch((error) => {
    console.error(error);
    alert(`刷新失败: ${error.message}`);
  });
});

refresh().catch((error) => {
  console.error(error);
  alert(`初始化失败: ${error.message}`);
});
