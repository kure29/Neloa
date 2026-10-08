import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import ts from 'typescript';

function loadHistoryGroups() {
  const filename = path.resolve('src/lib/historyGroups.ts');
  const code = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
  }).outputText;
  const module = { exports: {} };
  // Same realm as the assertions, so deepEqual sees ordinary arrays.
  new Function('module', 'exports', code)(module, module.exports);
  return module.exports;
}

const { groupHistoryByDay } = loadHistoryGroups();
const at = (y, m, d, h = 12) => new Date(y, m - 1, d, h).getTime();
const record = (id, atMs, direction = 'sent') => ({
  id, atMs, direction, name: id, detail: '', peer: 'Mac',
});

test('groups newest-first records into labelled local days', () => {
  const now = new Date(2026, 9, 8, 9);
  const groups = groupHistoryByDay([
    record('a', at(2026, 10, 8, 8)),
    record('b', at(2026, 10, 8, 1)),
    record('c', at(2026, 10, 7, 23)),
    record('d', at(2026, 9, 30)),
    record('e', at(2025, 12, 31)),
  ], 'all', now);

  assert.deepEqual(groups.map(group => group.label), ['今天', '昨天', '9月30日', '2025年12月31日']);
  assert.deepEqual(groups[0].records.map(item => item.id), ['a', 'b']);
});

test('filters by direction before grouping and drops empty days', () => {
  const now = new Date(2026, 9, 8, 9);
  const groups = groupHistoryByDay([
    record('a', at(2026, 10, 8), 'sent'),
    record('b', at(2026, 10, 7), 'received'),
  ], 'received', now);

  assert.deepEqual(groups.map(group => group.label), ['昨天']);
  assert.deepEqual(groups[0].records.map(item => item.id), ['b']);
});

test('yesterday is a calendar day across a month boundary', () => {
  const now = new Date(2026, 10, 1, 0, 30);
  const groups = groupHistoryByDay([record('a', at(2026, 10, 31, 23))], 'all', now);
  assert.equal(groups[0].label, '昨天');
});

test('formats a record moment against today', () => {
  const { formatRecordMoment } = loadHistoryGroups();
  const now = new Date(2026, 9, 8, 9);
  assert.equal(formatRecordMoment(new Date(2026, 9, 8, 7, 5).getTime(), now), '今天 07:05');
  assert.equal(formatRecordMoment(new Date(2026, 8, 30, 18, 40).getTime(), now), '9月30日 18:40');
});
