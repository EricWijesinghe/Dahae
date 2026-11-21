// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/auth_pay.dart';
import 'package:core/core.dart';
import 'package:customer/screens/pay_method_picker.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/customer_host.dart';
import 'support/customer_routes.dart';

FakeApi _api() {
  final fake = FakeApi()
    ..reply('GET', '/payments/methods', [
      {'code': 'card', 'flow': 'billing'},
      {'code': 'kakaopay', 'flow': 'billing'},
      {'code': 'transfer', 'flow': 'checkout'},
    ])
    ..reply('GET', '/me/payment-methods/setup', {
      'enabled': true,
      'gateway': 'portone',
      'store_id': 'store-1',
      'channels': {'card': 'ch-card'},
      'issue_id': 'iss-1',
    });
  stockCustomerApi(fake, methods: const <Json>[]);
  return fake;
}

/// Pumps the picker and returns the channel the customer picks by tapping [code].
Future<PayChannel?> _tap(
  WidgetTester tester,
  String code, {
  FakeApi? api,
}) async {
  setViewport(tester, phoneSize);
  final env = await TestEnv.create(fake: api ?? _api());
  PayChannel? picked;
  await pumpCustomerScreen(
    tester,
    env,
    Scaffold(
      body: ListView(
        padding: const EdgeInsets.all(Space.gutter),
        children: [
          PayMethodPicker(
            onChoose: (c) => picked = c is ChannelChoice ? c.channel : null,
          ),
        ],
      ),
    ),
  );
  await settle(tester);
  await tester.tap(
    find.byWidgetPredicate((w) => w is PayChannelTile && w.code == code),
  );
  await settle(tester);
  return picked;
}

void main() {
  testWidgets(
    'a saved-method tile that cannot be saved is paid once instead of hidden',
    (tester) async {
      final api = _api();
      final kakao = await _tap(tester, 'kakaopay', api: api);
      expect(
        kakao?.flow,
        'checkout',
        reason: 'no billing channel for kakaopay',
      );
      for (final path in ['/payments/methods', '/me/payment-methods/setup']) {
        expect(
          api.lastCall('GET', path)!.query['gateways'],
          'portone,tosspayments',
          reason: 'the server offers Toss only to apps that open its pages',
        );
      }
      await unmount(tester);
    },
  );

  testWidgets('card on Windows (no card registration) is paid in the window', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.windows;
    try {
      final card = await _tap(tester, 'card');
      expect(card?.code, 'card');
      expect(card?.flow, 'checkout');
      await unmount(tester);
    } finally {
      debugDefaultTargetPlatformOverride = null;
    }
  });
}
