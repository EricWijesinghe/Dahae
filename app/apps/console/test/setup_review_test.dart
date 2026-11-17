import 'package:console/screens/setup_review.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

const _path = '/admin/setup/review';

Future<FakeApi> _open(WidgetTester tester, {Object? reply}) async {
  setViewport(tester, tallPhoneSize);
  final fake = FakeApi()
    ..on('PUT', _path, (_) => reply ?? {'phone': '01099990001'});
  final env = await TestEnv.create(fake: fake, role: 'staff_admin');
  await pumpConsoleScreen(
    tester,
    env,
    SingleChildScrollView(
      child: ReviewAccountCard(current: null, onChanged: () {}),
    ),
  );
  await tester.tap(find.text(koT.cReviewTitle));
  await settle(tester);
  await tester.enterText(find.byType(TextField).at(0), '010-9999-0001');
  await tester.enterText(find.byType(TextField).at(1), '123456');
  await tester.tap(find.text(koT.cReviewEnable));
  await settle(tester);
  return fake;
}

Future<void> _typeOwnerCode(WidgetTester tester) async {
  expect(find.text(koT.oaTotpTitle), findsOneWidget);
  await tester.enterText(find.byType(TextField).last, '654321');
  await tester.tap(find.widgetWithText(FilledButton, koT.oaContinue));
  await settle(tester);
}

void main() {
  testWidgets('turning the review login on sends the owner code', (
    tester,
  ) async {
    final fake = await _open(tester);
    expect(
      fake.callsTo('PUT', _path),
      isEmpty,
      reason: 'nothing before the code',
    );

    await _typeOwnerCode(tester);
    final sent = fake.lastCall('PUT', _path);
    expect(sent, isNotNull);
    expect(sent!.headers['x-owner-totp'], '654321');
    expect(sent.json['phone'], '010-9999-0001');
    await unmount(tester);
  });

  testWidgets('a number someone already uses is explained', (tester) async {
    await _open(
      tester,
      reply: const ApiReply(409, {
        'error': {'code': 'phone_in_use'},
      }),
    );
    await _typeOwnerCode(tester);
    expect(find.text(koT.cReviewPhoneInUse), findsOneWidget);
    await unmount(tester);
  });
}
