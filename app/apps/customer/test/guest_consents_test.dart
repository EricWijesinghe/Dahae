// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:customer/state.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/customer_host.dart';
import 'support/customer_routes.dart';

Analytics _analytics() =>
    Analytics(send: (_) async {}, platform: 'web', anonId: 'anon-1');

Json _consents({required bool marketing}) => {
  'version': '2026-01',
  'reconsent_required': false,
  'items': [
    {'kind': 'marketing', 'granted': marketing},
  ],
};

void main() {
  test(
    'a guest launch never asks for consents and keeps analytics off',
    () async {
      final env = await TestEnv.create(signedIn: false);
      final analytics = _analytics()..setConsent(true);
      final app = AppState(
        env.api,
        FakeRealtime(env.api),
        analytics: analytics,
      );

      await app.syncAnalyticsConsent();

      expect(env.fake.callsTo('GET', '/me/consents'), isEmpty);
      expect(analytics.track('app_open'), isFalse);
    },
  );

  test('a signed-in launch reads the marketing consent', () async {
    final fake = FakeApi()
      ..reply('GET', '/me/consents', _consents(marketing: true));
    final env = await TestEnv.create(fake: fake, signedIn: true);
    final analytics = _analytics();
    final app = AppState(env.api, FakeRealtime(env.api), analytics: analytics);

    await app.syncAnalyticsConsent();

    expect(fake.callsTo('GET', '/me/consents'), hasLength(1));
    expect(analytics.track('app_open'), isTrue);
  });

  testWidgets('browsing home as a guest makes no /me/consents call', (
    tester,
  ) async {
    setViewport(tester, phoneSize);
    final fake = FakeApi();
    stockCustomerApi(fake);
    final env = await TestEnv.create(fake: fake, signedIn: false);

    await pumpCustomerApp(tester, env);
    await pumpUntilFound(tester, find.text(koT.tabHome));
    await settle(tester, duration: const Duration(milliseconds: 600));

    expect(fake.callsTo('GET', '/me/consents'), isEmpty);
    await unmount(tester);
  });
}
