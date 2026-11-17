// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/screens/first_run.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

Future<void> _submitWith(WidgetTester tester, ApiReply reply) async {
  setViewport(tester, tallPhoneSize);
  final fake = FakeApi()..on('POST', '/setup/admin', (_) => reply);
  final env = await TestEnv.create(fake: fake, role: 'staff_admin');
  await pumpConsoleScreen(tester, env, const FirstRunScreen());
  final fields = find.byType(TextField);
  await tester.enterText(fields.at(0), 'ABCD2345EFGH');
  await tester.enterText(fields.at(1), '사장님');
  await tester.enterText(fields.at(2), 'owner@example.com');
  await tester.enterText(fields.at(3), 'correct-horse-battery');
  await tester.tap(find.text(koT.cFirstRunCreate));
  await settle(tester);
}

void main() {
  testWidgets('an expired setup code says so and asks for a new one', (
    tester,
  ) async {
    await _submitWith(
      tester,
      const ApiReply(410, {
        'error': {'code': 'setup_code_expired'},
      }),
    );
    expect(find.text(koT.cSetupCodeExpired), findsOneWidget);
    expect(find.text(koT.cWrongSetupCode), findsNothing);
  });

  testWidgets('a wrong setup code still says it is not right', (tester) async {
    await _submitWith(
      tester,
      const ApiReply(403, {
        'error': {'code': 'forbidden'},
      }),
    );
    expect(find.text(koT.cWrongSetupCode), findsOneWidget);
    expect(find.text(koT.cSetupCodeExpired), findsNothing);
  });
}
