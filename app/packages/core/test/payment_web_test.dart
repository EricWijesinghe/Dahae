// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/auth_pay.dart';
import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const _intent = CheckoutIntent(
  id: 'ck-1',
  amount: 52000,
  orderName: '화장실 청소',
  method: 'card',
  storeId: 'store-1',
  channelKey: 'ch-card',
);

void main() {
  test(
    'web builds take the hosted redirect page, native builds the WebView',
    () {
      expect(
        paymentLauncherFor('https://api.example.com', isWeb: true),
        isA<PortOneRedirectLauncher>(),
      );
      expect(
        paymentLauncherFor('https://api.example.com', isWeb: false),
        isA<PortOneWebViewLauncher>(),
      );
    },
  );

  test('the default follows kIsWeb', () {
    final launcher = paymentLauncherFor('https://api.example.com');
    expect(
      launcher,
      kIsWeb ? isA<PortOneRedirectLauncher>() : isA<PortOneWebViewLauncher>(),
    );
  });

  test('dev base keeps the simulated window, on web too', () {
    expect(
      paymentLauncherFor('http://127.0.0.1:8080', isWeb: true),
      isA<DevPaymentLauncher>(),
    );
    expect(
      paymentLauncherFor('http://localhost:8080', isWeb: false),
      isA<DevPaymentLauncher>(),
    );
  });

  test('redirect page carries the return path only when asked', () {
    final back = portOnePageUri('https://x', _intent, back: webPayReturnPath);
    expect(back.queryParameters['back'], '/app/#/pay-return');
    expect(back.queryParameters['checkout'], 'ck-1');
    expect(
      portOnePageUri('https://x', _intent).queryParameters.containsKey('back'),
      isFalse,
    );
  });

  test(
    'the web tab opens toss.html for a Toss checkout, with the return path',
    () {
      const toss = CheckoutIntent(
        id: 'ck-1',
        amount: 52000,
        orderName: '화장실 청소',
        method: 'card',
        storeId: '',
        channelKey: 'test_ck_1',
        gateway: 'tosspayments',
      );
      final u = portOnePageUri('https://x', toss, back: webPayReturnPath);
      expect(u.path, '/pay/toss.html');
      expect(u.queryParameters['back'], '/app/#/pay-return');
      expect(u.queryParameters.containsKey('store'), isFalse);
    },
  );

  testWidgets(
    'a Toss checkout without a client key is refused before any page opens',
    (tester) async {
      await tester.pumpWidget(const SizedBox());
      final context = tester.element(find.byType(SizedBox));
      const keyless = CheckoutIntent(
        id: 'ck-1',
        amount: 1,
        orderName: '',
        method: 'card',
        storeId: '',
        channelKey: '',
        gateway: 'tosspayments',
      );
      final out = await const PortOneRedirectLauncher(
        'https://x',
      ).pay(context: context, intent: keyless);
      expect(
        out,
        isA<PaymentRejected>().having(
          (r) => r.code,
          'code',
          'provider_not_configured',
        ),
      );
    },
  );
}
