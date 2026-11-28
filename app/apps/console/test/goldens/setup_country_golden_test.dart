// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

@Tags(['golden'])
library;

import 'dart:io';

import 'package:console/screens/setup_country.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import '../support/console_host.dart';

/// Compared only on the CI runner, where fonts and rasterisation are pinned (scripts/ci/goldens.sh).
bool _enabled(String baseline) =>
    Platform.environment['CI'] == 'true' &&
    (autoUpdateGoldenFiles || File(baseline).existsSync());

/// The country step with a draft pack previewed, in English and mirrored in Arabic (right to left).
void main() {
  for (final lang in const ['en', 'ar']) {
    final file = 'setup_country_$lang.png';
    testWidgets('country step golden ($lang)', (tester) async {
      setViewport(tester, tallPhoneSize);
      await tester.runAsync(loadShotFonts);
      final fake = FakeApi()
        ..reply('GET', '/admin/setup/packs', {
          'country': null,
          'detected': 'GB',
          'packs': [
            {
              'id': 'GB',
              'name': 'United Kingdom',
              'maturity': {'level': 'draft'},
              'currency': {'code': 'GBP', 'minor_units': 2},
              'zones': ['Europe/London'],
              'locales': ['en'],
              'payments': {
                'recommended': ['stripe'],
              },
              'legal': ['terms', 'privacy'],
            },
          ],
        });
      final env = await TestEnv.create(
        fake: fake,
        role: 'staff_admin',
        themeMode: ThemeMode.light,
      );
      env.locale.locale = Locale(lang);
      await pumpConsoleScreen(
        tester,
        env,
        SingleChildScrollView(child: SetupCountryCard(onSaved: () {})),
        locales: T.supportedLocales,
      );
      await settle(tester);
      await expectLater(find.byType(SetupCountryCard), matchesGoldenFile(file));
    }, skip: !_enabled('test/goldens/$file'));
  }
}
