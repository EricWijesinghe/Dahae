// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/main.dart';
import 'package:console/state.dart';
import 'package:core/core.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/console_host.dart';

const _brand = BrandConfig(
  name: '우집사',
  nameEn: 'Uzipsa',
  primary: Color(0xFF187A8F),
  primaryDeep: Color(0xFF12606F),
  fresh: Color(0xFF1E9EBA),
  sun: Color(0xFFFFD95A),
);

Future<void> _pumpConsole(
  WidgetTester tester,
  String lang,
  BrandConfig b, {
  bool signedIn = false,
}) async {
  setViewport(tester, wideSize);
  final fake = FakeApi();
  stockConsoleApi(fake);
  final env = await TestEnv.create(
    fake: fake,
    signedIn: signedIn,
    localeCode: lang,
  );
  final brand = BrandController(api: env.api, fallback: b);
  addTearDown(brand.dispose);
  await tester.pumpWidget(
    ConsoleApp(
      session: env.session,
      state: ConsoleState(env.api, FakeRealtime(env.api), env.session),
      locale: env.locale,
      theme: env.theme,
      brand: brand,
    ),
  );
  await settle(tester, duration: const Duration(milliseconds: 500));
}

String _windowTitle(WidgetTester tester) =>
    tester.widget<Title>(find.byType(Title)).title;

void main() {
  testWidgets('in English the console title uses the brand English name', (
    tester,
  ) async {
    await _pumpConsole(tester, 'en', _brand);
    final title = 'Uzipsa ${lookupT(const Locale('en')).cTitle}';
    expect(_windowTitle(tester), title);
    expect(find.text(title), findsOneWidget);
    expect(find.textContaining('우집사'), findsNothing);
    await unmount(tester);
  });

  testWidgets('in Korean the console title keeps the Korean name', (
    tester,
  ) async {
    await _pumpConsole(tester, 'ko', _brand);
    final title = '우집사 ${koT.cTitle}';
    expect(_windowTitle(tester), title);
    expect(find.text(title), findsOneWidget);
    await unmount(tester);
  });

  testWidgets(
    'signed in, the English side-nav header uses the brand English name',
    (tester) async {
      await _pumpConsole(tester, 'en', _brand, signedIn: true);
      final title = 'Uzipsa ${lookupT(const Locale('en')).cTitle}';
      expect(_windowTitle(tester), title);
      expect(find.text(title), findsOneWidget);
      expect(find.textContaining('우집사'), findsNothing);
      await unmount(tester);
    },
  );

  testWidgets('a brand without an English name sets its name apart in English', (
    tester,
  ) async {
    await _pumpConsole(tester, 'en', BrandConfig.placeholder);
    final title =
        '${lookupT(const Locale('en')).cTitle} · ${BrandConfig.placeholder.name}';
    expect(_windowTitle(tester), title);
    expect(find.text(title), findsOneWidget);
    await unmount(tester);
  });
}
