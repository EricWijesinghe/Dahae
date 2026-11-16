import 'package:console/screens/setup.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

Json _field(String name, String kind, {bool set = false, Object? preview}) => {
  'name': name,
  'kind': kind,
  'required': true,
  'set': set,
  'preview': preview,
};

Json _payoutCard({String? mode, bool configured = true}) => {
  'code': 'payout',
  'source': configured ? 'console' : 'none',
  'configured': configured,
  'mode': mode,
  'fields': [
    _field('mode', 'text', set: configured, preview: configured ? 'manual' : null),
    _field('secret_key', 'secret', set: configured, preview: configured ? '••••1234' : null),
    _field('security_key', 'secret'),
  ],
  'last_test': null,
};

Json _portoneCard() => {
  'code': 'portone',
  'source': 'none',
  'configured': false,
  'fields': [_field('store_id', 'text'), _field('api_secret', 'secret')],
  'last_test': null,
};

Future<void> _pump(WidgetTester tester, Json card, {String? webhookUrl, String? tossWebhookUrl}) async {
  setViewport(tester, tallPhoneSize);
  final env = await TestEnv.create(fake: FakeApi(), role: 'staff_admin');
  await pumpConsoleScreen(
    tester,
    env,
    ListView(
      children: [
        IntegrationCard(card: card, onChanged: () {}, webhookUrl: webhookUrl, tossWebhookUrl: tossWebhookUrl),
      ],
    ),
  );
  await settle(tester);
}

void main() {
  _lockedTests();

  testWidgets('a saved test key shows the 테스트 badge and no live badge', (tester) async {
    await _pump(tester, _payoutCard(mode: 'test'));

    expect(find.text(koT.cSetupModeTest), findsOneWidget);
    expect(find.text(koT.cSetupModeLive), findsNothing);
    await unmount(tester);
  });

  testWidgets('a saved live key shows the 실거래 badge and no test badge', (tester) async {
    await _pump(tester, _payoutCard(mode: 'live'));

    expect(find.text(koT.cSetupModeLive), findsOneWidget);
    expect(find.text(koT.cSetupModeTest), findsNothing);
    await unmount(tester);
  });

  testWidgets('no mode from the server means no badge', (tester) async {
    await _pump(tester, _payoutCard(mode: null));

    expect(find.text(koT.cSetupModeTest), findsNothing);
    expect(find.text(koT.cSetupModeLive), findsNothing);
    await unmount(tester);
  });

  testWidgets('a saved secret shows only its last four characters', (tester) async {
    await _pump(tester, _payoutCard(mode: 'test'));
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    expect(find.textContaining('••••1234'), findsOneWidget);
    expect(find.textContaining('test_sk_'), findsNothing, reason: 'the key prefix is not echoed either');
    await unmount(tester);
  });

  testWidgets('the PortOne webhook address copies to the clipboard', (tester) async {
    const url = 'https://api.example.test/v1/webhooks/portone';
    final copied = <String>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'Clipboard.setData') copied.add((call.arguments as Map)['text'] as String);
      return null;
    });
    addTearDown(() => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null));

    await _pump(tester, _portoneCard(), webhookUrl: url);
    expect(find.text(koT.cSetupWebhook), findsOneWidget);
    expect(find.text(url), findsOneWidget);

    await tester.tap(find.byTooltip(koT.cCopy));
    await settle(tester);

    expect(copied, [url]);
    await unmount(tester);
  });

  testWidgets('the Toss webhook address shows on the payout card and not on PortOne', (tester) async {
    const tossUrl = 'https://api.example.test/v1/webhooks/toss';
    // Unconfigured cards open by default, so the copy rows are on screen.
    await _pump(tester, _payoutCard(configured: false), tossWebhookUrl: tossUrl);
    expect(find.text(tossUrl), findsOneWidget);
    await unmount(tester);

    await _pump(tester, _portoneCard(), tossWebhookUrl: tossUrl);
    expect(find.text(tossUrl), findsNothing);
    await unmount(tester);
  });
}

Json _lockedPortone({bool unlocked = false}) => {
  'code': 'portone',
  'source': 'console',
  'configured': true,
  'locked': !unlocked,
  'unlock_expires_at': unlocked ? '2099-01-01T00:05:00Z' : null,
  'fields': [
    _field('store_id', 'text', set: true, preview: '••••0000'),
    _field('api_secret', 'secret', set: true, preview: '••••0000'),
    _field('webhook_secret', 'secret'),
    _field('channel_keys', 'map', set: true, preview: {'card': '••••0000'}),
    _field('cash_receipts', 'toggle'),
  ],
  'last_test': null,
};

void _lockedTests() {
  testWidgets('a saved card is locked: badge, disabled fields, disabled save, unlock button', (tester) async {
    await _pump(tester, _lockedPortone());
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    expect(find.text(koT.cSetupLockedBadge), findsOneWidget);
    expect(find.text(koT.cSetupUnlock), findsOneWidget);
    final save = tester.widget<FilledButton>(find.widgetWithText(FilledButton, koT.cSetupSave));
    expect(save.onPressed, isNull);
    final store = tester.widget<TextField>(find.byType(TextField).first);
    expect(store.enabled, isFalse);
    await unmount(tester);
  });

  testWidgets('saved values are masked and never filled into the paste boxes', (tester) async {
    await _pump(tester, _lockedPortone());
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    expect(find.textContaining('••••0000'), findsWidgets);
    final store = tester.widget<TextField>(find.byType(TextField).first);
    expect(store.controller!.text, isEmpty);
    await unmount(tester);
  });

  testWidgets('an unlocked card shows the unlocked badge and a lock-now button', (tester) async {
    await _pump(tester, _lockedPortone(unlocked: true));
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    expect(find.text(koT.cSetupUnlockedBadge), findsOneWidget);
    expect(find.text(koT.cSetupLockNow), findsOneWidget);
    expect(find.text(koT.cSetupLockedBadge), findsNothing);
    await unmount(tester);
  });

  testWidgets('a secret paste box can show and hide what is typed', (tester) async {
    await _pump(tester, _portoneCard());
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    final secret = find.byType(TextField).at(1);
    expect(tester.widget<TextField>(secret).obscureText, isTrue);
    await tester.tap(find.byTooltip(koT.cSetupShow));
    await settle(tester);
    expect(tester.widget<TextField>(secret).obscureText, isFalse);
    await unmount(tester);
  });

  testWidgets('a bad store id shows a friendly message under the box', (tester) async {
    await _pump(tester, _portoneCard());
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    await tester.enterText(find.byType(TextField).first, 'shop-1');
    await settle(tester);

    expect(find.text(koT.err_invalid_store_id), findsOneWidget);
    await unmount(tester);
  });
}
