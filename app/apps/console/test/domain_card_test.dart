// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/host_job.dart';
import 'package:console/screens/setup_domain.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

const _dnsOk =
    '{"domain":"new.example","server_ip":"1.2.3.4","a":["1.2.3.4"],"aaaa":[],"match":true,"aaaa_ok":true}';

/// A server that runs the DNS check, holds `domain-set` for the approval code and then ends in [end].
/// Like the agent, it stays on hold for one poll after the code is sent, then runs for one poll.
FakeApi _server({
  required Json end,
  List<Json> checklist = const [],
  List<Json> aliases = const [],
  String dns = _dnsOk,
}) {
  var approved = false;
  var polls = 0;
  return FakeApi()
    ..reply('GET', '/admin/domains', {
      'current': 'old.example',
      'aliases': aliases,
      'checklist': checklist,
    })
    ..on(
      'POST',
      '/admin/host/jobs',
      (c) => ApiReply(202, {
        'id': (c.body as Json)['kind'] == 'domain-check' ? 'chk' : 'set',
      }),
    )
    ..reply('GET', '/admin/host/jobs/chk', {'id': 'chk', 'state': 'ok'})
    ..reply('GET', '/admin/host/jobs/chk/output', {'text': dns})
    ..on(
      'GET',
      '/admin/host/jobs/set',
      (_) => switch (approved ? ++polls : 0) {
        0 || 1 => {'id': 'set', 'state': 'awaiting_approval'},
        2 => {'id': 'set', 'state': 'running'},
        _ => end,
      },
    )
    ..on('POST', '/admin/host/jobs/set/approve', (_) {
      approved = true;
      return null;
    });
}

Future<FakeApi> _pump(WidgetTester tester, FakeApi fake) async {
  setViewport(tester, tallPhoneSize);
  final env = await TestEnv.create(fake: fake, role: 'staff_admin');
  await pumpConsoleScreen(
    tester,
    env,
    const SingleChildScrollView(child: DomainCard(business: {})),
  );
  await settle(tester);
  return fake;
}

Future<void> _code(WidgetTester tester, String code) async {
  await tester.enterText(find.byType(TextField).last, code);
  await tester.tap(find.widgetWithText(FilledButton, koT.oaContinue));
  await settle(tester);
}

/// Domain typed, DNS checked, owner code and server approval code given.
Future<void> _move(WidgetTester tester) async {
  await tester.tap(find.text(koT.cRbDomainChange));
  await settle(tester);
  await tester.enterText(find.byType(TextField), 'New.Example');
  await tester.tap(find.text(koT.cRbDomainCheck));
  await settle(tester);
  expect(find.text(koT.cRbDomainDnsOk('1.2.3.4')), findsOneWidget);
  await tester.ensureVisible(find.text(koT.cRbDomainApply));
  await tester.tap(find.text(koT.cRbDomainApply));
  await settle(tester);
  await _code(tester, '111111'); // owner code
  expect(find.text(koT.cRbApprovalBody), findsOneWidget);
  await _code(tester, '222222'); // server approval code
}

