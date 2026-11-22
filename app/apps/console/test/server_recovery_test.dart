// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:convert';

import 'package:console/host_job.dart';
import 'package:console/screens/server_recovery.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

const _token = '0123456789abcdef0123456789abcdef';
const _rollbackSummary = '이전 버전으로 되돌려요 (데이터는 그대로예요)';

/// A server that holds `rollback` for the approval code and then ends in [end]; `keys-backup` ends in [keysEnd]
/// with [keysOut] as its one-time output. Like the agent, it stays on hold for one poll after the code.
FakeApi _server({
  Json end = const {'id': 'rb', 'state': 'ok'},
  String summary = _rollbackSummary,
  Json keysEnd = const {'id': 'keys', 'state': 'ok'},
  String keysOut = '',
}) {
  var approved = false;
  var polls = 0;
  return FakeApi()
    ..on(
      'POST',
      '/admin/host/jobs',
      (c) => ApiReply(202, {
        'id': (c.body as Json)['kind'] == 'rollback' ? 'rb' : 'keys',
        'progress_token': _token,
      }),
    )
    ..reply('GET', '/ops-progress/$_token', {
      'state': 'awaiting_approval',
      'summary': summary,
    })
    ..on(
      'GET',
      '/admin/host/jobs/rb',
      (_) => switch (approved ? ++polls : 0) {
        0 || 1 => {
          'id': 'rb',
          'state': 'awaiting_approval',
          'progress_token': _token,
        },
        2 => {'id': 'rb', 'state': 'running', 'progress_token': _token},
        _ => end,
      },
    )
    ..on('POST', '/admin/host/jobs/rb/approve', (_) {
      approved = true;
      return null;
    })
    ..reply('GET', '/admin/host/jobs/keys', keysEnd)
    ..reply('GET', '/admin/host/jobs/keys/output', {'text': keysOut});
}

Future<FakeApi> _pump(WidgetTester tester, FakeApi fake) async {
  setViewport(tester, tallPhoneSize);
  final env = await TestEnv.create(fake: fake, role: 'staff_admin');
  await pumpConsoleScreen(
    tester,
    env,
    SingleChildScrollView(child: ServerRecoveryCard(onQueued: () async {})),
  );
  await settle(tester);
  return fake;
}

Future<void> _code(WidgetTester tester, String code) async {
  await tester.enterText(find.byType(TextField).last, code);
  await tester.tap(find.widgetWithText(FilledButton, koT.oaContinue));
  await settle(tester);
}

Future<void> _startRollback(WidgetTester tester) async {
  await tester.tap(find.widgetWithText(OutlinedButton, koT.srRollbackButton));
  await settle(tester);
  expect(find.text(koT.srRollbackConfirm), findsOneWidget);
  await tester.tap(find.widgetWithText(FilledButton, koT.srRollbackButton));
  await settle(tester);
  await _code(tester, '111111'); // owner code
}

Future<void> _keys(WidgetTester tester, String pass, [String? again]) async {
  final fields = find.byType(TextField);
  await tester.enterText(fields.at(0), pass);
  await tester.enterText(fields.at(1), again ?? pass);
  await tester.tap(find.widgetWithText(OutlinedButton, koT.srKeysButton));
  await settle(tester);
}

