// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart' show Json;
import 'package:customer/screens/more.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:test_support/test_support.dart';

import 'support/customer_host.dart';
import 'support/customer_routes.dart';

Json _legal(String slug, String text) => {
  ...contentJson(kind: 'legal', title: {'ko': slug}),
  'slug': slug,
  'body': {'text': text},
};

void main() {
  setUp(
    () => PackageInfo.setMockInitialValues(
      appName: 'customer',
      packageName: 'kr.co.dahae.customer',
      version: '1.4.2',
      buildNumber: '7',
      buildSignature: '',
    ),
  );

  testWidgets('the privacy and terms rows open their published text', (
    tester,
  ) async {
    setViewport(tester, const Size(390, 2400));
    final fake = FakeApi()
      ..reply('GET', '/content', [
        _legal('privacy', 'PRIVACY-TEXT'),
        _legal('terms', 'TERMS-TEXT'),
      ]);
    stockCustomerApi(fake);
    final env = await TestEnv.create(fake: fake);
    final more = await pumpCustomerScreen(tester, env, const MoreTab());
    await more.loadAll();
    await settle(tester);

    await tester.scrollUntilVisible(
      find.text(koT.morePrivacy),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text(koT.morePrivacy));
    await pumpUntilFound(tester, find.text('PRIVACY-TEXT'));
    await tester.tap(find.byType(BackButton));
    await settle(tester);

    await tester.scrollUntilVisible(
      find.text(koT.moreTerms),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text(koT.moreTerms));
    await pumpUntilFound(tester, find.text('TERMS-TEXT'));
    await unmount(tester);
  });
}
