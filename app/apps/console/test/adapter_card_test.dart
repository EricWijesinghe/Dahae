// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/screens/adapter_card.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

Json _field(
  String name,
  String kind, {
  bool set = false,
  bool required = true,
}) => {'name': name, 'kind': kind, 'required': required, 'set': set};

/// A catalog adapter with no bespoke card (a section the console has no guide for).
Json _generic({
  bool configured = false,
  bool locked = false,
  bool ownerCode = false,
}) => {
  'id': 'acme_pay',
  'name': 'Acme Pay',
  'section': 'acme_pay',
  'recommended': true,
  'caps': ['card'],
  'webhook_url': 'https://shop.example/v1/webhooks/acme_pay',
  'docs': {
    'keys_url': 'https://dashboard.acme.example/keys',
    'signup_url': 'https://acme.example/signup',
  },
  'configured': configured,
  'source': configured ? 'console' : 'none',
  'locked': locked,
  'mode': null,
  'owner_code': ownerCode,
  'fields': [
    _field('account_id', 'text', set: configured),
    _field('secret_key', 'secret', set: configured),
    {..._field('active', 'toggle', required: false), 'value': true},
  ],
};

/// Today's Korean catalog, trimmed: every adapter here has a bespoke card or no keys.
Json _krCatalog() => {
  'country': 'KR',
  'pack_version': '2026.11.0',
  'sections': [
    {
      'capability': 'payments',
      'adapters': [
        {
          'id': 'portone',
          'section': 'portone',
          'fields': [_field('store_id', 'text')],
        },
      ],
    },
    {
      'capability': 'payouts',
      'adapters': [
        {'id': 'manual', 'section': '', 'fields': <Json>[]},
        {
          'id': 'toss',
          'section': 'payout',
          'fields': [_field('mode', 'text')],
        },
      ],
    },
  ],
};

Json _catalogWith(Json adapter) => {
  ..._krCatalog(),
  'sections': [
    ..._krCatalog()['sections'] as List,
    {
      'capability': 'payments',
      'adapters': [adapter],
    },
  ],
};

Future<FakeApi> _pumpCatalog(WidgetTester tester, FakeApi api) async {
  setViewport(tester, const Size(390, 2400));
  final env = await TestEnv.create(fake: api, role: 'staff_admin');
  await pumpConsoleScreen(
    tester,
    env,
    ListView(children: [AdapterCatalog(onChanged: () {})]),
  );
  await settle(tester);
  return api;
}

void main() {
  testWidgets(
    'a Korean install shows no generic card: every service has its own',
    (tester) async {
      final api = FakeApi()..reply('GET', '/admin/setup/catalog', _krCatalog());
      await _pumpCatalog(tester, api);

      expect(api.callsTo('GET', '/admin/setup/catalog'), hasLength(1));
      expect(find.byType(AdapterCard), findsNothing);
      expect(find.text(koT.cAdapterOtherServices), findsNothing);
      await unmount(tester);
    },
  );

  testWidgets('a server without the catalog shows nothing', (tester) async {
    await _pumpCatalog(tester, FakeApi());

    expect(find.byType(AdapterCard), findsNothing);
    expect(find.text(koT.cAdapterOtherServices), findsNothing);
    await unmount(tester);
  });

  testWidgets(
    'an adapter without a bespoke card gets the generic one, driven by the catalog',
    (tester) async {
      final api = FakeApi()
        ..reply(
          'GET',
          '/admin/setup/catalog',
          _catalogWith(_generic(configured: true)),
        );
      await _pumpCatalog(tester, api);

      expect(find.byType(AdapterCard), findsOneWidget);
      expect(find.text('Acme Pay'), findsOneWidget);
      expect(find.text(koT.cAdapterRecommended), findsOneWidget);
      expect(find.text(koT.cAdapterKeysLink), findsOneWidget);
      expect(find.text(koT.cAdapterSignupLink), findsOneWidget);
      expect(
        find.text('https://shop.example/v1/webhooks/acme_pay'),
        findsOneWidget,
      );
      expect(find.text('Account id'), findsOneWidget);
      expect(find.text('Secret key'), findsOneWidget);
      expect(
        find.text(koT.cSetupKeepValue),
        findsNWidgets(2),
        reason: 'set fields say so and show nothing else',
      );
      expect(
        tester.widget<SwitchListTile>(find.byType(SwitchListTile)).value,
        isTrue,
      );
      expect(
        find.widgetWithText(OutlinedButton, koT.cSetupTest),
        findsOneWidget,
      );
      await unmount(tester);
    },
  );

  testWidgets(
    'a typed secret is masked, sent once on save and never shown again',
    (tester) async {
      const secret = 'sk_live_TYPED-SECRET-0001';
      final api = FakeApi()
        ..reply('GET', '/admin/setup/catalog', _catalogWith(_generic()))
        ..reply('PUT', '/admin/setup/acme_pay', {'code': 'acme_pay'});
      await _pumpCatalog(tester, api);

      expect(find.text(koT.cAdapterNotSet), findsNWidgets(2));
      final secretBox = find.widgetWithText(TextField, 'Secret key');
      await tester.enterText(secretBox, secret);
      await tester.enterText(
        find.widgetWithText(TextField, 'Account id'),
        'acct-1',
      );
      await settle(tester);
      final editable = tester.widget<EditableText>(
        find.descendant(of: secretBox, matching: find.byType(EditableText)),
      );
      expect(editable.obscureText, isTrue, reason: 'masked while typed');

      await tester.tap(find.widgetWithText(FilledButton, koT.cSetupSave));
      await settle(tester);

      final put = api.lastCall('PUT', '/admin/setup/acme_pay')!;
      expect(put.json, {'account_id': 'acct-1', 'secret_key': secret});
      expect(
        put.headers.keys.map((k) => k.toLowerCase()),
        isNot(contains('x-owner-totp')),
      );
      expect(
        find.text(secret),
        findsNothing,
        reason: 'the box is cleared once saved',
      );
      expect(
        api.callsTo('GET', '/admin/setup/catalog'),
        hasLength(2),
        reason: 'reloaded after the save',
      );
      await unmount(tester);
    },
  );

  testWidgets('a first gateway save asks for the owner code', (tester) async {
    final api = FakeApi()
      ..reply(
        'GET',
        '/admin/setup/catalog',
        _catalogWith(_generic(ownerCode: true)),
      );
    await _pumpCatalog(tester, api);

    await tester.enterText(
      find.widgetWithText(TextField, 'Secret key'),
      'sk_test_1',
    );
    await tester.tap(find.widgetWithText(FilledButton, koT.cSetupSave));
    await settle(tester);

    expect(find.text(koT.cOwnerCodeTitle), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('a locked card cannot be saved and offers the owner unlock', (
    tester,
  ) async {
    final api = FakeApi()
      ..reply(
        'GET',
        '/admin/setup/catalog',
        _catalogWith(_generic(configured: true, locked: true)),
      );
    await _pumpCatalog(tester, api);

    final save = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, koT.cSetupSave),
    );
    expect(save.onPressed, isNull);
    expect(find.text(koT.cSetupLockedBadge), findsOneWidget);
    expect(
      find.widgetWithText(OutlinedButton, koT.cSetupUnlock),
      findsOneWidget,
    );
    expect(find.widgetWithText(TextButton, koT.cSetupClear), findsNothing);
    await unmount(tester);
  });
}
