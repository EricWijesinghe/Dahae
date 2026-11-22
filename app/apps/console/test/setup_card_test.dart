// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/screens/setup.dart';
import 'package:console/screens/setup_guides.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

Json _field(
  String name,
  String kind, {
  bool set = false,
  Object? preview,
  bool required = true,
}) => {
  'name': name,
  'kind': kind,
  'required': required,
  'set': set,
  'preview': preview,
};

Json _payoutCard({
  String? mode,
  bool configured = true,
  Json? lastTest,
  Json? lastCheck,
}) => {
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
      preview: configured ? '••••' : null,
    ),
    _field('security_key', 'secret'),
  ],
  'last_test': lastTest,
  'last_check': lastCheck,
};

const _pass = {'ok': true, 'code': 'sandbox_connected', 'key_fp': 'fp'};
const _fail = {'ok': false, 'code': 'unreachable', 'key_fp': 'fp'};

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
  String? tossPaymentsWebhookUrl,
  FakeApi? api,
  Size size = tallPhoneSize,
}) async {
  setViewport(tester, size);
  final env = await TestEnv.create(fake: api ?? FakeApi(), role: 'staff_admin');
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
          tossPaymentsWebhookUrl: tossPaymentsWebhookUrl,
        ),
      ],
    ),
  );
  await settle(tester);
}

/// Tall enough that the whole card, including its buttons, is on screen for taps.
const _fullCardSize = Size(390, 3200);

void main() {
  _lockedTests();
  _lastCheckTests();
  _tossPaymentsTests();

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

  testWidgets('the AWS template bucket reads as AWS S3 (automatic)', (
    tester,
  ) async {
    await _pump(tester, {
      'code': 'backup',
      'source': 'aws_role',
      'configured': true,
      'fields': [_field('bucket', 'text'), _field('access_key', 'secret')],
      'last_test': null,
    });

    expect(find.text(koT.cSetupAwsAuto), findsOneWidget);
    expect(find.text(koT.cSetupFromEnv), findsNothing);
    await unmount(tester);
  });

  testWidgets('no mode from the server means no badge', (tester) async {
    await _pump(tester, _payoutCard(mode: null));

    expect(find.text(koT.cSetupModeTest), findsNothing);
    expect(find.text(koT.cSetupModeLive), findsNothing);
    await unmount(tester);
  });

  testWidgets('a saved secret shows only the mask, no part of its value', (
    tester,
  ) async {
    await _pump(tester, _payoutCard(mode: 'test'));
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    expect(find.textContaining('••••'), findsOneWidget);
    expect(
      find.textContaining('1234'),
      findsNothing,
      reason: 'no tail of the key is shown',
    );
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

    await tester.tap(find.byTooltip(koT.cCopy));
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
      // Toss's address goes into the Toss developer center, not the PortOne console.
      expect(find.text(koT.cSetupTossWebhook), findsOneWidget);
      expect(find.text(koT.cSetupWebhook), findsNothing);
      await unmount(tester);

      await _pump(tester, _portoneCard(), tossWebhookUrl: tossUrl);
      expect(find.text(tossUrl), findsNothing);
      await unmount(tester);
    },
  );
}

Json _lockedPortone({bool unlocked = false, bool canTest = true}) => {
  'code': 'portone',
  'can_test': canTest,
  'source': 'console',
  'configured': true,
  'locked': !unlocked,
  'unlock_expires_at': unlocked
      ? DateTime.now().add(const Duration(minutes: 5)).toUtc().toIso8601String()
      : null,
  'fields': [
    _field(
      'store_id',
      'text',
      set: true,
      preview: 'store-00000000-0000-0000-0000-000000000000',
    ),
    _field('api_secret', 'secret', set: true, preview: '••••'),
    _field('webhook_secret', 'secret'),
    _field('channel_keys', 'map', set: true, preview: {'card': '••••'}),
    _field('cash_receipts', 'toggle'),
  ],
  'last_test': null,
};

