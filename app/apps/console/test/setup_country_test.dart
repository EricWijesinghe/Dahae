// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/screens/setup_country.dart';
import 'package:console/screens/setup_guides.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/console_host.dart';

final enT = lookupT(const Locale('en'));
final arT = lookupT(const Locale('ar'));

const _packs = '/admin/setup/packs';
const _country = '/admin/setup/country';

/// A pack card as `GET /admin/setup/packs` serves it.
Json _pack(String id, String name, String level, Json currency) => {
  'id': id,
  'name': name,
  'maturity': {'level': level},
  'currency': currency['code'],
  'minor_units': currency['minor_units'],
  'zones': ['Europe/London'],
  'locales': ['en'],
  'recommended': {
    'payments': ['stripe'],
    'messaging': ['twilio'],
  },
  'legal': ['terms', 'privacy'],
};

final _kr = {
  ..._pack('KR', '대한민국', 'reviewed', {'code': 'KRW', 'minor_units': 0}),
  'zones': ['Asia/Seoul'],
  'locales': ['ko', 'en'],
  'recommended': {
    'payments': ['portone', 'tosspayments'],
    'messaging': ['twilio'],
  },
  'legal': ['terms', 'privacy', 'location_terms'],
};
final _gb = _pack('GB', 'United Kingdom', 'draft', {
  'code': 'GBP',
  'minor_units': 2,
});

/// A fresh install: no country yet, the server guesses KR.
Json _fresh() => {
  'current': {'country': null, 'locked': false},
  'default': 'KR',
  'packs': [_kr, _gb],
};

Future<FakeApi> _open(
  WidgetTester tester, {
  Object? packs,
  int status = 200,
  String locale = 'en',
}) async {
  setViewport(tester, tallPhoneSize);
  final fake = FakeApi()
    ..reply('GET', _packs, packs ?? _fresh(), status: status)
    ..on('PUT', _country, (_) => {'country': 'GB'});
  final env = await TestEnv.create(fake: fake, role: 'staff_admin');
  env.locale.locale = Locale(locale);
  await pumpConsoleScreen(
    tester,
    env,
    SingleChildScrollView(child: SetupCountryCard(onSaved: () {})),
    locales: T.supportedLocales,
  );
  await settle(tester);
  return fake;
}

void main() {
  testWidgets('a fresh install picks a country after a preview', (
    tester,
  ) async {
    final fake = await _open(tester);
    expect(find.text(enT.cLangStepTitle), findsOneWidget);
    expect(find.text(enT.cCountryStepTitle), findsOneWidget);
    expect(find.text(enT.cCountryDetected('대한민국')), findsOneWidget);
    // The detected country is preselected and previewed, never saved on its own.
    expect(
      find.textContaining('KRW · ${enT.cCountryPriceSample('₩49,000')}'),
      findsOneWidget,
    );
    expect(
      find.textContaining(
        '${guides['portone']!.title(enT)}, ${guides['tosspayments']!.title(enT)}, twilio',
      ),
      findsOneWidget,
    );
    expect(fake.callsTo('PUT', _country), isEmpty);

    await tester.tap(find.text('United Kingdom'));
    await settle(tester);
    expect(find.textContaining(enT.cCountryPriceSample('£49.90')), findsOne);
    expect(find.text(enT.cCountryDraftWarn), findsOneWidget);
    final confirm = find.widgetWithText(FilledButton, enT.cCountryConfirm);
    expect(tester.widget<FilledButton>(confirm).onPressed, isNull);

    await tester.tap(find.text(enT.cCountryDraftAck));
    await settle(tester);
    await tester.ensureVisible(confirm);
    await tester.tap(confirm);
    await settle(tester);
    expect(fake.lastCall('PUT', _country)?.body, {
      'country': 'GB',
      'draft_ack': true,
    });
    expect(find.text(enT.cCountryStepTitle), findsNothing);
  });

  testWidgets('search narrows the list', (tester) async {
    await _open(tester);
    await tester.enterText(find.byType(TextField), 'king');
    await settle(tester);
    expect(find.text('United Kingdom'), findsOneWidget);
    expect(find.text('대한민국'), findsNothing);
    await tester.enterText(find.byType(TextField), 'zz');
    await settle(tester);
    expect(find.text(enT.cCountryNone), findsOneWidget);
  });

  testWidgets('an install whose country is set never sees the steps', (
    tester,
  ) async {
    await _open(
      tester,
      packs: {
        ..._fresh(),
        'current': {'country': 'KR', 'locked': false},
      },
      locale: 'ko',
    );
    expect(find.text(koT.cCountryStepTitle), findsNothing);
    expect(find.byType(AppCard), findsNothing);
  });

  testWidgets('a server without the pack list shows nothing', (tester) async {
    await _open(
      tester,
      packs: {
        'error': {'code': 'not_found'},
      },
      status: 404,
    );
    expect(find.text(enT.cCountryStepTitle), findsNothing);
    expect(find.byType(ErrorRetry), findsNothing);
  });

  testWidgets('Arabic lays the steps out right to left', (tester) async {
    await _open(tester, locale: 'ar');
    final title = find.text(arT.cCountryStepTitle);
    expect(title, findsOneWidget);
    expect(Directionality.of(tester.element(title)), TextDirection.rtl);
    // Start-aligned text sits on the right in Arabic.
    final card = tester.getRect(find.byType(AppCard));
    expect(tester.getRect(title).right, greaterThan(card.center.dx));
    expect(find.text(arT.cCountryConfirm), findsOneWidget);
  });

  test('a sample price fits the currency', () {
    expect(samplePrice(Currency.krw), 49000);
    expect(samplePrice(const Currency('EUR', 2)), 4990);
    expect(samplePrice(const Currency('KWD', 3)), 49900);
  });
}
