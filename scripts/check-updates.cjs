const { createRequire } = require('node:module');
const { readFileSync } = require('node:fs');
const assert = require('node:assert/strict');
const req = createRequire(process.cwd() + '/package.json');
const vue = req('vue'), ts = req('typescript'), sfc = req('vue/compiler-sfc');
const renderer = vue.createRenderer({
  createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  insert() {}, remove() {}, parentNode: () => null, nextSibling: () => null,
  setText() {}, setElementText() {}, patchProp() {},
});

(async () => {
  const calls = [], warnings = [], confirmations = [];
  let fail = false;
  const pkg = { manager: 'apt', name: 'chatgpt', arch: 'amd64', current_version: '26.1027.51404', new_version: '26.1103.31037', repository: 'stable' };
  const gui = {
    id: 'codex-gui:fixture', name: 'ChatGPT GUI（含 Codex）', provider: 'codex-gui', kind: 'gui',
    current_version: pkg.current_version, latest_version: pkg.new_version, architecture: 'amd64',
    status: 'available', native_update: { ...pkg, repository: 'https://persistent.oaistatic.com/codex-app-prod/linux/deb/dists/stable/main/binary-amd64/Packages' },
    release_channel: 'stable', install_path: '/usr/lib/chatgpt/codex-launcher', source_url: '', update_hint: '', note: '',
  };
  const cli = { ...gui, id: 'npm:fixture', name: '@openai/codex', kind: 'cli', provider: 'npm', native_update: null, current_version: '0.166.0', latest_version: '0.167.0' };
  const report = () => ({
    packages: [pkg], tools: [gui, cli], warnings: [], channels: [{ provider: 'apt', state: 'completed' }],
    checked_at: new Date().toISOString(), expires_at: new Date(Date.now() + 600000).toISOString(),
    executable_tools: [cli.id], automatic_managers: ['apt'],
  });
  const mods = {
    vue,
    '@tauri-apps/api/core': { isTauri: () => true },
    '@tauri-apps/api/event': { listen: async () => () => {} },
    '@tauri-apps/plugin-opener': { openUrl: async () => {} },
    'element-plus': {
      ElMessage: { warning: text => warnings.push(text), error() {}, success() {} },
      ElMessageBox: { confirm: async text => confirmations.push(text), close() {} },
    },
    '../tasks': {
      activeTask: () => null,
      runTask: async (command, args) => {
        calls.push({ command, args });
        if (command === 'check_software_updates') return structuredClone(report());
        assert.ok(['upgrade_software', 'upgrade_tool'].includes(command));
        if (fail) throw new Error('fixture authorization or installation failure');
        return 'fixture upgrade verified';
      },
    },
  };
  const source = sfc.compileScript(sfc.parse(readFileSync('src/views/UpdatesView.vue', 'utf8')).descriptor, { id: 'updates-test' }).content;
  const exports = {};
  new Function('require', 'exports', ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } }).outputText)(
    id => mods[id] || new Proxy({ default: {} }, { get: (object, key) => key in object ? object[key] : () => {} }), exports,
  );
  const component = exports.default;
  component.render = () => null;
  const app = renderer.createApp(component);
  app.mount({});
  const state = app._instance.setupState;
  const findGui = () => state.tools.find(tool => tool.kind === 'gui');
  const upgrades = () => calls.filter(call => call.command.startsWith('upgrade_'));

  await state.checkUpdates();
  assert.equal(state.canUpgradeTool(findGui()), true, 'a matching native candidate enables the GUI row');
  assert.equal(state.availableCount, 2, 'GUI and native rows count as one installation, CLI remains separate');
  state.selectedPackages = state.updates;
  state.selectedTools = [findGui()];
  assert.equal(state.selectedCount, 1);
  await state.runQueue([state.packageQueue(state.updates[0]), state.toolQueue(findGui())]);
  assert.equal(upgrades().length, 1, 'selecting both views must install only once');
  assert.deepEqual(upgrades()[0], { command: 'upgrade_software', args: { manager: 'apt', name: pkg.name, expectedVersion: pkg.new_version, arch: pkg.arch } });
  assert.match(confirmations[0], /ChatGPT GUI/);
  assert.equal(findGui().status, 'current');
  assert.equal(findGui().current_version, pkg.new_version);
  assert.equal(findGui().native_update, null);
  assert.equal(state.updates.length, 0);

  await state.checkUpdates();
  await state.runQueue([state.toolQueue(state.tools.find(tool => tool.kind === 'cli'))]);
  assert.deepEqual(upgrades().at(-1), { command: 'upgrade_tool', args: { id: cli.id } });
  assert.equal(findGui().status, 'available', 'CLI upgrades cannot change the GUI row');

  await state.checkUpdates();
  const before = upgrades().length;
  state.updates = [];
  assert.equal(state.canUpgradeTool(findGui()), false);
  await state.runQueue([state.toolQueue(findGui())]);
  assert.equal(upgrades().length, before, 'missing native candidates never fall back to upgrade_tool');
  assert.match(warnings.at(-1), /刷新软件源/);
  for (const patch of [{ arch: 'arm64' }, { new_version: '26.1104.1' }, { current_version: '26.1001.1' }]) {
    state.updates = [{ ...pkg, ...patch }];
    assert.equal(state.canUpgradeTool(findGui()), false, 'versions and architecture must match the detected GUI');
  }
  state.updates = [structuredClone(pkg), { ...pkg, repository: 'another-source' }];
  assert.equal(state.canUpgradeTool(findGui()), false, 'ambiguous candidates require another check');

  await state.checkUpdates();
  fail = true;
  await state.runQueue([state.toolQueue(findGui())]);
  assert.equal(state.failedQueue.length, 1);
  assert.equal(findGui().status, 'available', 'failed installation preserves the candidate and old version');
  assert.equal(state.updates.length, 1);
  fail = false;
  await state.runQueue(state.failedQueue);
  assert.equal(findGui().status, 'current', 'retry succeeds through the same native route');

  await state.checkUpdates();
  const unexpiredCalls = upgrades().length;
  state.expiresAt = new Date(Date.now() - 1).toISOString();
  await state.runQueue([state.packageQueue(state.updates[0]), state.toolQueue(findGui())]);
  assert.equal(upgrades().length, unexpiredCalls, 'deduplication must preserve GUI expiration checks');
  assert.match(warnings.at(-1), /10 分钟/);

  await state.checkUpdates();
  await state.runQueue([state.packageQueue(state.updates[0])]);
  assert.equal(findGui().status, 'current', 'updating the system row also updates its GUI view');
  app.unmount();
  console.log('PASS GUI/native routing, exact candidate identity, deduplication, CLI isolation, failure retry, stale reports and both-view synchronization');
})().catch(error => { console.error(error); process.exitCode = 1; });
