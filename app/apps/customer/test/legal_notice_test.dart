// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:customer/screens/more.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:test_support/test_support.dart';

import 'support/customer_host.dart';
import 'support/customer_routes.dart';

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

  testWidgets('the licenses row opens the page with the owner copyright line', (
    tester,
  ) async {
    setViewport(tester, const Size(390, 2400));
    final fake = FakeApi();
    stockCustomerApi(fake);
    final env = await TestEnv.create(fake: fake);
    final more = await pumpCustomerScreen(tester, env, const MoreTab());
    await more.loadAll();
    await settle(tester);
    await tester.scrollUntilVisible(
      find.text(koT.moreLicenses),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text(koT.moreLicenses));
    await pumpUntilFound(tester, find.text(koT.legalCopyright));
    await unmount(tester);
  });
}