void main() {
  setUp(() => hostJobPoll = const Duration(milliseconds: 100));

  testWidgets(
    'check, owner code, server approval code: the move is queued once and ends on the new address',
    (tester) async {
      final fake = await _pump(
        tester,
        _server(end: {'id': 'set', 'state': 'ok', 'step': '완료'}),
      );
      await _move(tester);

      final check = fake.callsTo('POST', '/admin/host/jobs').first;
      expect(check.body, {'kind': 'domain-check', 'arg': 'new.example'});
      expect(
        check.headers['x-owner-totp'],
        isNull,
        reason: 'a DNS check needs no owner code',
      );
      final set = fake.callsTo('POST', '/admin/host/jobs').last;
      expect(set.body, {'kind': 'domain-set', 'arg': 'new.example'});
      expect(set.headers['x-owner-totp'], '111111');
      expect(fake.lastCall('POST', '/admin/host/jobs/set/approve')!.body, {
        'code': '222222',
      });
      expect(
        fake.callsTo('POST', '/admin/host/jobs/set/approve'),
        hasLength(1),
        reason: 'the hold before the agent reads the code asks for nothing',
      );
      expect(find.text(koT.cRbDomainDone('new.example')), findsOneWidget);
      expect(find.text(koT.cRbDomainOpenNew), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets('a failed move says it was rolled back, with the reason', (
    tester,
  ) async {
    await _pump(
      tester,
      _server(
        end: {'id': 'set', 'state': 'rolled_back', 'step': '새 주소가 응답하지 않아요'},
      ),
    );
    await _move(tester);
    expect(
      find.text(koT.cRbDomainRolledBack('새 주소가 응답하지 않아요')),
      findsOneWidget,
    );
    await unmount(tester);
  });

  testWidgets('a refusal shows the agent reason; none means an old agent', (
    tester,
  ) async {
    await _pump(
      tester,
      _server(end: {'id': 'set', 'state': 'rejected', 'step': '승인 코드가 틀렸어요'}),
    );
    await _move(tester);
    expect(find.text(koT.cRbDomainRejected('승인 코드가 틀렸어요')), findsOneWidget);
    await unmount(tester);

    await _pump(tester, _server(end: {'id': 'set', 'state': 'rejected'}));
    await _move(tester);
    expect(find.text(koT.cRbDomainOldAgent), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('a DNS answer that points elsewhere offers no move', (
    tester,
  ) async {
    final fake = _server(
      end: const {},
      dns:
          '{"domain":"new.example","server_ip":"1.2.3.4","a":["9.9.9.9"],"aaaa":[],"match":false,"aaaa_ok":true}',
    );
    await _pump(tester, fake);
    await tester.tap(find.text(koT.cRbDomainChange));
    await settle(tester);
    await tester.enterText(find.byType(TextField), 'old.example');
    await tester.tap(find.text(koT.cRbDomainCheck));
    await settle(tester);
    expect(
      find.text(koT.cRbDomainInvalid),
      findsOneWidget,
      reason: 'same as now',
    );

    await tester.enterText(find.byType(TextField), 'new.example');
    await tester.tap(find.text(koT.cRbDomainCheck));
    await settle(tester);
    expect(
      find.text(koT.cRbDomainDnsBad('1.2.3.4', '9.9.9.9')),
      findsOneWidget,
    );
    expect(find.text(koT.cRbDomainApply), findsNothing);
    await unmount(tester);
  });

  testWidgets('no checklist before any move', (tester) async {
    await _pump(
      tester,
      _server(
        end: const {},
        checklist: [
          {'id': 'portone_webhook', 'value': 'https://x', 'done': false},
        ],
      ),
    );
    expect(find.text(koT.cRbChecklistTitle), findsNothing);
    await unmount(tester);
  });

  testWidgets(
    'checklist copy buttons copy the exact API values, and ticks are saved',
    (tester) async {
      final copied = <String>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            copied.add((call.arguments as Map)['text'] as String);
          }
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        ),
      );
      const url = 'https://new.example/v1/webhooks/portone';
      const vars =
          'API_BASE=https://new.example\nAPP_LINK_DOMAIN=new.example,old.example';
      final fake = await _pump(
        tester,
        _server(
          end: const {},
          aliases: [
            {'host': 'old.example', 'since': '2026-11-01T00:00:00Z'},
          ],
          checklist: [
            {'id': 'portone_webhook', 'value': url, 'done': false},
            {'id': 'github_store_vars', 'value': vars, 'done': true},
          ],
        )..reply('PUT', '/admin/domains/checklist/portone_webhook', null),
      );
      expect(find.text(koT.cRbCheck_portone_webhook), findsOneWidget);
      expect(find.text('APP_LINK_DOMAIN'), findsOneWidget);

      // One button per value: each variable's value alone, never two lines at once.
      for (final b in find.byTooltip(koT.cCopy).evaluate().toList()) {
        await tester.tap(find.byWidget(b.widget));
      }
      await settle(tester);
      expect(copied, [url, 'https://new.example', 'new.example,old.example']);

      await tester.tap(find.byType(Checkbox).first);
      await settle(tester);
      expect(
        fake.lastCall('PUT', '/admin/domains/checklist/portone_webhook')!.body,
        {'done': true},
      );
      await unmount(tester);
    },
  );
}
