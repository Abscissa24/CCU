// ── End-to-end: ccu-keygen → activation service → app token ────────────
//
// This is the contract test the unit tests cannot give us: it runs the real
// Rust keygen, registers its real batch with the activation service, then
// activates the code exactly as the app does and verifies the returned
// Ed25519-signed licence with the keygen's public key.
//
//   1. cargo build --release --manifest-path tools/keygen/Cargo.toml
//   2. node tools/keygen/scripts/e2e.mjs
//
// By default it starts activation/server.mjs. Set CCU_E2E_SERVER=deno to run
// the same contract against activation/deno/server.ts (requires Deno):
//
//   CCU_E2E_SERVER=deno node tools/keygen/scripts/e2e.mjs
//
// Everything runs in an isolated temp directory with a throwaway key and
// data file; no production key or server is touched.

import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createServer } from 'node:net';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ed25519 } from '@noble/curves/ed25519.js';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils.js';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const BIN = process.env.CCU_KEYGEN_BIN || join(ROOT, 'tools/keygen/target/release/ccu-keygen');
const ORG = 'Port Shepstone High School';
const ADMIN = 'e2e-admin-token';
const DAY = 86_400_000;

if (process.platform !== 'linux') {
  console.error('The keygen is Linux-only; run this on Linux.');
  process.exit(1);
}

let passed = 0;
let failed = 0;
function check(label, ok, detail) {
  if (ok) { passed++; console.log('  PASS ' + label); }
  else { failed++; console.log('  FAIL ' + label + (detail == null ? '' : ' — ' + detail)); }
}

const freePort = () => new Promise((resolve) => {
  const s = createServer();
  s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => resolve(p)); });
});

function schoolCode(name) {
  const B36 = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ';
  const digest = createHash('sha256').update(String(name).trim().replace(/\s+/g, ' ').toLowerCase()).digest();
  let v = 0n;
  for (let i = 0; i < 6; i++) v = (v << 8n) | BigInt(digest[i]);
  v %= 36n ** 6n;
  let out = '';
  while (v > 0n) { out = B36[Number(v % 36n)] + out; v /= 36n; }
  return out.padStart(6, '0');
}

const tmp = mkdtempSync(join(tmpdir(), 'ccu-keygen-e2e-'));
const keyHome = join(tmp, 'keygen');
const dataFile = join(tmp, 'data.json');
const kvFile = join(tmp, 'deno-kv');
const keyFile = join(tmp, 'activation.key');
const batchFile = join(tmp, 'batch.json');

