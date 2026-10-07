import { chromium } from 'playwright-core';

const server = Bun.serve({
  hostname: '127.0.0.1', port: 0,
  fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<!doctype html><title>Component benchmark</title>', { headers: { 'Content-Type': 'text/html' } });
    if (!path.startsWith('/target/') || path.includes('..')) return new Response('', { status: 404 });
    return new Response(Bun.file(`${import.meta.dir}${path}`));
  },
});
let browser;
try {
  browser = await chromium.launch({ executablePath: process.env.CHROMIUM || '/etc/profiles/per-user/chris/bin/chromium', headless: true });
  const page = await browser.newPage();
  page.on('console', message => console.log(message.text()));
  await page.goto(`http://127.0.0.1:${server.port}`);
  const results = await page.evaluate(async () => {
    const start = performance.now();
    let active = (await import('/target/browser/fury.js')).fury;
    const jcoLoadMs = performance.now() - start;
    let memory, words;
    const write = (result, pointer) => {
      const bonus = active.bonusPercent(result.stacks);
      if (words?.buffer !== memory.buffer) words = new BigUint64Array(memory.buffer);
      const at = pointer / 8;
      words[at] = result.stacks;
      words[at + 1] = result.remainingMicros;
      words[at + 2] = BigInt(bonus);
    };
    const { instance } = await WebAssembly.instantiateStreaming(fetch('/target/browser/host.wasm'), {
      plugin: { hit(stacks, critical, pointer) {
        write(active.onHit(stacks, critical !== 0), pointer);
      }, hits(stacks, pointer, length, output) {
        const hits = Array.from(new Uint8Array(memory.buffer, pointer, length), Boolean);
        write(active.onHits(stacks, 1_000_000n, hits), output);
      } },
    });
    const host = instance.exports;
    memory = host.memory;
    let before = performance.now();
    host.setup_rune();
    const runeSetupMs = performance.now() - before;
    const component = new Uint8Array(await (await fetch('/target/wasm32-wasip2/release/fury_plugin.wasm')).arrayBuffer());
    const ptr = host.reserve_component(component.length);
    new Uint8Array(memory.buffer, ptr, component.length).set(component);
    before = performance.now();
    host.load_component();
    const wasmiSetupMs = performance.now() - before;
    const labels = ['Rune 0.14.1', 'Component via nested Wasmi', 'Component via browser JIT', 'Rust linked directly'];
    let checked = 0;
    const read = (backend, stacks, critical) => {
      const pointer = host.check(backend, stacks, critical);
      return Array.from(new BigUint64Array(memory.buffer, pointer, 3), String);
    };
    for (let stacks = 0n; stacks <= 12n; stacks++) {
      for (const critical of [0, 1]) {
        const expected = JSON.stringify(read(3, stacks, critical));
        for (let backend = 0; backend < 4; backend++) {
          if (JSON.stringify(read(backend, stacks, critical)) !== expected) throw Error(`Mismatch: ${labels[backend]}, ${stacks}, ${critical}`);
        }
        checked++;
      }
    }
    for (const stacks of [-(2n ** 63n), -1n, 13n, 2n ** 63n - 1n]) {
      for (const critical of [0, 1]) {
        const expected = JSON.stringify(read(3, stacks, critical));
        for (const backend of [1, 2]) {
          if (JSON.stringify(read(backend, stacks, critical)) !== expected) throw Error('Invalid state mismatch');
        }
        checked++;
      }
    }
    active = (await import('/target/browser/fury.js?replacement')).fury;
    if (read(2, 0n, 1)[0] !== '3') throw Error('Replacement failed');
    host.load_component();
    if (read(1, 0n, 1)[0] !== '3') throw Error('Nested replacement failed');
    for (const micros of [0n, 2_499_999n, 2_500_000n, 2n ** 64n - 1n]) {
      const state = active.elapse(active.onHit(12n, true), micros);
      if (state.stacks !== (micros < 2_500_000n ? 12n : 0n)) throw Error('Expiry mismatch');
    }
    for (const length of [0, 1, 8, 64]) {
      for (const stacks of [0n, 5n, 12n]) {
        const readBatch = backend => {
          const pointer = host.check_batch(backend, stacks, length, 17);
          return JSON.stringify(Array.from(new BigUint64Array(memory.buffer, pointer, 3), String));
        };
        const expected = readBatch(3);
        for (let backend = 0; backend < 4; backend++) {
          if (readBatch(backend) !== expected) throw Error(`Batch mismatch: ${labels[backend]}, ${length}, ${stacks}`);
        }
        checked++;
      }
    }
    const measure = length => {
      const run = (backend, count, seed) => length === 1
        ? host.benchmark(backend, count, seed)
        : host.benchmark_batch(backend, count, length, seed);
      const batches = labels.map(() => 1000);
      for (let backend = 0; backend < 4; backend++) {
        for (let warmup = 0; warmup < 5; warmup++) run(backend, 1000, warmup);
        for (;;) {
          const before = performance.now();
          run(backend, batches[backend], 17);
          if (performance.now() - before >= 50 || batches[backend] >= 1_024_000) break;
          batches[backend] *= 2;
        }
      }
      const samples = labels.map(() => []);
      for (let round = 0; round < 9; round++) {
        for (let slot = 0; slot < 4; slot++) {
          const backend = (slot + round) % 4;
          const before = performance.now();
          const checksum = run(backend, batches[backend], round * 997);
          samples[backend].push((performance.now() - before) * 1000 / batches[backend] / length);
          if (checksum !== run(3, batches[backend], round * 997)) throw Error('Benchmark checksum mismatch');
        }
      }
      return { hitsPerCall: length, results: labels.map((name, i) => ({ name, msPer1000Hits: [...samples[i]].sort((a, b) => a - b)[4], samples: samples[i], callsPerSample: batches[i] })) };
    };
    return { userAgent: navigator.userAgent, checked, replacement: true,
      setupMs: { rune: runeSetupMs, nestedComponent: wasmiSetupMs, browserComponent: jcoLoadMs },
      measurements: [1, 8, 64].map(measure),
    };
  });
  await Bun.write(`${import.meta.dir}/benchmark-result.json`, JSON.stringify(results, null, 2) + '\n');
  console.log(JSON.stringify(results, null, 2));
} finally {
  await browser?.close();
  server.stop();
}
