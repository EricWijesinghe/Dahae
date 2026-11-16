import 'package:console/screens/people.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';
import 'support/payout_fixtures.dart';

const _id = 'po-9';
const _sync = '/admin/settlements/payouts/$_id/sync';
const _notSent = '/admin/settlements/payouts/$_id/not-sent';

/// A Toss batch whose send ended without an answer. Toss cannot be reached, or with [listed] false it
/// answers but has no record of the send.
FakeApi _fake({bool? listed}) {
  final row = {
    ...payoutJson(_id, 'requested', failure: 'status 502 Bad Gateway'),
    'provider': 'toss',
  };
  return FakeApi()
    ..reply('GET', '/admin/settlements/payouts', [row])
    ..reply(
      'POST',
      _sync,
      listed == null
          ? {
              'error': {'code': 'provider_unreachable'},
            }
          : {'id': _id, 'state': 'requested'},
      status: listed == null ? 503 : 200,
    )
    ..reply('POST', _notSent, {'id': _id, 'state': 'failed'});
}

Future<void> _pump(WidgetTester tester, FakeApi fake, [T? t]) async {
  setViewport(tester, tallPhoneSize);
  final env = await TestEnv.create(
    fake: fake,
    role: 'staff_admin',
    localeCode: t == null ? 'ko' : 'en',
  );
  await pumpConsoleScreen(
    tester,
    env,
    const SettlementsScreen(initialPayouts: true),
  );
  await pumpUntilFound(tester, find.text((t ?? koT).poCheckToss));
}

/// Checks with Toss (unreachable), then opens the not-sent dialog.
Future<void> _openDialog(WidgetTester tester, [T? t]) async {
  await tester.tap(find.text((t ?? koT).poCheckToss));
  await settle(tester);
  await tester.tap(find.text((t ?? koT).poNotSent));
  await settle(tester);
}

void main() {
  testWidgets(
    'not-sent is offered only after the Toss check could not reach Toss',
    (tester) async {
      final fake = _fake();
      await _pump(tester, fake);
      expect(find.text(koT.poNotSent), findsNothing);
      expect(
        find.text(koT.poMarkFailed),
        findsNothing,
        reason: 'refused by the server for an ambiguous send',
      );

      await tester.tap(find.text(koT.poCheckToss));
      await settle(tester);
      expect(fake.callsTo('POST', _sync), hasLength(1));
      expect(find.text(koT.poNotSent), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets(
    'the dialog explains the Toss check and sends the owner code once ticked',
    (tester) async {
      final fake = _fake();
      await _pump(tester, fake);
      await _openDialog(tester);

      expect(find.text(koT.poNotSentTitle), findsOneWidget);
      expect(find.text(koT.poNotSentWarning), findsOneWidget);
      final confirm = find.widgetWithText(FilledButton, koT.poNotSentConfirm);
      expect(
        tester.widget<FilledButton>(confirm).onPressed,
        isNull,
        reason: 'off until the owner ticks the check',
      );

      await tester.tap(find.text(koT.poNotSentChecked));
      await settle(tester);
      await tester.tap(confirm);
      await settle(tester);
      expect(find.text(koT.oaTotpTitle), findsOneWidget);
      expect(
        fake.callsTo('POST', _notSent),
        isEmpty,
        reason: 'nothing sent before the code',
      );

      await tester.enterText(find.byType(TextField), '123456');
      await tester.tap(find.widgetWithText(FilledButton, koT.oaContinue));
      await settle(tester);
      final sent = fake.lastCall('POST', _notSent);
      expect(sent, isNotNull);
      expect(sent!.headers['x-owner-totp'], '123456');
      expect(find.text(koT.poNotSentDone), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets(
    'not-sent is offered too when Toss answers but has no record of the send',
    (tester) async {
      await _pump(tester, _fake(listed: false));
      await tester.tap(find.text(koT.poCheckToss));
      await settle(tester);
      expect(find.text(koT.poCheckPending), findsOneWidget);
      expect(find.text(koT.poNotSent), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets('closing the dialog sends nothing', (tester) async {
    final fake = _fake();
    await _pump(tester, fake);
    await _openDialog(tester);
    await tester.tap(find.text(koT.close));
    await settle(tester);
    expect(find.text(koT.oaTotpTitle), findsNothing);
    expect(fake.callsTo('POST', _notSent), isEmpty);
    await unmount(tester);
  });

  testWidgets('the dialog reads in English too', (tester) async {
    final en = lookupT(const Locale('en'));
    await _pump(tester, _fake(), en);
    await _openDialog(tester, en);
    expect(find.text(en.poNotSentTitle), findsOneWidget);
    expect(
      find.textContaining('Toss Payments merchant dashboard'),
      findsOneWidget,
    );
    expect(find.text(en.poNotSentWarning), findsOneWidget);
    await unmount(tester);
  });
}