void main() {
  final saved = <(List<int>, String)>[];
  setUp(() {
    hostJobPoll = const Duration(milliseconds: 100);
    saved.clear();
    saveKeyCopy = (bytes, name) async {
      saved.add((bytes, name));
      return true;
    };
  });

  testWidgets(
    'rollback sends the owner code, then the approval code for the agent\'s rollback summary',
    (tester) async {
      final fake = await _pump(tester, _server());
      await _startRollback(tester);
      final sent = fake.lastCall('POST', '/admin/host/jobs')!;
      expect(sent.body, {'kind': 'rollback', 'arg': ''});
      expect(sent.headers['x-owner-totp'], '111111');

      expect(find.textContaining(koT.cRbApprovalBody), findsOneWidget);
      expect(find.textContaining(_rollbackSummary), findsOneWidget);
      await _code(tester, '222222'); // server approval code
      await tester.pump(const Duration(seconds: 1));
      await settle(tester);
      expect(fake.lastCall('POST', '/admin/host/jobs/rb/approve')!.body, {
        'code': '222222',
      });
      expect(find.text(koT.srRollbackDone), findsOneWidget);
    },
  );

  testWidgets('a held job whose summary is not a rollback is never approved', (
    tester,
  ) async {
    final fake = await _pump(
      tester,
      _server(summary: '도메인을 evil.example 로 바꿔요'),
    );
    await _startRollback(tester);
    await tester.pump(const Duration(seconds: 6));
    await settle(tester);
    expect(find.text(koT.cRbApprovalNoSummary), findsOneWidget);
    expect(find.textContaining(koT.cRbApprovalBody), findsNothing);
    expect(fake.callsTo('POST', '/admin/host/jobs/rb/approve'), isEmpty);
  });

  testWidgets('a rollback the payout check stopped shows the agent\'s reason', (
    tester,
  ) async {
    const why = '토스 결과를 기다리는 정산이 있어서 되돌리지 않았어요';
    await _pump(
      tester,
      _server(end: const {'id': 'rb', 'state': 'failed', 'step': why}),
    );
    await _startRollback(tester);
    await _code(tester, '222222');
    await tester.pump(const Duration(seconds: 1));
    await settle(tester);
    expect(find.text(koT.srFailed(why)), findsOneWidget);
  });

  testWidgets('a short or unmatched passphrase sends nothing', (tester) async {
    final fake = await _pump(tester, _server());
    await _keys(tester, 'elevenchars');
    expect(find.text(koT.srKeysPassShort), findsOneWidget);
    await _keys(tester, 'correct horse battery', 'correct horse batterY');
    expect(find.text(koT.srKeysPassMismatch), findsOneWidget);
    expect(fake.callsTo('POST', '/admin/host/jobs'), isEmpty);
    expect(find.text(koT.oaTotpTitle), findsNothing);
  });

  testWidgets(
    'the key copy goes out as hex behind the owner code and is saved from the one-time output',
    (tester) async {
      final sealed = utf8.encode('Salted__ sealed bytes');
      final fake = await _pump(tester, _server(keysOut: base64.encode(sealed)));
      await _keys(tester, '비밀번호 열두 자 이상');
      await _code(tester, '111111');
      final sent = fake.lastCall('POST', '/admin/host/jobs')!;
      expect(sent.body, {
        'kind': 'keys-backup',
        'arg': hex(utf8.encode('비밀번호 열두 자 이상')),
      });
      expect(sent.headers['x-owner-totp'], '111111');
      expect(saved, hasLength(1));
      expect(saved.single.$1, sealed);
      expect(
        saved.single.$2,
        matches(RegExp(r'^dahae-keys-\d{8}-\d{6}\.env\.enc$')),
      );
      expect(find.text(koT.srKeysDone), findsOneWidget);
      expect(
        tester.widget<TextField>(find.byType(TextField).first).controller!.text,
        isEmpty,
        reason: 'the passphrase does not stay on screen',
      );
    },
  );

  testWidgets('a key copy that was not made says why and saves nothing', (
    tester,
  ) async {
    const why = '비밀 키 사본 파일을 찾지 못했어요';
    await _pump(
      tester,
      _server(keysEnd: const {'id': 'keys', 'state': 'failed', 'step': why}),
    );
    await _keys(tester, 'correct horse battery');
    await _code(tester, '111111');
    expect(saved, isEmpty);
    expect(find.text(koT.srKeysFailed(why)), findsOneWidget);
  });

  test('hex is lowercase and two digits per byte', () {
    expect(hex([0, 10, 255]), '000aff');
  });
}
