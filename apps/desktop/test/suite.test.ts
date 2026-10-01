// The suite sections' list and edits (src/lib/suiteModel.ts), run by Node itself:
//
//   node --test apps/desktop/test/

import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  loginOf,
  byId,
  moveOps,
  newId,
  rdpAddress,
  shellQuote,
  siblingsOf,
  sshCommand,
  tunnelSpec,
  workspaces,
  type SuiteRecord,
} from '../src/lib/suiteModel.ts';

let seq = 0;
function rec(id: string, kind: SuiteRecord['kind'], data: Record<string, unknown>): SuiteRecord {
  return { id, kind, seq: ++seq, updatedMs: 0, data, broken: false };
}

const records: SuiteRecord[] = [
  rec('g1', 'group', { workspace: 'private', name: 'Homelab', position: 1 }),
  rec('g2', 'group', { workspace: 'private', name: 'Cloud', position: 0 }),
  rec('g3', 'group', { workspace: 'business', name: 'Office', position: 0, identity_id: 'i2' }),
  rec('h1', 'host', {
    name: 'nas',
    address: 'nas.example.com',
    port: 22,
    workspace: 'private',
    position: 1,
    group_id: 'g1',
    identity_id: 'i1',
  }),
  rec('h2', 'host', {
    name: 'pi',
    address: '192.0.2.5',
    port: 2222,
    workspace: 'private',
    position: 0,
    group_id: 'g1',
    identity_id: null,
  }),
  rec('h3', 'host', {
    name: 'loose',
    address: 'loose.example.org',
    port: 22,
    workspace: 'private',
    position: 0,
    group_id: null,
    identity_id: null,
  }),
  rec('h4', 'host', {
    name: 'desk',
    address: '2001:db8::7',
    port: 3389,
    workspace: 'business',
    position: 0,
    group_id: 'g3',
    identity_id: null,
  }),
  rec('h5', 'host', {
    name: 'lab',
    address: 'lab.example.net',
    port: 22,
    workspace: 'lab',
    position: 0,
    group_id: null,
    identity_id: null,
  }),
  rec('i1', 'identity', { label: 'root', username: 'root', auth_type: 'key' }),
  rec('i2', 'identity', {
    label: 'anna',
    username: 'anna',
    domain: 'EXAMPLE',
    auth_type: 'password',
  }),
];

test('hosts are listed by workspace, then group, in the dragged order', () => {
  const list = workspaces(records);
  assert.deepEqual(
    list.map((w) => w.workspace),
    ['private', 'business', 'lab'],
    'a workspace a newer app made comes last, not lost',
  );
  const priv = list[0]!;
  assert.deepEqual(
    priv.groups.map((g) => g.group?.id ?? null),
    ['g2', 'g1', null],
  );
  assert.deepEqual(
    priv.groups[1]!.hosts.map((h) => h.id),
    ['h2', 'h1'],
  );
  assert.deepEqual(priv.groups[0]!.hosts, [], 'an empty group shows without a search');
});

test('a search keeps only matching hosts and the groups they are in', () => {
  const list = workspaces(records, 'ROOT nas');
  assert.equal(list.length, 1);
  assert.deepEqual(
    list[0]!.groups.map((g) => [g.group?.id, g.hosts.map((h) => h.id)]),
    [['g1', ['h1']]],
  );
});

test('a host logs in with its own login, else its group’s', () => {
  const index = byId(records);
  assert.equal(loginOf(index.get('h1')!, index)?.id, 'i1');
  assert.equal(loginOf(index.get('h4')!, index)?.id, 'i2');
  assert.equal(loginOf(index.get('h2')!, index), null);
});

test('the copied command is what a shell takes', () => {
  assert.equal(
    sshCommand({ address: 'server.example.com', port: 22 }, { username: 'root' }),
    'ssh root@server.example.com',
  );
  assert.equal(sshCommand({ address: '192.0.2.5', port: 2222 }, null), 'ssh -p 2222 192.0.2.5');
  assert.equal(
    sshCommand({ address: 'h.example.com', port: 22 }, { username: "o'brien; rm" }),
    `ssh 'o'\\''brien; rm@h.example.com'`,
  );
  assert.equal(
    sshCommand({ address: '-oProxyCommand=touch x', port: 22 }, null),
    "ssh -- '-oProxyCommand=touch x'",
  );
  assert.equal(shellQuote('a b'), "'a b'");
  assert.equal(rdpAddress({ address: '2001:db8::7', port: 3389 }), '[2001:db8::7]:3389');
  assert.equal(rdpAddress({ address: 'desk.example.com', port: 3390 }), 'desk.example.com:3390');
});

test('moving a host rewrites only the positions that change', () => {
  const index = byId(records);
  const h1 = index.get('h1')!;
  const ops = moveOps(siblingsOf(records, h1), 'h1', -1);
  assert.deepEqual(
    ops.map((o) => (o.op === 'put' ? [o.id, o.patch.position] : null)),
    [
      ['h1', 0],
      ['h2', 1],
    ],
  );
  assert.deepEqual(moveOps(siblingsOf(records, h1), 'h2', -1), [], 'the first stays first');
  const g2 = index.get('g2')!;
  assert.deepEqual(
    siblingsOf(records, g2)
      .map((g) => g.id)
      .sort(),
    ['g1', 'g2'],
    'groups move among the groups of their workspace',
  );
});

test('port forwards read as ssh would take them', () => {
  assert.equal(
    tunnelSpec({
      kind: 'local',
      bind_address: '127.0.0.1',
      bind_port: 8080,
      target_host: 'localhost',
      target_port: 80,
    }),
    '-L 127.0.0.1:8080:localhost:80',
  );
  assert.equal(
    tunnelSpec({
      kind: 'remote',
      bind_address: '',
      bind_port: 9000,
      target_host: 'db.example.com',
      target_port: 5432,
    }),
    '-R 9000:db.example.com:5432',
  );
});

test('new ids are random v4 UUIDs', () => {
  const a = newId();
  assert.match(a, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  assert.notEqual(a, newId());
});
