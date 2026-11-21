// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/brand_assets.dart';
import 'package:console/screens/setup_business.dart';
import 'package:console/state.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

Widget _scrolled(Widget child) => SingleChildScrollView(child: child);

void main() {
  testWidgets(
    'a colour white text cannot read gets a warning that the apps will not use it',
    (tester) async {
      setViewport(tester, wideSize);
      final env = await TestEnv.create(fake: FakeApi(), role: 'staff_admin');
      await pumpConsoleScreen(
        tester,
        env,
        _scrolled(
          BusinessCard(
            initial: const {'primary_color': '#0B5D2A'},
            onSaved: () {},
          ),
        ),
      );
      await settle(tester);
      expect(find.text(koT.cSetupColorContrast), findsNothing);

      await tester.enterText(
        find.widgetWithText(TextField, '#RRGGBB'),
        '#FFD95A',
      );
      await settle(tester, duration: const Duration(milliseconds: 200));

      expect(find.text(koT.cSetupColorContrast), findsOneWidget);
      expect(koT.cSetupColorContrast, contains('앱에는 적용되지 않아요'));

      await tester.enterText(
        find.widgetWithText(TextField, '#RRGGBB'),
        '#0B5D2A',
      );
      await settle(tester, duration: const Duration(milliseconds: 200));
      expect(find.text(koT.cSetupColorContrast), findsNothing);
      await unmount(tester);
    },
  );

  testWidgets(
    'the new profile fields load, save in one PUT and keep the saved images',
    (tester) async {
      setViewport(tester, wideSize);
      final fake = FakeApi()..reply('PUT', '/admin/setup/business', null);
      final env = await TestEnv.create(fake: fake, role: 'staff_admin');
      var saved = 0;
      const assets = {'icon_512': '/files/a.png'};
      await pumpConsoleScreen(
        tester,
        env,
        _scrolled(
          BusinessCard(
            initial: const {
              'name': '청소왕',
              'name_en': 'Clean King',
              'short_name': '청소왕',
              'tagline': '깨끗하게',
              'tagline_en': 'Spotless',
              'primary_color': '#187A8F',
              'logo_url': '/files/logo.png',
              'brand_assets': assets,
              'company': {'privacy_email': 'privacy@example.kr'},
            },
            onSaved: () => saved++,
          ),
        ),
      );
      await settle(tester);
      await tester.tap(
        find.text(koT.cSetupBusiness),
      ); // a named business starts folded
      await settle(tester);

      await tester.enterText(
        find.widgetWithText(TextField, koT.cRbTaglineEn),
        'Clean homes',
      );
      await tester.ensureVisible(find.text(koT.cSetupSave));
      await tester.tap(find.text(koT.cSetupSave));
      await settle(tester);

      expect(
        fake.callsTo('POST', '/uploads'),
        isEmpty,
        reason: 'logo and colour unchanged',
      );
      final body = fake.lastCall('PUT', '/admin/setup/business')!.body as Json;
      expect(body['name_en'], 'Clean King');
      expect(body['short_name'], '청소왕');
      expect(body['tagline'], '깨끗하게');
      expect(body['tagline_en'], 'Clean homes');
      expect((body['company'] as Json)['privacy_email'], 'privacy@example.kr');
      expect(body['brand_assets'], assets);
      expect(find.text(koT.cRbAppliedTitle), findsOneWidget);
      await tester.tap(find.text(koT.ok));
      await settle(tester);
      expect(saved, 1);
      await unmount(tester);
    },
  );

  testWidgets(
    'without a logo a renamed business is warned, and a letter mark is made, previewed and uploaded with every slot',
    (tester) async {
      setViewport(tester, wideSize);
      final fake = FakeApi()..reply('PUT', '/admin/setup/business', null);
      final env = await TestEnv.create(fake: fake, role: 'staff_admin');
      // The fake HTTP layer reads bodies as text, so binary PNG uploads are recorded here instead.
      final api = _UploadApi(env);
      await tester.pumpWidget(
        ConsoleTestHost(
          env: env,
          state: ConsoleState(api, FakeRealtime(api), env.session),
          home: _scrolled(
            BusinessCard(
              initial: const {'name': '청소왕', 'primary_color': '#187A8F'},
              onSaved: () {},
            ),
          ),
        ),
      );
      await settle(tester);
      await tester.tap(find.text(koT.cSetupBusiness));
      await settle(tester);
      expect(find.text(koT.cRbNoLogoWarn), findsOneWidget);

      await tester.tap(find.text(koT.cRbLetterMark));
      await _realWork(tester);
      expect(find.text(koT.cRbNoLogoWarn), findsNothing);

      await tester.ensureVisible(find.text(koT.cSetupSave));
      await tester.tap(find.text(koT.cSetupSave));
      await _realWork(tester);
      expect(find.text(koT.cRbPreviewTitle), findsOneWidget);
      await tester.tap(find.widgetWithText(FilledButton, koT.cSetupSave).last);
      await _realWork(tester);

      expect(api.uploads.first, 'logo.png');
      expect(api.uploads.skip(1).toSet(), {
        for (final slot in brandSlots.keys) '$slot.png',
      }, reason: 'logo + 7 slots');
      final body = fake.lastCall('PUT', '/admin/setup/business')!.body as Json;
      expect(body['logo_url'], '/files/0.png');
      expect((body['brand_assets'] as Json).keys, hasLength(7));
      await tester.tap(find.text(koT.ok));
      await unmount(tester);
    },
  );
}

class _UploadApi extends ApiClient {
  _UploadApi(TestEnv env)
    : super(
        baseUrl: TestEnv.baseUrl,
        session: env.session,
        client: env.fake.client,
      );

  final uploads = <String>[];

  @override
  Future<String> upload(List<int> bytes, String filename) async {
    expect(bytes.take(4), [0x89, 0x50, 0x4E, 0x47], reason: 'PNG signature');
    uploads.add(filename);
    return '/files/${uploads.length - 1}.png';
  }
}

/// Image drawing and encoding run in the engine, outside fake time: let real time pass between frames.
Future<void> _realWork(WidgetTester tester) async {
  for (var i = 0; i < 20; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 50)),
    );
    await tester.pump(const Duration(milliseconds: 100));
  }
}
