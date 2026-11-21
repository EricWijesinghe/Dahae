// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Manual book images, chapters 26 to 29: system status, alerts, switches, server update, problems.
// Excluded from normal runs (tag `manual_screenshots`); see scripts/manual/shots.sh.
@Tags(['manual_screenshots'])
library;

import 'package:console/screens/ops.dart';
import 'package:core/core.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'manual_fixtures.dart';
import 'manual_support.dart';

String _ago(Duration d) => DateTime.now().subtract(d).toUtc().toIso8601String();
double _unix(Duration ago) =>
    DateTime.now().subtract(ago).millisecondsSinceEpoch / 1000;

/// GET /admin/ops/status. [red] turns the disk check and the text-message service red.
Json _status({bool red = false}) => {
  'level': red ? 'red' : 'green',
  'version': '1.5.1',
  'uptime_secs': 3 * 86400 + 4 * 3600,
  'dev_mode': false,
  'db': {
    'ok': true,
    'latency_ms': 3,
    'size_bytes': 52428800,
    'sessions': 3,
    'pool_open': 2,
    'pool_idle': 1,
  },
  'disk': {
    'total_bytes': 100,
    'free_bytes': red ? 6 : 64,
    'free_pct': red ? 6.0 : 64.0,
  },
  'checks': [
    {'key': 'db', 'level': 'green', 'value': 3},
    {'key': 'disk', 'level': red ? 'red' : 'green', 'value': red ? 6 : 64},
    {'key': 'backup', 'level': 'green', 'value': 5},
    {'key': 'errors', 'level': 'green', 'value': 0},
    {'key': 'payments', 'level': 'green', 'value': 0},
    {'key': 'webhook', 'level': 'green', 'value': 0},
  ],
  'integrations': [
    {
      'code': 'solapi',
      'configured': true,
      'level': red ? 'red' : 'green',
      'last_ok_at': _ago(const Duration(hours: 1)),
      'last_fail_at': red ? _ago(const Duration(minutes: 10)) : null,
      'last_error': red ? 'provider_error' : null,
    },
    {
      'code': 'tosspayments',
      'configured': true,
      'level': 'green',
      'last_ok_at': _ago(const Duration(minutes: 25)),
      'last_fail_at': null,
      'last_error': null,
    },
    {
      'code': 'push',
      'configured': true,
      'level': 'green',
      'last_ok_at': _ago(const Duration(minutes: 3)),
      'last_fail_at': null,
      'last_error': null,
    },
    {
      'code': 'backup',
      'configured': false,
      'level': 'yellow',
      'last_ok_at': null,
      'last_fail_at': null,
      'last_error': null,
    },
  ],
  'jobs': [
    {
      'job': 'reconciler',
      'last_run_unix': _unix(const Duration(minutes: 1)),
      'last_ok_unix': _unix(const Duration(minutes: 1)),
      'errors': 0,
      'stalled': false,
    },
    {
      'job': 'notify_outbox',
      'last_run_unix': _unix(const Duration(seconds: 20)),
      'last_ok_unix': _unix(const Duration(seconds: 20)),
      'errors': 0,
      'stalled': false,
    },
  ],
};

const _alerts = <String, Object?>{
  'prefs': {
    'rules': {'sms_failing': true, 'disk_low': true, 'backup_stale': true},
    'quiet_start': 22,
    'quiet_end': 7,
    'alert_phone': '01012345678',
  },
  'rules': ['sms_failing', 'disk_low', 'backup_stale', 'payment_failing'],
  'default_phone': '01012345678',
  'cooldown_hours': 6,
};

Json _config({bool maintenance = false}) => {
  'flags': {'booking': true, 'chat': true, 'ai': true, 'payments': true},
  'maintenance': {
    'on': maintenance,
    'message': maintenance ? '서버 점검 중이에요. 오후 3시에 다시 열어요.' : '',
  },
};

Json _jobs({bool running = false}) => {
  'jobs': [
    if (running)
      {
        'id': 'job-2',
        'kind': 'update',
        'state': 'running',
        'step': '백업 중',
        'created_at': _ago(const Duration(minutes: 1)),
      },
    {
      'id': 'job-1',
      'kind': 'update',
      'state': 'ok',
      'step': '완료',
      'created_at': _ago(const Duration(days: 6)),
    },
  ],
};

void _opsApi(
  FakeApi f, {
  bool red = false,
  bool running = false,
  bool maintenance = false,
}) {
  f
    ..reply('GET', '/admin/ops/status', _status(red: red))
    ..reply('GET', '/admin/ops/alerts', _alerts)
    ..reply('GET', '/admin/config', _config(maintenance: maintenance))
    ..reply('GET', '/admin/client-errors', {'items': <Object?>[]})
    ..reply('GET', '/admin/me/account', {'is_owner': true})
    ..reply('GET', '/admin/host/jobs', _jobs(running: running));
}

ManualShot _ops(
  String id, {
  String Function(T t)? at,
  bool red = false,
  bool running = false,
  bool maintenance = false,
}) => ManualShot(
  id,
  build: consoleApp,
  api: (f) => _opsApi(f, red: red, running: running, maintenance: maintenance),
  act: (tester, t) async {
    await openSection(tester, t.cOps);
    if (at != null) {
      await scrollTo(tester, find.text(at(t)), within: OpsScreen);
    }
  },
);

final _shots = <ManualShot>[
  // 26 System status
  _ops('console-ops-overview'),
  _ops('console-ops-integrations', at: (t) => t.oIntegrations),
  _ops('console-ops-alerts', at: (t) => t.oAlerts),
  _ops('console-ops-flags', at: (t) => t.oFlags, maintenance: true),
  _ops('console-ops-update', at: (t) => t.hjTitle),
  _ops('console-ops-update-running', at: (t) => t.hjTitle, running: true),
  // 27 Backups
  _ops('console-ops-backup-check', at: (t) => t.oChecks),
  // 29 When something is red
  _ops('console-ops-red', red: true),
];

void main() => runManualShots('ops', _shots, addBase);
