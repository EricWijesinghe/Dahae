import 'package:flutter/material.dart';
import 'package:customer/screens/more.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/customer_host.dart';
import 'support/customer_routes.dart';

const _version = '2026-11-17';

Map<String, Object?> _consents({bool marketing = false}) => {
  'version': _version,
  'reconsent_required': false,
  'items': [
    {'kind': 'terms', 'required': true, 'granted': true, 'granted_at': null},
    {'kind': 'privacy', 'required': true, 'granted': true, 'granted_at': null},
    {
      'kind': 'overseas_transfer',
      'required': false,
      'granted': true,
      'granted_at': null,
    },
    {
      'kind': 'marketing',
      'required': false,
      'granted': marketing,
      'granted_at': null,
    },
  ],
};

/// More -> profile card (AccountScreen). The card is the first AppCard on the More tab.
Future<void> _openAccount(WidgetTester tester) async {
  await pumpUntilFound(tester, find.text(koT.tabMore));
  await tester.tap(find.text(koT.tabMore));
  await settle(tester);
  await tester.tap(
    find
        .descendant(of: find.byType(MoreTab), matching: find.byType(AppCard))
        .first,
  );
  await settle(tester);
}

/// After a policy version bump: the required grants are for the old text, so none counts.
Map<String, Object?> _staleConsents() => {
  'version': _version,
  'reconsent_required': true,
  'items': [
    for (final kind in ['terms', 'privacy'])
      {'kind': kind, 'required': true, 'granted': false, 'granted_at': null},
    {
      'kind': 'overseas_transfer',
      'required': false,
      'granted': false,
      'granted_at': null,
    },
    {
      'kind': 'marketing',
      'required': false,
      'granted': false,
      'granted_at': null,
    },
  ],
};

void main() {
  testWidgets(
    'after a policy change the consent screen opens on start and the required items can be ticked again',
    (tester) async {
      setViewport(tester, phoneSize);
      final fake = FakeApi();
      stockCustomerApi(fake);
      fake.reply('GET', '/me/consents', _staleConsents());
      fake.reply('PUT', '/me/consents', _consents());
      final env = await TestEnv.create(fake: fake);
      await pumpCustomerApp(tester, env);
      await pumpUntilFound(tester, find.text(koT.pvReconsent));

      for (final kind in ['terms', 'privacy', 'overseas_transfer']) {
        await tester.tap(find.byKey(ValueKey('consent-$kind')));
        await settle(tester);
      }
      await tester.tap(find.text(koT.save));
      await settle(tester);

      final body = fake.lastCall('PUT', '/me/consents')!.json;
      final sent = (body['consents'] as List).cast<Map<String, dynamic>>();
      for (final kind in ['terms', 'privacy', 'overseas_transfer']) {
        expect(sent.firstWhere((c) => c['kind'] == kind), {
          'kind': kind,
          'granted': true,
          'version': _version,
        });
      }
      expect(
        tester
            .widget<CheckboxListTile>(
              find.byKey(const ValueKey('consent-terms')),
            )
            .onChanged,
        isNull,
        reason: 'once granted at the current version it locks again',
      );
    },
  );

  testWidgets(
    'marketing can be granted and saved with the policy version; required consents stay on',
    (tester) async {
      setViewport(tester, phoneSize);
      final fake = FakeApi();
      stockCustomerApi(fake);
      fake.reply('GET', '/me/consents', _consents());
      fake.reply('PUT', '/me/consents', _consents(marketing: true));
      final env = await TestEnv.create(fake: fake);
      await pumpCustomerApp(tester, env);
      await _openAccount(tester);

      await tester.tap(find.text(koT.pvManage));
      await settle(tester);
      expect(
        tester
            .widget<CheckboxListTile>(
              find.byKey(const ValueKey('consent-terms')),
            )
            .onChanged,
        isNull,
        reason: 'required consent is locked',
      );

      await tester.tap(find.byKey(const ValueKey('consent-marketing')));
      await settle(tester);
      await tester.tap(find.text(koT.save));
      await settle(tester);

      final body = fake.lastCall('PUT', '/me/consents')!.json;
      final sent = (body['consents'] as List).cast<Map<String, dynamic>>();
      expect(sent.firstWhere((c) => c['kind'] == 'marketing'), {
        'kind': 'marketing',
        'granted': true,
        'version': _version,
      });
      expect(
        sent.length,
        4,
        reason: 'every item is sent so the server records the full state',
      );
    },
  );

  testWidgets(
    'overseas transfer can be withdrawn from consent settings and is sent as not granted',
    (tester) async {
      setViewport(tester, phoneSize);
      final fake = FakeApi();
      stockCustomerApi(fake);
      fake.reply('GET', '/me/consents', _consents());
      fake.reply('PUT', '/me/consents', _consents());
      final env = await TestEnv.create(fake: fake);
      await pumpCustomerApp(tester, env);
      await _openAccount(tester);

      await tester.tap(find.text(koT.pvManage));
      await settle(tester);
      await tester.tap(find.byKey(const ValueKey('consent-overseas_transfer')));
      await settle(tester);
      await tester.tap(find.text(koT.save));
      await settle(tester);

      final body = fake.lastCall('PUT', '/me/consents')!.json;
      final sent = (body['consents'] as List).cast<Map<String, dynamic>>();
      expect(sent.firstWhere((c) => c['kind'] == 'overseas_transfer'), {
        'kind': 'overseas_transfer',
        'granted': false,
        'version': _version,
      });
    },
  );

  testWidgets('data export shows the server document as JSON and offers copy', (
    tester,
  ) async {
    setViewport(tester, phoneSize);
    final fake = FakeApi();
    stockCustomerApi(fake);
    fake.reply('GET', '/me/export', {
      'profile': {'phone': '010-5555-0101', 'name': '지민'},
      'bookings': <Object>[],
    });
    final env = await TestEnv.create(fake: fake);
    await pumpCustomerApp(tester, env);
    await _openAccount(tester);

    await tester.scrollUntilVisible(
      find.text(koT.pvExport),
      200,
      scrollable: find.descendant(
        of: find.byType(AccountScreen),
        matching: find.byType(Scrollable),
      ),
    );
    await tester.tap(find.text(koT.pvExport));
    await settle(tester);

    expect(find.textContaining('010-5555-0101'), findsOneWidget);
    expect(find.text(koT.pvCopy), findsOneWidget);
  });
}
