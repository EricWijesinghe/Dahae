// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:async';

import 'package:console/host_job.dart';
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
  group('edition card', editionTests);

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

const _token = '0123456789abcdef0123456789abcdef';

/// A server whose installer offered [offered]. An `edition` job is held for the approval code with the agent's
/// [summary], stays on hold for one poll after the code (like the agent), then ends in [end].
Future<FakeApi> _pumpEdition(
  WidgetTester tester, {
  List<String> offered = const ['uzipsa'],
  String summary = '업데이트 판을 uzipsa 로 바꿔요 (먼저 백업해요. 데이터·키·도메인·앱은 그대로예요)',
  Json end = const {'id': 'ed', 'state': 'ok'},
}) async {
  setViewport(tester, tallPhoneSize);
  var approved = false;
  var polls = 0;
  final fake = FakeApi()
    ..reply('GET', '/admin/me/account', {'is_owner': true})
    ..reply('GET', '/admin/host/jobs', {
      'jobs': <Json>[],
      'edition': 'dahae',
      'editions_offered': offered,
    })
    ..reply('POST', '/admin/host/jobs', {
      'id': 'ed',
      'progress_token': _token,
    }, status: 202)
    ..reply('GET', '/ops-progress/$_token', {
      'state': 'awaiting_approval',
      'summary': summary,
    })
    ..on(
      'GET',
      '/admin/host/jobs/ed',
      (_) => switch (approved ? ++polls : 0) {
        0 || 1 => {
          'id': 'ed',
          'state': 'awaiting_approval',
          'progress_token': _token,
        },
        _ => end,
      },
    )
    ..on('POST', '/admin/host/jobs/ed/approve', (_) {
      approved = true;
      return null;
    });
  final env = await TestEnv.create(fake: fake, role: 'staff_admin');
  await pumpConsoleScreen(
    tester,
    env,
    const SingleChildScrollView(child: HostUpdateCard()),
  );
  await settle(tester);
  return fake;
}

Future<void> _code(WidgetTester tester, String code) async {
  await tester.enterText(find.byType(TextField).last, code);
  await tester.tap(find.widgetWithText(FilledButton, koT.oaContinue));
  await settle(tester);
}

Future<void> _startSwitch(WidgetTester tester) async {
  final button = find.widgetWithText(OutlinedButton, koT.ednSwitch('Uzipsa'));
  await tester.ensureVisible(button);
  await tester.tap(button);
  await settle(tester);
  expect(find.text(koT.ednConfirm('Uzipsa')), findsOneWidget);
  await tester.tap(find.widgetWithText(FilledButton, koT.ednSwitch('Uzipsa')));
  await settle(tester);
  await _code(tester, '111111'); // owner code
}

void editionTests() {
  setUp(() => hostJobPoll = const Duration(milliseconds: 100));

  testWidgets('no edition card on a server that was offered none', (
    tester,
  ) async {
    await _pump(tester, owner: true);
    expect(find.text(koT.ednTitle), findsNothing);
    await _pumpEdition(tester, offered: const []);
    expect(find.text(koT.ednTitle), findsNothing);
  });

  testWidgets(
    'an offered edition shows the card; the switch takes the owner code, then the approval code',
    (tester) async {
      final fake = await _pumpEdition(tester);
      expect(find.text(koT.ednTitle), findsOneWidget);
      expect(find.text(koT.ednBody('Dahae')), findsOneWidget);

      await _startSwitch(tester);
      final sent = fake.lastCall('POST', '/admin/host/jobs')!;
      expect(sent.body, {'kind': 'edition', 'arg': 'uzipsa'});
      expect(sent.headers['x-owner-totp'], '111111');
      expect(find.textContaining('업데이트 판을 uzipsa 로'), findsOneWidget);

      await _code(tester, '222222'); // server approval code
      await tester.pump(const Duration(seconds: 1));
      await settle(tester);
      expect(fake.lastCall('POST', '/admin/host/jobs/ed/approve')!.body, {
        'code': '222222',
      });
      expect(find.text(koT.ednDone('Uzipsa')), findsOneWidget);
    },
  );

  testWidgets('a held job whose summary is not an edition switch is never approved', (
    tester,
  ) async {
    final fake = await _pumpEdition(
      tester,
      summary: '도메인을 evil.example 로 바꿔요',
    );
    await _startSwitch(tester);
    await tester.pump(const Duration(seconds: 6));
    await settle(tester);
    expect(fake.callsTo('POST', '/admin/host/jobs/ed/approve'), isEmpty);
    expect(find.text(koT.ednNoSummary), findsOneWidget);
  });

  testWidgets('a switch that rolled back says the server stayed on its edition', (
    tester,
  ) async {
    await _pumpEdition(
      tester,
      end: const {'id': 'ed', 'state': 'rolled_back', 'step': 'x'},
    );
    await _startSwitch(tester);
    await _code(tester, '222222');
    await tester.pump(const Duration(seconds: 1));
    await settle(tester);
    expect(find.text(koT.ednRolledBack('Dahae')), findsOneWidget);
  });

  testWidgets('a held switch to another edition than the one picked is never approved', (
    tester,
  ) async {
    final fake = await _pumpEdition(
      tester,
      summary: '업데이트 판을 other 로 바꿔요 (먼저 백업해요. 데이터·키·도메인·앱은 그대로예요)',
    );
    await _startSwitch(tester);
    await tester.pump(const Duration(seconds: 6));
    await settle(tester);
    expect(fake.callsTo('POST', '/admin/host/jobs/ed/approve'), isEmpty);
    expect(find.text(koT.ednNoSummary), findsOneWidget);
  });

  testWidgets('a switch that kept the old tools (exit 5) says it switched and asks for Update now', (
    tester,
  ) async {
    await _pumpEdition(
      tester,
      end: const {
        'id': 'ed',
        'state': 'failed',
        'step': "uzipsa 로 바꿨고 정상이에요. 관리 도구는 '지금 업데이트' 를 한 번 눌러 주세요",
      },
    );
    await _startSwitch(tester);
    await _code(tester, '222222');
    await tester.pump(const Duration(seconds: 1));
    await settle(tester);
    expect(find.text('${koT.ednDone('Uzipsa')} ${koT.ednToolsStale}'), findsOneWidget);
    expect(find.textContaining(koT.ednFailed('')), findsNothing);
  });
}