void _lockedTests() {
  testWidgets(
    'a saved card is locked: badge, disabled fields, disabled save, unlock button',
    (tester) async {
      await _pump(tester, _lockedPortone());
      await tester.tap(find.byType(ExpansionTile));
      await settle(tester);

      expect(find.text(koT.cSetupLockedBadge), findsOneWidget);
      expect(find.text(koT.cSetupUnlock), findsOneWidget);
      final save = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, koT.cSetupSave),
      );
      expect(save.onPressed, isNull);
      final store = tester.widget<TextField>(find.byType(TextField).first);
      expect(store.enabled, isFalse);
      await unmount(tester);
    },
  );

  testWidgets(
    'the owner runs the connection test on a locked card with no unlock, no owner code and no values',
    (tester) async {
      final api = FakeApi()
        ..reply('POST', '/admin/setup/portone/test', {
          'ok': true,
          'code': 'connected',
        });
      await _pump(tester, _lockedPortone(), api: api, size: _fullCardSize);
      await tester.tap(find.byType(ExpansionTile));
      await settle(tester);

      final check = find.widgetWithText(OutlinedButton, koT.cSetupTestApiKey);
      expect(tester.widget<OutlinedButton>(check).onPressed, isNotNull);
      await tester.tap(check);
      await settle(tester);

      expect(find.byType(AlertDialog), findsNothing, reason: 'no owner code');
      final call = api.lastCall('POST', '/admin/setup/portone/test')!;
      expect(call.body, isNull, reason: 'the server tests the saved values');
      expect(call.headers.keys, isNot(contains('x-owner-totp')));
      expect(api.callsTo('POST', '/admin/setup/portone/unlock'), isEmpty);
      await unmount(tester);
    },
  );

  testWidgets(
    'a locked card hides the connection test from a viewer who may not run it',
    (tester) async {
      await _pump(tester, _lockedPortone(canTest: false), size: _fullCardSize);
      await tester.tap(find.byType(ExpansionTile));
      await settle(tester);

      expect(
        find.widgetWithText(OutlinedButton, koT.cSetupTestApiKey),
        findsNothing,
      );
      expect(find.text(koT.cSetupLockedBadge), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets('saved values are masked and never filled into the paste boxes', (
    tester,
  ) async {
    await _pump(tester, _lockedPortone());
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    expect(
      find.textContaining('store-00000000-0000-0000-0000-000000000000'),
      findsOneWidget,
      reason: 'the store id shows in full',
    );
    expect(
      find.textContaining('••••'),
      findsWidgets,
      reason: 'secrets and channel keys show the mask',
    );
    final store = tester.widget<TextField>(find.byType(TextField).first);
    expect(store.controller!.text, isEmpty);
    await unmount(tester);
  });

  testWidgets(
    'an unlocked card counts down the minutes left and offers lock-now',
    (tester) async {
      await _pump(tester, _lockedPortone(unlocked: true));
      await tester.tap(find.byType(ExpansionTile));
      await settle(tester);

      expect(find.text(koT.cSetupUnlockLeft('5')), findsOneWidget);
      expect(find.text(koT.cSetupLockNow), findsOneWidget);
      expect(find.text(koT.cSetupLockedBadge), findsNothing);
      await unmount(tester);
    },
  );

  testWidgets(
    'a wrong owner code shows the owner copy, not the generic message',
    (tester) async {
      final api = FakeApi()
        ..reply('POST', '/admin/setup/portone/unlock', {
          'error': {'code': 'totp_invalid'},
        }, status: 400);
      await _pump(tester, _lockedPortone(), api: api, size: _fullCardSize);
      await tester.tap(find.byType(ExpansionTile));
      await settle(tester);

      await tester.tap(find.widgetWithText(OutlinedButton, koT.cSetupUnlock));
      await settle(tester);
      final dialog = find.byType(AlertDialog);
      await tester.enterText(
        find.descendant(of: dialog, matching: find.byType(TextField)),
        '123456',
      );
      await settle(tester);
      await tester.tap(
        find.descendant(
          of: dialog,
          matching: find.widgetWithText(FilledButton, koT.cOwnerCodeConfirm),
        ),
      );
      await settle(tester);

      expect(find.text(koT.cOwnerCodeWrong), findsOneWidget);
      expect(find.text(koT.cInvalidValue), findsNothing);
      await unmount(tester);
    },
  );

  testWidgets(
    'a value that breaks its format is refused before any request is sent',
    (tester) async {
      final api = FakeApi();
      await _pump(
        tester,
        _lockedPortone(unlocked: true),
        api: api,
        size: _fullCardSize,
      );
      await tester.tap(find.byType(ExpansionTile));
      await settle(tester);

      await tester.enterText(find.byType(TextField).first, 'shop-1');
      await settle(tester);
      await tester.tap(find.widgetWithText(FilledButton, koT.cSetupSave));
      await settle(tester);

      expect(api.callsTo('PUT', '/admin/setup/portone'), isEmpty);
      expect(find.text(setupMessage(koT, 'invalid_store_id')), findsWidgets);
      await unmount(tester);
    },
  );

  testWidgets('a saved channel slot can be cleared alone and is sent as null', (
    tester,
  ) async {
    final api = FakeApi()
      ..reply('PUT', '/admin/setup/portone', _lockedPortone(unlocked: true));
    await _pump(
      tester,
      _lockedPortone(unlocked: true),
      api: api,
      size: _fullCardSize,
    );
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    await tester.tap(find.byTooltip(koT.cSetupClearSlot));
    await settle(tester);
    expect(find.text(koT.cSetupSlotCleared), findsOneWidget);
    expect(find.byTooltip(koT.cSetupUndoClearSlot), findsOneWidget);
    await tester.tap(find.widgetWithText(FilledButton, koT.cSetupSave));
    await settle(tester);

    final call = api.lastCall('PUT', '/admin/setup/portone')!;
    final slots = call.json['channel_keys'] as Map;
    expect(slots.containsKey('card'), isTrue);
    expect(slots['card'], isNull, reason: 'null removes the saved slot');
    expect(find.text(koT.cSetupSlotCleared), findsNothing);
    await unmount(tester);
  });

  testWidgets('a locked card offers no slot clear', (tester) async {
    await _pump(tester, _lockedPortone(), size: _fullCardSize);
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);
    expect(find.byTooltip(koT.cSetupClearSlot), findsNothing);
    await unmount(tester);
  });

  testWidgets('after a save the typed values leave the boxes', (tester) async {
    final api = FakeApi()
      ..reply('PUT', '/admin/setup/portone', _lockedPortone(unlocked: true));
    await _pump(
      tester,
      _lockedPortone(unlocked: true),
      api: api,
      size: _fullCardSize,
    );
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    await tester.enterText(
      find.byType(TextField).first,
      'store-00000000-0000-0000-0000-000000000000',
    );
    await settle(tester);
    await tester.tap(find.widgetWithText(FilledButton, koT.cSetupSave));
    await settle(tester);

    expect(api.callsTo('PUT', '/admin/setup/portone'), hasLength(1));
    final store = tester.widget<TextField>(find.byType(TextField).first);
    expect(store.controller!.text, isEmpty);
    await unmount(tester);
  });

  testWidgets(
    'channel slots explain where their key comes from and group the other methods',
    (tester) async {
      await _pump(tester, _lockedPortone(unlocked: true));
      await tester.tap(find.byType(ExpansionTile));
      await settle(tester);

      expect(find.textContaining(koT.cSlotHintCard), findsOneWidget);
      expect(find.text(koT.cSetupOtherMethods), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets('a secret paste box can show and hide what is typed', (
    tester,
  ) async {
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

  testWidgets('a bad store id shows a friendly message under the box', (
    tester,
  ) async {
    await _pump(tester, _portoneCard());
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    await tester.enterText(find.byType(TextField).first, 'shop-1');
    await settle(tester);

    expect(find.text(koT.err_invalid_store_id), findsOneWidget);
    await unmount(tester);
  });
}

void _lastCheckTests() {
  testWidgets('a passing Toss check reads as connected', (tester) async {
    await _pump(
      tester,
      _payoutCard(mode: 'test', lastTest: _pass, lastCheck: _pass),
    );

    expect(find.text(koT.cSetupConnected), findsOneWidget);
    expect(find.text(koT.cSetupLastPassKept), findsNothing);
    await unmount(tester);
  });

  testWidgets(
    'a reopened card shows the latest failed check, not the earlier pass',
    (tester) async {
      await _pump(
        tester,
        _payoutCard(mode: 'test', lastTest: _pass, lastCheck: _fail),
      );

      expect(find.text(koT.cSetupUnreachable), findsOneWidget);
      expect(find.text(koT.cSetupConnected), findsNothing);
      expect(find.text(koT.cSetupLastPassKept), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets('a failed check answer replaces the shown pass', (tester) async {
    final api = FakeApi()
      ..reply('POST', '/admin/setup/payout/test', {
        ..._fail,
        'last_test': _pass,
      });
    await _pump(
      tester,
      _payoutCard(mode: 'test', lastTest: _pass, lastCheck: _pass),
      api: api,
      size: _fullCardSize,
    );
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);
    await tester.tap(find.widgetWithText(OutlinedButton, koT.cSetupTest));
    await settle(tester);

    expect(find.text(koT.cSetupUnreachable), findsOneWidget);
    expect(find.text(koT.cSetupConnected), findsNothing);
    expect(find.text(koT.cSetupLastPassKept), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('a card without a newer attempt still shows its stored check', (
    tester,
  ) async {
    await _pump(tester, _payoutCard(mode: 'test', lastTest: _fail));

    expect(find.text(koT.cSetupUnreachable), findsOneWidget);
    expect(find.text(koT.cSetupLastPassKept), findsNothing);
    await unmount(tester);
  });
}

Json _tossPaymentsCard({
  bool configured = false,
  bool locked = false,
  String? mode,
}) => {
  'code': 'tosspayments',
  'can_test': true,
  'source': configured ? 'console' : 'none',
  'configured': configured,
  'locked': locked,
  'mode': mode,
  'fields': [
    _field(
      'client_key',
      'text',
      set: configured,
      preview: configured ? 'test_ck_fakefakefake' : null,
    ),
    _field(
      'secret_key',
      'secret',
      set: configured,
      preview: configured ? '••••' : null,
    ),
    _field('billing_client_key', 'text', required: false),
    _field('billing_secret_key', 'secret', required: false),
    _field('security_key', 'secret', required: false),
    _field('active', 'toggle', required: false),
  ],
  'last_test': null,
};

void _tossPaymentsTests() {
  const paymentsUrl = 'https://api.example.test/v1/webhooks/tosspayments';
  const payoutsUrl = 'https://api.example.test/v1/webhooks/toss';

  testWidgets(
    'the Toss Payments card labels every field, explains the switch and shows its own webhook address',
    (tester) async {
      await _pump(
        tester,
        _tossPaymentsCard(),
        tossWebhookUrl: payoutsUrl,
        tossPaymentsWebhookUrl: paymentsUrl,
        size: _fullCardSize,
      );

      expect(find.text(koT.cIntTossPayments), findsOneWidget);
      final g = guides['tosspayments']!;
      for (final f in ['client_key', 'secret_key']) {
        expect(find.text(g.fields[f]!.label.ko), findsOneWidget, reason: f);
      }
      expect(
        find.text(
          '${g.fields['billing_client_key']!.label.ko} (${koT.cSetupOptional})',
        ),
        findsOneWidget,
      );
      expect(find.text(g.fields['active']!.hint!.ko), findsOneWidget);
      expect(find.text(paymentsUrl), findsOneWidget);
      expect(find.text(koT.cSetupTossWebhook), findsOneWidget);
      expect(
        find.text(payoutsUrl),
        findsNothing,
        reason: 'the payouts address belongs on the payout card',
      );
      await unmount(tester);
    },
  );

  testWidgets(
    'the first save of the Toss Payments card asks for the owner code and sends it',
    (tester) async {
      final api = FakeApi()
        ..reply('PUT', '/admin/setup/tosspayments', _tossPaymentsCard());
      await _pump(tester, _tossPaymentsCard(), api: api, size: _fullCardSize);

      await tester.enterText(
        find.byType(TextField).at(0),
        'test_ck_fakefakefake',
      );
      await tester.enterText(
        find.byType(TextField).at(1),
        'test_sk_fakefakefake',
      );
      await settle(tester);
      await tester.tap(find.widgetWithText(FilledButton, koT.cSetupSave));
      await settle(tester);

      final dialog = find.byType(AlertDialog);
      expect(dialog, findsOneWidget, reason: 'the owner code is asked first');
      await tester.enterText(
        find.descendant(of: dialog, matching: find.byType(TextField)),
        '123456',
      );
      await settle(tester);
      await tester.tap(
        find.descendant(
          of: dialog,
          matching: find.widgetWithText(FilledButton, koT.cOwnerCodeConfirm),
        ),
      );
      await settle(tester);

      final call = api.lastCall('PUT', '/admin/setup/tosspayments')!;
      expect(call.headers['x-owner-totp'], '123456');
      expect(call.body, {
        'client_key': 'test_ck_fakefakefake',
        'secret_key': 'test_sk_fakefakefake',
      });
      await unmount(tester);
    },
  );

  testWidgets('a widget client key is refused before any request is sent', (
    tester,
  ) async {
    final api = FakeApi();
    await _pump(tester, _tossPaymentsCard(), api: api, size: _fullCardSize);

    await tester.enterText(find.byType(TextField).at(0), 'test_gck_fakefake');
    await settle(tester);
    await tester.tap(find.widgetWithText(FilledButton, koT.cSetupSave));
    await settle(tester);

    expect(api.callsTo('PUT', '/admin/setup/tosspayments'), isEmpty);
    expect(find.byType(AlertDialog), findsNothing);
    expect(find.text(koT.err_invalid_client_key), findsWidgets);
    await unmount(tester);
  });

  testWidgets(
    'a saved Toss Payments card checks its API key and shows the test badge',
    (tester) async {
      final api = FakeApi()
        ..reply('POST', '/admin/setup/tosspayments/test', {
          'ok': false,
          'code': 'billing_invalid_key',
        });
      await _pump(
        tester,
        _tossPaymentsCard(configured: true, mode: 'test'),
        api: api,
        size: _fullCardSize,
      );
      expect(find.text(koT.cSetupModeTest), findsOneWidget);
      await tester.tap(find.byType(ExpansionTile));
      await settle(tester);

      await tester.tap(
        find.widgetWithText(OutlinedButton, koT.cSetupTestApiKey),
      );
      await settle(tester);

      expect(
        api.callsTo('POST', '/admin/setup/tosspayments/test'),
        hasLength(1),
      );
      expect(find.text(koT.cSetupBillingInvalidKey), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets('the gateway switch does not move on a locked card', (
    tester,
  ) async {
    await _pump(
      tester,
      _tossPaymentsCard(configured: true, locked: true),
      size: _fullCardSize,
    );
    await tester.tap(find.byType(ExpansionTile));
    await settle(tester);

    final toggle = tester.widget<SwitchListTile>(find.byType(SwitchListTile));
    expect(toggle.onChanged, isNull);
    await unmount(tester);
  });

  Future<void> setupScreen(
    WidgetTester tester,
    Json overview, {
    List<Json>? cards,
    List<Json> methods = const [],
  }) async {
    setViewport(tester, tallPhoneSize);
    final api = FakeApi()
      ..reply('GET', '/admin/setup', {
        'business': {'name': 'Dahae'},
        'review_account': null,
        'integrations': cards ?? [_portoneCard(), _tossPaymentsCard()],
        'dev_mode': false,
        'webhook_url': 'https://api.example.test/v1/webhooks/portone',
        'toss_payments_webhook_url': paymentsUrl,
        ...overview,
      })
      ..reply('GET', '/admin/payment-methods', methods)
      ..reply('GET', '/admin/app-links', {
        'android_cert_sha256': <String>[],
        'apple_team_id': null,
        'source': 'env',
      });
    final env = await TestEnv.create(fake: api, role: 'staff_admin');
    await pumpConsoleScreen(tester, env, const SetupScreen());
    await settle(tester);
  }

  Future<void> scrollTo(WidgetTester tester, Finder line) =>
      tester.scrollUntilVisible(
        line,
        300,
        scrollable: find
            .descendant(
              of: find.byType(SetupScreen),
              matching: find.byType(Scrollable),
            )
            .first,
      );

  Future<void> gatewayHeading(
    WidgetTester tester,
    Json overview,
    String expected,
  ) async {
    await setupScreen(tester, overview);
    final line = find.text(expected);
    await scrollTo(tester, line);
    expect(line, findsOneWidget);
    await unmount(tester);
  }

  const toss = 'Toss Payments';
  String live(String names) => koT.cSetupGatewayLive(names);
  final testKeys = koT.cSetupGatewayTestKeys;

  testWidgets(
    'with no gateway set up, the heading says so',
    (tester) => gatewayHeading(tester, {
      'payment_gateways': <Json>[],
      'payment_gateway': 'portone',
    }, koT.cSetupGatewayNone),
  );

  testWidgets(
    'PortOne alone is named in the heading',
    (tester) => gatewayHeading(tester, {
      'payment_gateways': [
        {'code': 'portone', 'mode': null},
      ],
    }, live('PortOne')),
  );

  testWidgets(
    'Toss Payments on live keys is named without a mark',
    (tester) => gatewayHeading(tester, {
      'payment_gateways': [
        {'code': 'tosspayments', 'mode': 'live'},
      ],
    }, live('토스페이먼츠')),
  );

  testWidgets(
    'Toss Payments on test keys says no real money moves',
    (tester) => gatewayHeading(tester, {
      'payment_gateways': [
        {'code': 'tosspayments', 'mode': 'test'},
      ],
    }, live('토스페이먼츠 $testKeys')),
  );

  testWidgets(
    'both gateways are listed PortOne first, the test mark next to Toss',
    (tester) => gatewayHeading(tester, {
      'payment_gateways': [
        {'code': 'portone', 'mode': null},
        {'code': 'tosspayments', 'mode': 'test'},
      ],
    }, live('PortOne, 토스페이먼츠 $testKeys')),
  );

  testWidgets(
    'an older server with one gateway field reads the same',
    (tester) => gatewayHeading(tester, {
      'payment_gateway': 'tosspayments',
      'payment_gateway_mode': 'test',
    }, live('토스페이먼츠 $testKeys')),
  );

  test('the English heading uses the English brand names', () {
    final en = lookupT(const Locale('en'));
    expect(
      en.cSetupGatewayLive('PortOne, $toss ${en.cSetupGatewayTestKeys}'),
      'Payments go to: PortOne, Toss Payments (test keys: no real money moves)',
    );
  });

  testWidgets('a server without gateway fields shows no heading line', (
    tester,
  ) async {
    await setupScreen(tester, const {});
    expect(find.textContaining(live('').trim()), findsNothing);
    expect(find.text(koT.cSetupGatewayNone), findsNothing);
    await unmount(tester);
  });

  testWidgets('one payment gateway set up is enough to finish setup', (
    tester,
  ) async {
    await setupScreen(
      tester,
      {
        'payment_gateways': [
          {'code': 'portone', 'mode': null},
        ],
      },
      cards: [
        {..._portoneCard(), 'source': 'console', 'configured': true},
        _tossPaymentsCard(),
      ],
    );
    final line = find.text(koT.cSetupProgress('2', '2'));
    await scrollTo(tester, line);
    expect(line, findsOneWidget);
    await unmount(tester);
  });

  testWidgets('each payment method row names its gateway', (tester) async {
    await setupScreen(
      tester,
      {
        'payment_gateways': [
          {'code': 'tosspayments', 'mode': 'live'},
        ],
      },
      methods: [
        {
          'code': 'card',
          'flow': 'billing',
          'enabled': true,
          'configured': true,
          'gateway': 'tosspayments',
        },
        {
          'code': 'transfer',
          'flow': 'checkout',
          'enabled': false,
          'configured': false,
          'gateway': null,
        },
      ],
    );
    final tile = find.text(koT.cSetupPayMethods);
    await scrollTo(tester, tile);
    await tester.tap(tile);
    await settle(tester);

    expect(find.text(koT.cPayMethodGateway('토스페이먼츠')), findsOneWidget);
    expect(find.text(koT.cSetupNeedsKey), findsOneWidget);
    await unmount(tester);
  });
}
