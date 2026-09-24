import assert from "node:assert/strict";
import { test } from "node:test";
import type { CodexOfficialQuota, UsageReading } from "../src/api/usage.ts";
import { applyLanguagePreference } from "../src/i18n/current.ts";
import { officialQuotaReadings, formatTrayReading, usagePrimary, usageTone } from "../src/lib/usage-format.ts";

// Formatter labels follow the module-level language; these assertions pin it
// explicitly so the expectations below do not depend on the host locale.
applyLanguagePreference("zh-CN");

const reading: UsageReading = { remaining: null, used: null, total: null, unit: "USD" };

test("零余额与缺失余额明确区分，不从已用和总量虚构余额", () => {
  assert.deepEqual(usagePrimary({ ...reading, remaining: 0 }), { label: "余额", value: 0 });
  assert.deepEqual(usagePrimary({ ...reading, used: 3, total: 10 }), { label: "已用", value: 3 });
  assert.equal(usagePrimary(reading), null);
  assert.equal(formatTrayReading(reading), "暂无读数");
});

test("官方各窗口保留自己的余额和重置时间", () => {
  const quota: CodexOfficialQuota = { status: "available", at: null, stale: false, lastReset: null,
    windows: [{ label: "5 小时", usedPercent: 28, resetsAt: "2026-09-21T02:00:00Z" },
      { label: "7 天", usedPercent: 54, resetsAt: null }] };
  const rows = officialQuotaReadings(quota);
  assert.deepEqual(rows.map((row) => row.remaining), [72, 46]);
  assert.equal(rows[0].resetsAt, quota.windows[0].resetsAt);
  assert.equal(rows[1].resetsAt, undefined);
  assert.equal(formatTrayReading(rows[0]), "剩余 72%");
});

test("失效与耗尽使用明确状态，正常余额不加警告", () => {
  assert.equal(formatTrayReading({ ...reading, remaining: 20, isValid: false }), "已失效");
  assert.equal(usageTone({ ...reading, remaining: 0 }), "danger");
  assert.equal(usageTone({ ...reading, remaining: 20 }), undefined);
});

test("英文语言输出对应的英文标签", () => {
  applyLanguagePreference("en-US");
  assert.deepEqual(usagePrimary({ ...reading, remaining: 0 }), { label: "balance", value: 0 });
  assert.deepEqual(usagePrimary({ ...reading, used: 3, total: 10 }), { label: "used", value: 3 });
  assert.equal(usagePrimary(reading), null);
  assert.equal(formatTrayReading(reading), "No readings");
  const quota: CodexOfficialQuota = { status: "available", at: null, stale: false, lastReset: null,
    windows: [{ label: "5 小时", usedPercent: 28, resetsAt: null }] };
  assert.equal(formatTrayReading(officialQuotaReadings(quota)[0]), "remaining 72%");
  assert.equal(formatTrayReading({ ...reading, remaining: 20, isValid: false }), "Invalid");
  assert.equal(formatTrayReading({ ...reading, remaining: 0 }), "balance 0 USD · depleted");
  applyLanguagePreference("zh-CN");
});
