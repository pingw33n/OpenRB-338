// The web player's AudioWorkletProcessor (task-171, docs/web.md). orb_player::web loads this
// script into the AudioContext's worklet (from a Blob URL) and makes the node with the
// worklet's wasm module (crates/orb-worklet) as bytes in processorOptions.wasm. The module
// imports nothing; its C ABI is described in crates/orb-worklet/src/lib.rs.
//
// Messages from the UI (ArrayBuffers of orb_player::wire) are copied into the module as they
// arrive, between render quanta; process() renders one quantum and posts what the module has
// for the UI. A trap of the module ends the playing: an Error message (wire tag 4) goes to the
// UI and the node outputs silence from then on.
//
// Every second the processor also posts {stats: {...}} (a plain object, not a wire message):
// the quanta rendered and the longest gap between two process() calls by Date.now(), for the
// test page (crates/orb-web-demo).

class OrbProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const o = options.processorOptions;
    this.queue = [];
    this.ex = null;
    this.dead = false;
    this.quanta = 0;
    this.lastCall = 0;
    this.maxGap = 0;
    this.statsAt = 0;
    this.port.onmessage = (e) => this.onMessage(e.data);
    try {
      const module = new WebAssembly.Module(o.wasm);
      this.start(new WebAssembly.Instance(module, {}), o.rate);
    } catch (e) {
      // a synchronous compile refused: compile asynchronously and queue until then
      WebAssembly.instantiate(o.wasm, {}).then(
        (r) => this.start(r.instance, o.rate),
        (err) => this.fail("the worklet's wasm module: " + err),
      );
    }
  }

  start(instance, rate) {
    this.ex = instance.exports;
    this.call(() => this.ex.orb_init(rate));
    for (const m of this.queue) this.take(m);
    this.queue = [];
    this.drain();
  }

  // A call into the module; a trap ends it.
  call(f) {
    if (this.dead) return undefined;
    try {
      return f();
    } catch (e) {
      let msg = String(e);
      try {
        const n = this.ex.orb_panic_len();
        if (n > 0) {
          const b = new Uint8Array(this.ex.memory.buffer, this.ex.orb_panic_ptr(), n);
          msg = new TextDecoderLite().decode(b) + " (" + msg + ")";
        }
      } catch (_) {}
      this.fail(msg);
      return undefined;
    }
  }

  fail(msg) {
    this.dead = true;
    // wire::FromWorklet::Error: tag 4, the u32 length, the UTF-8 bytes
    const text = new TextEncoderLite().encode(msg);
    const b = new Uint8Array(5 + text.length);
    b[0] = 4;
    new DataView(b.buffer).setUint32(1, text.length, true);
    b.set(text, 5);
    this.port.postMessage(b.buffer, [b.buffer]);
  }

  onMessage(data) {
    if (!(data instanceof ArrayBuffer)) return;
    if (this.ex === null) this.queue.push(data);
    else this.take(data);
  }

  take(buf) {
    const n = buf.byteLength;
    this.call(() => {
      const p = this.ex.orb_buffer(n);
      new Uint8Array(this.ex.memory.buffer, p, n).set(new Uint8Array(buf));
      this.ex.orb_receive(n);
    });
  }

  // Posts the module's messages for the UI.
  drain() {
    for (;;) {
      const n = this.call(() => this.ex.orb_outgoing());
      if (!n) return;
      const src = new Uint8Array(this.ex.memory.buffer, this.ex.orb_outgoing_ptr(), n);
      const b = new Uint8Array(n);
      b.set(src);
      this.port.postMessage(b.buffer, [b.buffer]);
    }
  }

  process(inputs, outputs) {
    const out = outputs[0];
    const frames = out[0].length;
    const now = Date.now();
    if (this.lastCall > 0) this.maxGap = Math.max(this.maxGap, now - this.lastCall);
    this.lastCall = now;
    if (this.ex !== null && !this.dead) {
      const p = this.call(() => this.ex.orb_render(frames));
      if (p) {
        const pcm = new Float32Array(this.ex.memory.buffer, p, 2 * frames);
        out[0].set(pcm.subarray(0, frames));
        if (out.length > 1) out[1].set(pcm.subarray(frames, 2 * frames));
        this.quanta++;
      }
      this.drain();
    }
    if (now - this.statsAt >= 1000) {
      this.statsAt = now;
      this.port.postMessage({ stats: { quanta: this.quanta, maxGapMs: this.maxGap, dead: this.dead } });
      this.maxGap = 0;
    }
    return true;
  }
}

// AudioWorkletGlobalScope has no TextEncoder or TextDecoder in every browser: UTF-8 by hand
// (only error texts go through them).
class TextEncoderLite {
  encode(s) {
    const out = [];
    for (const ch of s) {
      let c = ch.codePointAt(0);
      if (c < 0x80) out.push(c);
      else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63));
      else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
      else
        out.push(
          0xf0 | (c >> 18),
          0x80 | ((c >> 12) & 63),
          0x80 | ((c >> 6) & 63),
          0x80 | (c & 63),
        );
    }
    return new Uint8Array(out);
  }
}

class TextDecoderLite {
  decode(b) {
    let s = "";
    for (let i = 0; i < b.length; ) {
      const c = b[i];
      let cp, n;
      if (c < 0x80) [cp, n] = [c, 1];
      else if (c < 0xe0) [cp, n] = [c & 31, 2];
      else if (c < 0xf0) [cp, n] = [c & 15, 3];
      else [cp, n] = [c & 7, 4];
      for (let k = 1; k < n; k++) cp = (cp << 6) | (b[i + k] & 63);
      s += String.fromCodePoint(cp);
      i += n;
    }
    return s;
  }
}

registerProcessor("orb-engine", OrbProcessor);