const env = { ...process.env, CCU_KEYGEN_HOME: keyHome };
function keygen(...args) {
  const r = spawnSync(BIN, args, { cwd: ROOT, env, encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`ccu-keygen ${args.join(' ')} failed:\n${r.stderr || r.stdout}`);
  return r.stdout;
}
function keygenJson(...args) {
  const text = keygen(...args).trim();
  const start = text.indexOf('{');
  if (start < 0) throw new Error('no JSON in keygen output: ' + text);
  return JSON.parse(text.slice(start));
}

let server = null;
const cleanup = () => {
  if (server) { try { server.kill(); } catch {} }
  try { rmSync(tmp, { recursive: true, force: true }); } catch {}
};
process.on('exit', cleanup);

try {
  console.log('Keyring');
  const init = keygenJson('init', '--json');
  const publicKey = init.public_keys['activation-2026'];
  check('init creates a 32-byte Ed25519 public key', /^[0-9a-f]{64}$/.test(publicKey), publicKey);
  check('init reports the key file', String(init.key_file).includes('keys.json'));
  keygen('export-activation-key', '--out', keyFile);
  const privateKey = readFileSync(keyFile, 'utf8').trim();
  check('exported private key derives the public key', bytesToHex(ed25519.getPublicKey(hexToBytes(privateKey))) === publicKey);

  const PORT = await freePort();
  const URL_ = `http://127.0.0.1:${PORT}`;
  const useDeno = process.env.CCU_E2E_SERVER === 'deno';
  let serverErr = '';
  server = useDeno
    ? spawn('deno', [
      'run',
      '--allow-net',
      '--allow-env',
      '--allow-read',
      '--allow-write',
      '--unstable-kv',
      'activation/deno/server.ts'
    ], {
      cwd: ROOT,
      env: {
        ...process.env,
        PORT: String(PORT),
        HOST: '127.0.0.1',
        CCU_ACTIVATION_PRIVATE_KEY: privateKey,
        CCU_ADMIN_TOKEN: ADMIN,
        CCU_ACTIVATION_KV: kvFile
      },
      stdio: ['ignore', 'pipe', 'pipe']
    })
    : spawn(process.execPath, ['activation/server.mjs'], {
      cwd: ROOT,
      env: {
        ...process.env,
        PORT: String(PORT),
        HOST: '127.0.0.1',
        CCU_ACTIVATION_PRIVATE_KEY: privateKey,
        CCU_ADMIN_TOKEN: ADMIN,
        CCU_ACTIVATION_DATA: dataFile
      },
      stdio: ['ignore', 'pipe', 'pipe']
    });
  server.stderr.on('data', d => { serverErr += d; });
  await new Promise((resolve, reject) => {
    server.stdout.on('data', d => { if (String(d).includes('listening')) resolve(); });
    server.on('exit', c => reject(new Error('activation server exited early: ' + serverErr)));
    setTimeout(() => reject(new Error('activation server did not start')), useDeno ? 20000 : 8000);
  });

  console.log('Generate + publish');
  const gen = keygenJson(
    'gen-codes', '--school', ORG, '--days', '30', '--count', '2', '--seats', '1',
    '--order', 'E2E-1', '--out', batchFile, '--publish',
    '--server', URL_, '--admin-token', ADMIN, '--json'
  );
  const codes = gen.codes || [];
  const batch = gen.batch || {};
  check('two formatted activation codes generated', codes.length === 2 && codes.every(c => /^CCU5[A-Z0-9]-[A-Z0-9]{5}-[A-Z0-9]{5}-[A-Z0-9]{5}$/.test(c)), codes.join(','));
  check('publish imported both codes', gen.published && gen.published.imported === 2, JSON.stringify(gen.published));
  check('batch JSON was written', JSON.parse(readFileSync(batchFile, 'utf8')).codes.length === 2);
  check('batch uses the service wire format', batch.version === 2 && batch.codes.every(c =>
    c.org === ORG && c.kind === 'school' && c.seats === 1 && c.orderId === 'E2E-1' &&
    /^lic_[0-9a-f]{20}$/.test(c.licenseId) && /^[0-9a-f]{64}$/.test(c.codeHash)
  ));
  check('orgId matches the server/app school code', batch.codes.every(c => c.orgId === schoolCode(ORG)));
  const first = batch.codes[0];
  const normalized = first.code.replace(/[^A-Z0-9]/g, '');
  check('codeHash is SHA-256 of the normalised code', first.codeHash === createHash('sha256').update(normalized).digest('hex'));
  check('expiry is ~30 days out', Math.abs(first.expiresAt - (Date.now() + 30 * DAY)) < 120_000, String(first.expiresAt));

  console.log('Activate like the app');
  const activate = async (code, deviceId, org = ORG, kind = 'school') => {
    const r = await fetch(URL_ + '/activate', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code, deviceId, school: org, kind })
    });
    return { status: r.status, body: await r.json() };
  };

  const devA = 'e2e-device-aaaaaaaa';
  const devB = 'e2e-device-bbbbbbbb';
  const actA = await activate(first.code, devA);
  check('the app can activate the generated code', actA.status === 200 && actA.body.ok === true, JSON.stringify(actA.body));
  const token = actA.body.token || '';
  const [prefix, payloadB64, sigB64] = token.split('.');
  const payloadBytes = Buffer.from(payloadB64 || '', 'base64url');
  const payload = JSON.parse(payloadBytes.toString());
  check('token is CCU2 and verifies with the keygen public key',
    prefix === 'CCU2' && ed25519.verify(Buffer.from(sigB64 || '', 'base64url'), payloadBytes, hexToBytes(publicKey)));
  check('signed payload carries the real org, device, expiry and seat',
    payload.org === ORG && payload.orgId === first.orgId && payload.deviceId === devA &&
    payload.expiresAt === first.expiresAt && payload.seats === 1 && payload.seat === 1 && payload.kind === 'school',
    JSON.stringify(payload));

  const again = await activate(first.code, devA);
  check('re-activating on the same device is free', again.status === 200 && again.body.seat === 1);
  const second = await activate(first.code, devB);
  check('a second device is refused on a one-seat code', second.status === 409 && second.body.error === 'no_seats', JSON.stringify(second.body));

  console.log('Release + revoke');
  const noAuth = await fetch(URL_ + '/release', {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ code: first.code, deviceId: devA })
  });
  check('release without the admin token is refused', noAuth.status === 401);
  const released = await fetch(URL_ + '/release', {
    method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Admin-Token': ADMIN },
    body: JSON.stringify({ code: first.code, deviceId: devA })
  });
  const afterRelease = await activate(first.code, devB);
  check('after release the replacement device activates', released.ok && afterRelease.status === 200, JSON.stringify(afterRelease.body));

  const secondCode = batch.codes[1].code;
  const revoked = await fetch(URL_ + '/admin/revoke', {
    method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Admin-Token': ADMIN },
    body: JSON.stringify({ code: secondCode })
  });
  const revokedAct = await activate(secondCode, devA);
  check('a revoked code cannot be activated', revoked.ok && revokedAct.status === 410 && /revoked/i.test(revokedAct.body.message), JSON.stringify(revokedAct.body));

  console.log('CLI diagnostics');
  const status = keygenJson('status', '--server', URL_, '--json');
  check('status sees the service and a matching keyring', status.ok === true && status.key_match === true && status.codes >= 2, JSON.stringify(status));
  const list = keygen('list', '--schools', 'public/database/index.json');
  check('list prints the school registry', list.includes(ORG));
} catch (err) {
  failed++;
  console.log('  FAIL unexpected error — ' + (err && err.stack || err));
} finally {
  cleanup();
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
