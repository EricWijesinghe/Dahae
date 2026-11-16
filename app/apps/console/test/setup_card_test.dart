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
    _field(
      'mode',
      'text',
      set: configured,
      preview: configured ? 'manual' : null,
    ),
    _field(
      'secret_key',
      'secret',
      set: configured,
      preview: configured ? '••••1234' : null,
    ),
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

Future<void> _pump(
  WidgetTester tester,
  Json card, {
  String? webhookUrl,
  String? tossWebhookUrl,
}) async {
  setViewport(tester, tallPhoneSize);
  final env = await TestEnv.create(fake: FakeApi(), role: 'staff_admin');
  await pumpConsoleScreen(
    tester,
    env,
    ListView(
      children: [
        IntegrationCard(
          card: card,
          onChanged: () {},
          webhookUrl: webhookUrl,
          tossWebhookUrl: tossWebhookUrl,
        ),
      ],
    ),
  );
  await settle(tester);
}

void main() {
  testWidgets('a saved test key shows the 테스트 badge and no live badge', (
    tester,
  ) async {
    await _pump(tester, _payoutCard(mode: 'test'));

    expect(find.text(koT.cSetupModeTest), findsOneWidget);
    expect(find.text(koT.cSetupModeLive), findsNothing);
    await unmount(tester);
  });

  testWidgets('a saved live key shows the 실거래 badge and no test badge', (
    tester,
  ) async {
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

  testWidgets('a saved secret shows only its last four characters', (
    tester,
  ) async {
    await _pump(tester, _payoutCard(mode: 'test'));
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    expect(find.textContaining('••••1234'), findsOneWidget);
    expect(
      find.textContaining('test_sk_'),
      findsNothing,
      reason: 'the key prefix is not echoed either',
    );
    await unmount(tester);
  });

  testWidgets('the PortOne webhook address copies to the clipboard', (
    tester,
  ) async {
    const url = 'https://api.example.test/v1/webhooks/portone';
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

    await _pump(tester, _portoneCard(), webhookUrl: url);
    expect(find.text(koT.cSetupWebhook), findsOneWidget);
    expect(find.text(url), findsOneWidget);

    await tester.tap(find.byTooltip(koT.cCopied));
    await settle(tester);

    expect(copied, [url]);
    await unmount(tester);
  });

  testWidgets(
    'the Toss webhook address shows on the payout card and not on PortOne',
    (tester) async {
      const tossUrl = 'https://api.example.test/v1/webhooks/toss';
      // Unconfigured cards open by default, so the copy rows are on screen.
      await _pump(
        tester,
        _payoutCard(configured: false),
        tossWebhookUrl: tossUrl,
      );
      expect(find.text(tossUrl), findsOneWidget);
      await unmount(tester);

      await _pump(tester, _portoneCard(), tossWebhookUrl: tossUrl);
      expect(find.text(tossUrl), findsNothing);
      await unmount(tester);
    },
  );
}
