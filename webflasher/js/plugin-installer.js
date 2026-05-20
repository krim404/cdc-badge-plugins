import { SerialLink } from "./webserial.js";
import { uploadPlugin, authenticate } from "./chunk-protocol.js";

const link = new SerialLink();
const logEl = document.getElementById("log");
const connectBtn = document.getElementById("connectBtn");
const connStatusEl = document.getElementById("connStatus");
const catalogEl = document.getElementById("catalog");
const pinInputEl = document.getElementById("pinInput");
let sessionAuthenticated = false;

function log(msg) {
  const ts = new Date().toLocaleTimeString();
  logEl.textContent += `[${ts}] ${msg}\n`;
  logEl.scrollTop = logEl.scrollHeight;
}

function setConnStatus(text, cls) {
  connStatusEl.textContent = text;
  connStatusEl.className = `status ${cls}`;
  connStatusEl.style.display = "block";
}

async function loadCatalog() {
  try {
    const res = await fetch("./catalog.json", { cache: "no-store" });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return await res.json();
  } catch (e) {
    log(`Failed to load catalog: ${e.message}`);
    return { plugins: [] };
  }
}

function renderCatalog(catalog) {
  while (catalogEl.firstChild) catalogEl.removeChild(catalogEl.firstChild);

  if (!catalog.plugins || catalog.plugins.length === 0) {
    const note = document.createElement("p");
    note.style.color = "var(--text-muted)";
    note.textContent = "No plugins published yet.";
    catalogEl.appendChild(note);
    return;
  }

  for (const p of catalog.plugins) {
    const row = document.createElement("div");
    row.className = "plugin";

    const info = document.createElement("div");
    info.className = "plugin-info";

    const name = document.createElement("div");
    name.className = "plugin-name";
    name.textContent = p.name;

    const desc = document.createElement("div");
    desc.className = "plugin-desc";
    desc.textContent = p.description || "";

    const meta = document.createElement("div");
    meta.className = "plugin-meta";
    meta.textContent = `v${p.version} - ${p.linear_memory_kb} KB - by ${p.author || "unknown"}`;

    const progress = document.createElement("div");
    progress.className = "progress";
    progress.style.display = "none";
    const bar = document.createElement("div");
    bar.className = "progress-bar";
    progress.appendChild(bar);

    info.appendChild(name);
    info.appendChild(desc);
    info.appendChild(meta);
    info.appendChild(progress);

    const installBtn = document.createElement("button");
    installBtn.className = "install";
    installBtn.dataset.id = p.id;
    installBtn.textContent = "Install";
    installBtn.addEventListener("click", () => installPlugin(p, row));

    row.appendChild(info);
    row.appendChild(installBtn);
    catalogEl.appendChild(row);
  }
}

async function installPlugin(plugin, rowEl) {
  if (!link.isConnected()) {
    setConnStatus("Connect to your badge first.", "err");
    return;
  }
  const installBtn = rowEl.querySelector("button.install");
  installBtn.disabled = true;
  installBtn.textContent = "Installing...";
  const progress = rowEl.querySelector(".progress");
  const bar = rowEl.querySelector(".progress-bar");
  progress.style.display = "block";

  try {
    if (!sessionAuthenticated) {
      const pin = (pinInputEl.value || "").trim();
      if (pin) {
        log("Authenticating...");
        await authenticate(link, pin);
        sessionAuthenticated = true;
      }
    }

    log(`Fetching ${plugin.wasm_url}`);
    const wasmRes = await fetch(plugin.wasm_url, { cache: "no-store" });
    if (!wasmRes.ok) throw new Error(`wasm HTTP ${wasmRes.status}`);
    const wasm = new Uint8Array(await wasmRes.arrayBuffer());

    log(`Fetching ${plugin.meta_url}`);
    const metaRes = await fetch(plugin.meta_url, { cache: "no-store" });
    if (!metaRes.ok) throw new Error(`meta HTTP ${metaRes.status}`);
    const meta = new Uint8Array(await metaRes.arrayBuffer());

    let lang = null;
    if (plugin.lang_url) {
      try {
        log(`Fetching ${plugin.lang_url}`);
        const langRes = await fetch(plugin.lang_url, { cache: "no-store" });
        if (langRes.ok) {
          lang = new Uint8Array(await langRes.arrayBuffer());
        } else {
          log(`lang HTTP ${langRes.status} - continuing without translations`);
        }
      } catch (e) {
        log(`lang fetch failed: ${e.message} - continuing without translations`);
      }
    }

    const sizeStr = lang
      ? `${wasm.length} B wasm, ${meta.length} B meta, ${lang.length} B lang`
      : `${wasm.length} B wasm, ${meta.length} B meta`;
    log(`Uploading ${plugin.id} (${sizeStr})`);

    await uploadPlugin(link, plugin.id, wasm, meta, (kind, frac) => {
      const pct = Math.round(frac * 100);
      bar.style.width = `${pct}%`;
      bar.title = `${kind}: ${pct} %`;
    }, lang);

    log(`Installed ${plugin.id}`);
    installBtn.textContent = "Installed";
  } catch (e) {
    log(`Install failed: ${e.message}`);
    installBtn.disabled = false;
    installBtn.textContent = "Install";
  } finally {
    setTimeout(() => { progress.style.display = "none"; bar.style.width = "0%"; }, 1200);
  }
}

connectBtn.addEventListener("click", async () => {
  try {
    if (link.isConnected()) {
      await link.disconnect();
      connectBtn.textContent = "Connect via Serial";
      setConnStatus("Disconnected", "info");
      return;
    }
    await link.connect();
    sessionAuthenticated = false;
    connectBtn.textContent = "Disconnect";
    setConnStatus("Connected to badge", "ok");
    log("Serial connected");
  } catch (e) {
    setConnStatus(`Connect failed: ${e.message}`, "err");
    log(`Connect failed: ${e.message}`);
  }
});

document.addEventListener("DOMContentLoaded", async () => {
  const catalog = await loadCatalog();
  renderCatalog(catalog);
  log(`Loaded catalog (${catalog.plugins?.length || 0} plugins, release ${catalog.release_version || "?"})`);
});
