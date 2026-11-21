// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Manual book images of the partner app (phone size, light theme), with the store-shot sample data.
// Excluded from normal runs (tag `manual_screenshots`); see scripts/manual/shots.sh.
@Tags(['manual_screenshots'])
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:partner/screens/complete.dart';
import 'package:partner/screens/jobs.dart';
import 'package:partner/screens/more.dart';
import 'package:partner/screens/shell.dart';
import 'package:partner/state.dart';
import 'package:test_support/test_support.dart';

import '../store_screenshots/partner_store_shots_test.dart' show storeShotEnv;
import '../support/partner_host.dart';

const _phone = ShotDevice('phone', 780, 1688, 2);
final _out = const String.fromEnvironment(
  'SHOT_OUT',
  defaultValue: 'build/manual_shots',
);
final _only = const String.fromEnvironment('SHOT_ONLY');

/// A partner who signed up and waits for the owner's approval.
Future<TestEnv> _pendingEnv(String lang) {
  final fake = FakeApi()
    ..reply('GET', '/partner/me', verifiedProfile(status: 'pending'));
  stockPartnerApi(fake, jobs: const []);
  return TestEnv.create(
    fake: fake,
    role: 'partner',
    localeCode: lang,
    themeMode: ThemeMode.light,
  );
}

final _shots = <(String, Widget Function(), bool)>[
  ('partner-signup', () => const Gate(), true),
  (
    'partner-jobs',
    () => const Scaffold(body: SafeArea(child: JobsTab())),
    false,
  ),
  ('partner-job-detail', () => const JobDetailScreen(id: 'job-1'), false),
  (
    'partner-complete',
    () => const CompleteScreen(bookingId: 'job-1', isQuote: false),
    false,
  ),
  ('partner-earnings', () => const SettlementsScreen(), false),
];

void main() {
  for (final lang in const ['ko', 'en']) {
    testWidgets('manual shots (partner, $lang)', (tester) async {
      await tester.runAsync(loadShotFonts);
      final shared = await storeShotEnv(lang, ThemeMode.light);
      for (final (id, screen, pending) in _shots) {
        if (_only.isNotEmpty && !_only.split(',').contains(id)) continue;
        final env = pending ? await _pendingEnv(lang) : shared;
        final app = PartnerState(
          env.api,
          FakeRealtime(env.api),
          outbox: testOutbox(),
        );
        await captureShot(
          tester,
          PartnerTestHost(env: env, app: app, home: screen()),
          _phone,
          '$_out/$lang/$id.png',
        );
        await unmount(tester);
      }
    });
  }
}
