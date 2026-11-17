// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:async';

import 'package:console/screens/host_update.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/console_host.dart';

Future<void> _pump(
  WidgetTester tester, {
  required bool owner,
  List<Json> jobs = const [],
}) async {
  setViewport(tester, tallPhoneSize);
  final fake = FakeApi()
    ..reply('GET', '/admin/me/account', {'is_owner': owner})
    ..reply('GET', '/admin/host/jobs', {'jobs': jobs})
    ..reply('POST', '/admin/host/jobs', {'id': 'job-1'}, status: 202);
  final env = await TestEnv.create(fake: fake, role: 'staff_admin');
  await pumpConsoleScreen(
    tester,
    env,
    const SingleChildScrollView(child: HostUpdateCard()),
  );
  await settle(tester);
  _fake = fake;
}

late FakeApi _fake;

PrimaryButton _button(WidgetTester tester) => tester.widget<PrimaryButton>(
  find.widgetWithText(PrimaryButton, koT.hjUpdateNow),
);

void main() {
  testWidgets(
    'the owner sends an update only with a fresh code and an idempotency key',
    (tester) async {
      await _pump(tester, owner: true);
      expect(find.text(koT.hjUpdateNow), findsOneWidget);
      expect(
        _fake.callsTo('POST', '/admin/host/jobs'),
        isEmpty,
        reason: 'nothing before the code',
      );

      await tester.tap(find.text(koT.hjUpdateNow));
      await settle(tester);
      expect(find.text(koT.oaTotpTitle), findsOneWidget);
      await tester.enterText(find.byType(TextField).last, '123456');
      await tester.tap(find.widgetWithText(FilledButton, koT.oaContinue));
      await settle(tester);

      final sent = _fake.lastCall('POST', '/admin/host/jobs')!;
      expect(sent.body, {'kind': 'update', 'arg': ''});
      expect(sent.headers['x-owner-totp'], '123456');
      expect(
        sent.headers.keys.any((k) => k.toLowerCase() == 'idempotency-key'),
        isTrue,
      );
    },
  );

  testWidgets('a second tap while the update is being sent queues nothing', (
    tester,
  ) async {
    setViewport(tester, tallPhoneSize);
    final gate = Completer<void>();
    final fake = FakeApi()
      ..reply('GET', '/admin/me/account', {'is_owner': true})
      ..reply('GET', '/admin/host/jobs', {'jobs': <Json>[]})
      ..on('POST', '/admin/host/jobs', (_) async {
        await gate.future;
        return const ApiReply(202, {'id': 'job-1'});
      });
    final env = await TestEnv.create(fake: fake, role: 'staff_admin');
    await pumpConsoleScreen(
      tester,
      env,
      const SingleChildScrollView(child: HostUpdateCard()),
    );
    await settle(tester);

    await tester.tap(find.text(koT.hjUpdateNow));
    await settle(tester);
    await tester.enterText(find.byType(TextField).last, '123456');
    await tester.tap(find.widgetWithText(FilledButton, koT.oaContinue));
    await tester.pump();
    expect(_button(tester).onPressed, isNull, reason: 'off while sending');

    gate.complete();
    await settle(tester);
    expect(fake.callsTo('POST', '/admin/host/jobs'), hasLength(1));
  });

  testWidgets('other admins see the jobs but no update button', (tester) async {
    await _pump(tester, owner: false);
    expect(find.text(koT.hjUpdateNow), findsNothing);
    expect(find.text(koT.hjTitle), findsOneWidget);
  });

  testWidgets(
    'a run still queued or running turns the button off and shows its state',
    (tester) async {
      await _pump(
        tester,
        owner: true,
        jobs: [
          {
            'id': 'job-1',
            'kind': 'update',
            'arg': '',
            'state': 'running',
            'step': '백업 중',
          },
        ],
      );
      expect(_button(tester).onPressed, isNull);
      expect(
        find.text('${koT.hjKindUpdate} · ${koT.hjStateRunning} · 백업 중'),
        findsOneWidget,
      );
    },
  );
}
