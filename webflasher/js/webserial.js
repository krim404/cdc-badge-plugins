// Thin wrapper around the Web Serial API. Handles connect, line-buffered
// reads, and writing strings/bytes to a chosen port.

export class SerialLink {
  constructor() {
    this.port = null;
    this.reader = null;
    this.writer = null;
    this._rxBuffer = "";
    this._lineQueue = [];
    this._lineResolvers = [];
  }

  async connect(baudRate = 115200) {
    if (!("serial" in navigator)) {
      throw new Error("WebSerial not supported by this browser");
    }
    this.port = await navigator.serial.requestPort();
    await this.port.open({ baudRate });
    this.writer = this.port.writable.getWriter();
    this._startReader();
  }

  async disconnect() {
    if (this.reader) {
      try { await this.reader.cancel(); } catch {}
      this.reader = null;
    }
    if (this.writer) {
      try { await this.writer.close(); } catch {}
      this.writer = null;
    }
    if (this.port) {
      try { await this.port.close(); } catch {}
      this.port = null;
    }
  }

  isConnected() { return this.port !== null; }

  async write(data) {
    if (!this.writer) throw new Error("Not connected");
    const bytes = typeof data === "string" ? new TextEncoder().encode(data) : data;
    await this.writer.write(bytes);
  }

  async writeLine(line) {
    await this.write(line + "\n");
  }

  async readLine(timeoutMs = 5000) {
    if (this._lineQueue.length > 0) {
      return this._lineQueue.shift();
    }
    return new Promise((resolve, reject) => {
      const id = setTimeout(() => {
        const idx = this._lineResolvers.indexOf(resolver);
        if (idx >= 0) this._lineResolvers.splice(idx, 1);
        reject(new Error("readLine timeout"));
      }, timeoutMs);
      const resolver = (line) => { clearTimeout(id); resolve(line); };
      this._lineResolvers.push(resolver);
    });
  }

  async _startReader() {
    const textDecoder = new TextDecoderStream();
    const decoderClosed = this.port.readable.pipeTo(textDecoder.writable).catch(() => {});
    this.reader = textDecoder.readable.getReader();
    while (true) {
      try {
        const { value, done } = await this.reader.read();
        if (done) break;
        if (!value) continue;
        this._rxBuffer += value;
        let nl;
        while ((nl = this._rxBuffer.indexOf("\n")) >= 0) {
          const line = this._rxBuffer.slice(0, nl).replace(/\r$/, "");
          this._rxBuffer = this._rxBuffer.slice(nl + 1);
          if (this._lineResolvers.length > 0) {
            this._lineResolvers.shift()(line);
          } else {
            this._lineQueue.push(line);
          }
        }
      } catch {
        break;
      }
    }
    decoderClosed.catch(() => {});
  }
}
