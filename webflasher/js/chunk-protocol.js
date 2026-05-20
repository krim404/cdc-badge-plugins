// Plugin upload protocol over the badge's USB-CDC console.
//
//   > AUTH <pin>                          (only if FEATURE_SECURE_SERIAL)
//   < OK: Authenticated
//   > PLUGIN UPLOAD[_META|_LANG] <id> <size> <crc32_hex>
//   < READY
//   > <size raw payload bytes>
//   < OK <size>     or     ERR <reason>
//
// The badge installs a byte interceptor as soon as `start_upload` runs, so
// raw payload bytes are NOT line-parsed and can contain any byte values.
// Command terminators are bare LF (\n); CRLF would be interpreted as a
// payload byte after the interceptor switch.

const CHUNK_SIZE = 4096;   // raw bytes per write; chosen for fast progress refresh
const READY_TIMEOUT_MS = 5000;
const OK_TIMEOUT_MS    = 30000;
const AUTH_TIMEOUT_MS  = 5000;

function crc32(bytes) {
  let crc = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) {
    crc = crc ^ bytes[i];
    for (let j = 0; j < 8; j++) {
      crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function hex8(n) {
  return n.toString(16).padStart(8, "0");
}

// Strip any leading "> " prompt fragments so a response like "> OK 504"
// still matches the `OK` prefix check.
function stripPrompt(line) {
  let s = line;
  while (s.startsWith(">")) s = s.slice(1).replace(/^\s+/, "");
  return s;
}

// Read response lines until one starts with any of the given prefixes,
// ignoring echoes, log lines and prompts.
async function waitFor(link, prefixes, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const remaining = deadline - Date.now();
    let line;
    try {
      line = await link.readLine(Math.max(250, remaining));
    } catch (e) {
      continue;
    }
    if (!line) continue;
    const s = stripPrompt(line.trim());
    for (const pref of prefixes) {
      if (s.startsWith(pref)) return s;
    }
  }
  return null;
}

/**
 * Authenticate the serial session. Skipped silently when `pin` is empty.
 * Throws on failure.
 */
export async function authenticate(link, pin) {
  if (!pin) return;
  await link.writeLine(`AUTH ${pin}`);
  const resp = await waitFor(link, ["OK", "ERROR"], AUTH_TIMEOUT_MS);
  if (!resp) throw new Error("AUTH timed out");
  if (!resp.startsWith("OK")) throw new Error(`AUTH failed: ${resp}`);
}

/**
 * Send `data` (Uint8Array) via `PLUGIN <command> ...` and wait for the OK
 * acknowledgement. `command` is the subcommand suffix, e.g. "UPLOAD_META".
 */
async function uploadFile(link, command, data, onProgress = () => {}) {
  const total = data.length;
  const crc = crc32(data);

  await link.writeLine(`PLUGIN ${command} ${total} ${hex8(crc)}`);

  const ready = await waitFor(link, ["READY", "ERR"], READY_TIMEOUT_MS);
  if (!ready) throw new Error("no READY response");
  if (ready.startsWith("ERR")) throw new Error(`badge refused upload: ${ready}`);

  // Stream payload. The badge consumes from its USB-CDC RX queue every
  // SerialCmd::process() tick (drains up to 4096 bytes), so writing in
  // CHUNK_SIZE blocks keeps progress smooth without backing up the queue.
  let sent = 0;
  while (sent < total) {
    const end = Math.min(sent + CHUNK_SIZE, total);
    await link.write(data.subarray(sent, end));
    sent = end;
    onProgress(sent / total);
  }

  const fin = await waitFor(link, ["OK", "ERR"], OK_TIMEOUT_MS);
  if (!fin || !fin.startsWith("OK")) {
    throw new Error(`upload not finalised: ${fin}`);
  }
  return fin;
}

/**
 * Install a plugin: meta, wasm and (optionally) lang in that order.
 * Aborts a stuck server-side session on failure so the next attempt has a
 * clean slate.
 */
export async function uploadPlugin(link, pluginId, wasmBytes, metaBytes, onProgress, langBytes = null) {
  try {
    await uploadFile(link, `UPLOAD_META ${pluginId}`, metaBytes, f => onProgress("meta", f));
    await uploadFile(link, `UPLOAD ${pluginId}`,     wasmBytes, f => onProgress("wasm", f));
    if (langBytes && langBytes.length > 0) {
      await uploadFile(link, `UPLOAD_LANG ${pluginId}`, langBytes, f => onProgress("lang", f));
    }
  } catch (err) {
    try { await link.writeLine("PLUGIN ABORT"); } catch {}
    throw err;
  }
}
