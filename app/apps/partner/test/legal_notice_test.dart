// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter_test/flutter_test.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:partner/screens/more.dart';
import 'package:test_support/test_support.dart';

import 'support/partner_host.dart';

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

  testWidgets('the licenses row opens the page with the owner copyright line', (
    tester,
  ) async {
    setViewport(tester, phoneSize);
    final fake = FakeApi();
    stockPartnerApi(fake);
    final env = await TestEnv.create(fake: fake, role: 'pro');
    await pumpPartnerScreen(tester, env, const MoreTab());
    await pumpUntilFound(tester, find.text(koT.moreVersion('9.8.7')));
    await tester.ensureVisible(find.text(koT.moreLicenses));
    await tester.pump();
    await tester.tap(find.text(koT.moreLicenses));
    await pumpUntilFound(tester, find.text(koT.legalCopyright));
  });
}
