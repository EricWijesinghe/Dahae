// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/host_job.dart';
import 'package:console/main.dart';
import 'package:console/screens/host_update.dart';
import 'package:console/screens/setup_app_links.dart';
import 'package:console/screens/setup_operator.dart';
import 'package:console/screens/setup_store_build.dart';
import 'package:console/server_moved_banner.dart';
import 'package:console/state.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/console_host.dart';

Widget _scrolled(Widget child) => SingleChildScrollView(child: child);

Future<void> _ownerCode(WidgetTester tester) async {
  await tester.enterText(find.byType(TextField).last, '123456');
  await tester.tap(find.widgetWithText(FilledButton, koT.oaContinue));
  await settle(tester);
}

void main() {
  setUp(() => hostJobPoll = const Duration(milliseconds: 100));

  testWidgets('the console title and login heading follow the profile name', (
    tester,
  ) async {
    setViewport(tester, wideSize);
    final fake = FakeApi()
      ..reply('GET', '/brand', {'name': '청소왕', 'primary_color': '#0B5D2A'});
    final env = await TestEnv.create(fake: fake, signedIn: false);
    final brand = BrandController(
      api: env.api,
      fallback: BrandConfig.placeholder,
      cache: JsonCache(storage: env.storage),
    );
    await brand.refresh();
    await tester.pumpWidget(
      ConsoleApp(
        session: env.session,
        state: ConsoleState(env.api, FakeRealtime(env.api), env.session),
        locale: env.locale,
        theme: env.theme,
        brand: brand,
      ),
    );
    await settle(tester);

    final app = tester.widget<MaterialApp>(find.byType(MaterialApp));
    expect(
      app.onGenerateTitle!(tester.element(find.byType(Scaffold).first)),
      '청소왕 ${koT.cTitle}',
    );
    expect(app.theme!.colorScheme.primary, const Color(0xFF0B5D2A));
    expect(find.text('청소왕 ${koT.cTitle}'), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('the store build card names what changed and records the build', (
    tester,
  ) async {
    setViewport(tester, tallPhoneSize);
    final fake = FakeApi()
      ..reply('POST', '/admin/setup/store-build/ack', {'ok': true});
    final env = await TestEnv.create(fake: fake, role: 'staff_admin');
    var acked = 0;
    await pumpConsoleScreen(
      tester,
      env,
      _scrolled(
        StoreBuildCard(
          data: const {
            'changed': ['name', 'icon', 'domain'],
            'acked_at': null,
            'build_url': null,
          },
          onAcked: () => acked++,
        ),
      ),
    );
    await settle(tester);
    expect(
      find.text(
        koT.cRbStoreChanged(
          [
            koT.cRbStoreItemName,
            koT.cRbStoreItemIcon,
            koT.cRbStoreItemDomain,
          ].join(', '),
        ),
      ),
      findsOneWidget,
    );

    await tester.enterText(
      find.byType(TextField),
      'https://github.com/acme/app/actions',
    );
    await tester.tap(find.text(koT.cRbStoreAck));
    await settle(tester);
    expect(fake.lastCall('POST', '/admin/setup/store-build/ack')!.body, {
      'build_url': 'https://github.com/acme/app/actions',
    });
    expect(acked, 1);
    await unmount(tester);
  });

  Future<FakeApi> pumpHost(WidgetTester tester, FakeApi fake) async {
    setViewport(tester, const Size(800, 2400));
    fake
      ..reply('GET', '/admin/me/account', {'is_owner': true})
      ..reply('GET', '/admin/host/jobs', {'jobs': <Json>[]});
    final env = await TestEnv.create(fake: fake, role: 'staff_admin');
    await pumpConsoleScreen(tester, env, _scrolled(const HostUpdateCard()));
    await settle(tester);
    return fake;
  }

  testWidgets('host controls send each job with the owner code', (
    tester,
  ) async {
    final fake = await pumpHost(
      tester,
      FakeApi()..reply('POST', '/admin/host/jobs', {'id': 'j'}, status: 202),
    );
    await tester.tap(find.text(koT.cRbChannelBeta));
    await settle(tester);
    await tester.tap(find.text(koT.cRbApply).first);
    await settle(tester);
    await _ownerCode(tester);
    var sent = fake.lastCall('POST', '/admin/host/jobs')!;
    expect(sent.body, {'kind': 'channel', 'arg': 'beta'});
    expect(sent.headers['x-owner-totp'], '123456');

    await tester.enterText(find.widgetWithText(TextField, koT.cRbTime), '4:00');
    await tester.tap(find.text(koT.cRbApply).last);
    await settle(tester);
    expect(find.text(koT.cRbInvalidTime), findsOneWidget);
    await tester.enterText(
      find.widgetWithText(TextField, koT.cRbTime),
      '23:30',
    );
    await tester.tap(find.text(koT.cRbApply).last);
    await settle(tester);
    await _ownerCode(tester);
    sent = fake.lastCall('POST', '/admin/host/jobs')!;
    expect(sent.body, {'kind': 'schedule', 'arg': 'mon@23:30'});

    await tester.tap(find.text(koT.cRbBackupNow));
    await settle(tester);
    await _ownerCode(tester);
    expect(fake.lastCall('POST', '/admin/host/jobs')!.body, {
      'kind': 'backup-now',
      'arg': '',
    });
    await unmount(tester);
  });

  testWidgets('the server approval code is enrolled and shown with copy fields', (
    tester,
  ) async {
    const uri =
        'otpauth://totp/Dahae:server?secret=ABCDEFGHIJKLMNOPQRSTUVWXYZ234567&issuer=Dahae&digits=6&period=30';
    final fake = await pumpHost(
      tester,
      FakeApi()
        ..reply('POST', '/admin/host/jobs', {'id': 'en'}, status: 202)
        ..reply('GET', '/admin/host/jobs/en', {'id': 'en', 'state': 'ok'})
        ..reply('GET', '/admin/host/jobs/en/output', {'text': '$uri\n'}),
    );
    await tester.tap(find.text(koT.cRbEnrollStart));
    await settle(tester);
    await _ownerCode(tester);
    await settle(tester);
    expect(fake.lastCall('POST', '/admin/host/jobs')!.body, {
      'kind': 'host-approval-enroll',
      'arg': '',
    }, reason: 'the server fills in the staff id');
    expect(find.text('ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'), findsOneWidget);
    expect(find.text(uri), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('the Toss payouts switch needs the owner code', (tester) async {
    setViewport(tester, tallPhoneSize);
    final fake = FakeApi()..reply('PUT', '/admin/setup/payout/switch', null);
    final env = await TestEnv.create(fake: fake, role: 'staff_admin');
    await pumpConsoleScreen(tester, env, const TossPayoutSwitchCard(on: false));
    await settle(tester);
    await tester.tap(find.byType(Switch));
    await settle(tester);
    await _ownerCode(tester);
    final put = fake.lastCall('PUT', '/admin/setup/payout/switch')!;
    expect(put.body, {'on': true});
    expect(put.headers['x-owner-totp'], '123456');
    expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
    await unmount(tester);
  });

  test('business hours need a day and a close after the open', () {
    expect(validBusinessHours({0, 4}, '09:00', '18:00'), isTrue);
    expect(validBusinessHours({}, '09:00', '18:00'), isFalse);
    expect(validBusinessHours({0}, '18:00', '09:00'), isFalse);
    expect(validBusinessHours({0}, '9:00', '18:00'), isFalse);
  });

  testWidgets('business hours load and save as weekdays and HH:MM', (
    tester,
  ) async {
    setViewport(tester, tallPhoneSize);
    final fake = FakeApi()
      ..reply('GET', '/admin/setup/business-hours', {
        'weekdays': [0, 1, 2, 3, 4],
        'open': '09:00',
        'close': '18:00',
      })
      ..reply('PUT', '/admin/setup/business-hours', null);
    final env = await TestEnv.create(fake: fake, role: 'staff_admin');
    await pumpConsoleScreen(tester, env, _scrolled(const BusinessHoursCard()));
    await settle(tester);
    await tester.tap(find.text(koT.cRbDays.split(',')[5])); // Saturday on
    await tester.enterText(
      find.widgetWithText(TextField, koT.cRbHoursClose),
      '20:00',
    );
    await tester.tap(find.text(koT.save));
    await settle(tester);
    expect(fake.lastCall('PUT', '/admin/setup/business-hours')!.body, {
      'weekdays': [0, 1, 2, 3, 4, 5],
      'open': '09:00',
      'close': '20:00',
    });
    await unmount(tester);
  });

  testWidgets('the app links card saves the store app IDs as lists', (
    tester,
  ) async {
    setViewport(tester, wideSize);
    final fake = FakeApi()
      ..reply('GET', '/admin/app-links', {
        'android_cert_sha256': <String>[],
        'apple_team_id': null,
        'android_packages': ['kr.example.app'],
        'ios_bundle_ids': <String>[],
        'source': 'database',
      })
      ..reply('PUT', '/admin/app-links', null);
    final env = await TestEnv.create(fake: fake, role: 'staff_admin');
    await pumpConsoleScreen(tester, env, _scrolled(const AppLinksCard()));
    await settle(tester);
    await tester.enterText(
      find.widgetWithText(TextField, koT.cRbIosBundles),
      'kr.example.app\n kr.example.partner ',
    );
    await tester.tap(find.widgetWithText(FilledButton, koT.save));
    await settle(tester);
    final body = fake.lastCall('PUT', '/admin/app-links')!.body as Json;
    expect(body['android_packages'], ['kr.example.app']);
    expect(body['ios_bundle_ids'], ['kr.example.app', 'kr.example.partner']);
    await unmount(tester);
  });

  test('a moved server is an https origin other than the saved one', () {
    const saved = 'https://old.example';
    expect(
      movedBase({'api_base': 'https://new.example'}, saved),
      'https://new.example',
    );
    expect(movedBase({'api_base': 'https://old.example/'}, saved), isNull);
    expect(movedBase({'api_base': 'http://new.example'}, saved), isNull);
    expect(movedBase({}, saved), isNull);
  });
}
