// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:customer/screens/more.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/customer_host.dart';

/// 계정정보 (C-07): name and email edit through the dialog; the phone shows as 010-1234-5678.
void main() {
  testWidgets(
    'account name and email are editable and the phone is formatted',
    (tester) async {
      setViewport(tester, phoneSize);
      var name = '지민';
      var email = 'jimin@example.com';
      final fake = FakeApi()
        ..onGet(
          '/me',
          (_) => {
            'name': name,
            'phone': '01012345678',
            'email': email,
            'logins': ['kakao'],
            'created_at': '2026-11-09T03:00:00Z',
          },
        )
        ..onPatch('/me', (call) {
          final body = call.body as Map;
          if (body.containsKey('name')) name = body['name'] as String;
          if (body.containsKey('email')) email = body['email'] as String;
          return null;
        });
      final env = await TestEnv.create(fake: fake);
      final app = await pumpCustomerScreen(tester, env, const AccountScreen());
      await app.loadProfile();
      await settle(tester);

      expect(find.text('010-1234-5678'), findsOneWidget);

      await tester.tap(find.text(koT.edit).first);
      await settle(tester);
      await tester.enterText(find.byType(TextField), '민지');
      await tester.tap(find.text(koT.save));
      await settle(tester);
      expect(
        find.text('민지'),
        findsNWidgets(2),
        reason: 'the profile header and the name row both show it',
      );
      expect(fake.lastCall('PATCH', '/me')?.body, {'name': '민지'});

      await tester.tap(find.text(koT.edit).last);
      await settle(tester);
      await tester.enterText(find.byType(TextField), 'min@example.com');
      await tester.tap(find.text(koT.save));
      await settle(tester);
      expect(find.text('min@example.com'), findsOneWidget);
      expect(fake.lastCall('PATCH', '/me')?.body, {'email': 'min@example.com'});
      await unmount(tester);
    },
  );
}
