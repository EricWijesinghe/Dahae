// Copyright (c) 2026 Eric Wijesinghe <EricWijesinghe@hotmail.com>. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart' show Json;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:partner/screens/more.dart';
import 'package:test_support/test_support.dart';

import 'support/partner_host.dart';

Json _legal(String slug, String text) => {
  ...contentJson(kind: 'legal', title: {'ko': slug}),
  'slug': slug,
  'body': {'text': text},
};

void main() {
  setUp(
    () => PackageInfo.setMockInitialValues(
      appName: 'Partner',
      packageName: 'kr.example.partner',
      version: '9.8.7',
      buildNumber: '1',
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
    stockPartnerApi(fake);
    final env = await TestEnv.create(fake: fake, role: 'pro');
    await pumpPartnerScreen(tester, env, const MoreTab());
    await pumpUntilFound(tester, find.text(koT.moreVersion('9.8.7')));

    await tester.dragUntilVisible(
      find.text(koT.morePrivacy),
      find.byType(Scrollable).first,
      const Offset(0, -200),
    );
    await tester.tap(find.text(koT.morePrivacy));
    await pumpUntilFound(tester, find.text('PRIVACY-TEXT'));
    await tester.tap(find.byType(BackButton));
    await settle(tester);

    await tester.dragUntilVisible(
      find.text(koT.moreTerms),
      find.byType(Scrollable).first,
      const Offset(0, -200),
    );
    await tester.tap(find.text(koT.moreTerms));
    await pumpUntilFound(tester, find.text('TERMS-TEXT'));
  });
}
