import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
import ts from 'typescript';

// Run the real hook and its helpers with deterministic state slots and an IPC
// adapter. This lets tests deliver native events before command promises resolve.
function mountApp() {
  const state = [];
  const effects = [];
  const listeners = new Map();
  const modules = new Map();
  let cursor = 0;
  let mounting = true;
  const react = {
    useState(initial) {
      const index = cursor++;
      if (mounting) state[index] = typeof initial === 'function' ? initial() : initial;
      return [state[index], value => {
        state[index] = typeof value === 'function' ? value(state[index]) : value;
      }];
    },
    useRef(initial) {
      const index = cursor++;
      if (mounting) state[index] = { current: initial };
      return state[index];
    },
    useMemo: compute => compute(),
    useCallback: callback => callback,
    useEffect: effect => { if (mounting) effects.push(effect); },
  };
  const overrides = {
    isDesktopRuntime: false,
    previewPlatform: () => 'macos',
    detectShell: () => 'desktop',
    decideFileOffer: async () => {},
    startFileTransfer: async () => 'A',
  };
  const bridge = new Proxy(overrides, {
    get(target, key) {
      if (key in target) return target[key];
      if (key.startsWith('on')) return callback => {
        listeners.set(key, callback);
        return key === 'onShellChange' ? () => {} : Promise.resolve(() => {});
      };
      if (key.startsWith('get')) return () => new Promise(() => {});
      throw new Error(`Unexpected bridge call: ${key}`);
    },
  });
  const window = {
    setTimeout: () => 1,
    clearTimeout: () => {},
    localStorage: { getItem: () => null, setItem: () => {} },
  };
  function load(filename) {
    if (modules.has(filename)) return modules.get(filename);
    const exports = {};
    modules.set(filename, exports);
    const code = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
    }).outputText;
    vm.runInNewContext(code, {
      exports, window,
      require(name) {
        if (name === 'react') return react;
        if (name === '../bridge') return bridge;
        return load(path.resolve(path.dirname(filename), `${name}.ts`));
      },
    }, { filename });
    return exports;
  }
  const { useNeloa } = load(path.resolve('src/lib/useNeloa.ts'));
  function render() { cursor = 0; return useNeloa(); }
  render();
  mounting = false;
  effects.forEach(effect => effect());
  return {
    render, bridge: overrides,
    emit(name, payload) { assert.ok(listeners.has(name)); listeners.get(name)(payload); },
  };
}

const offer = id => ({ transferId: id, name: `${id}.txt`, peerId: 'peer', peerName: 'Peer', size: 1 });
const result = (status = 'completed', direction = 'received') => ({
  ...offer('A'), status, direction, message: status, atMs: 1,
});

for (const accepted of [false, true]) {
  test(`terminal event before ${accepted ? 'accept' : 'reject'} response preserves the next offer`, async () => {
    const app = mountApp();
    app.emit('onFileOffer', offer('A'));
    app.emit('onFileOffer', offer('B'));
    app.bridge.decideFileOffer = async () => {
      app.emit('onFileTransferResult', result(accepted ? 'completed' : 'rejected'));
    };
    await app.render().submitFileOffer(accepted);
    const current = app.render();
    assert.deepEqual(Array.from(current.fileOffers, item => item.transferId), ['B']);
    assert.equal(current.transfers.length, 0);
    assert.equal(current.history.length, 1);
  });
}

test('accept response does not regress progress received during the command', async () => {
  const app = mountApp();
  app.emit('onFileOffer', offer('A'));
  app.bridge.decideFileOffer = async () => {
    app.emit('onFileTransferProgress', {
      ...offer('A'), direction: 'received', stage: 'transferring', transferred: 1, bytesPerSecond: 1,
    });
  };
  await app.render().submitFileOffer(true);
  const current = app.render();
  assert.equal(current.fileOffers.length, 0);
  assert.equal(current.transfers[0].stage, 'transferring');
  assert.equal(current.transfers[0].transferred, 1);
});

test('send response does not resurrect an already failed transfer', async () => {
  const app = mountApp();
  app.emit('onPeersChanged', { active: true, peers: [{ id: 'peer', name: 'Peer' }] });
  app.bridge.startFileTransfer = async () => {
    app.emit('onFileTransferResult', result('failed', 'sent'));
    return 'A';
  };
  app.render().retryTransfer({ ...offer('A'), path: '/example.txt' });
  await new Promise(setImmediate);
  const current = app.render();
  assert.equal(current.transfers.length, 0);
  assert.equal(current.history[0].status, 'failed');
});
