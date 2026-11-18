// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/auth_pay.dart';
import 'package:core/core.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final portone = BillingSetup.fromJson({
    'enabled': true,
    'gateway': 'portone',
    'store_id': 'store-1',
    'channels': {'card': 'ch-card'},
    'issue_id': 'iss-1',
    'customer_id': 'user-1',
  });
  final toss = BillingSetup.fromJson({
    'enabled': true,
    'gateway': 'tosspayments',
    'store_id': null,
    'channels': {'card': 'test_ck_billing'},
    'issue_id': '0b8e6c1e-3f7a-4d2b-9c1d-2a6f0e9b7c45',
    'customer_id': 'user-1',
  });

  test('setup reads the gateway; a missing one is PortOne', () {
    expect(toss.isToss, isTrue);
    expect(portone.isToss, isFalse);
    expect(BillingSetup.fromJson({'enabled': true}).isToss, isFalse);
  });

  test(
    'PortOne cards register on billing.html with store, issue and customer',
    () {
      final u = billingPageUri('https://x', portone, 'card');
      expect(u.path, '/pay/billing.html');
      expect(u.queryParameters, {
        'store': 'store-1',
        'channel': 'ch-card',
        'issue': 'iss-1',
        'customer': 'user-1',
        'method': 'card',
        'app': payAppScheme,
      });
    },
  );

  test(
    'Toss cards register on toss-billing.html; the issue id is the customerKey',
    () {
      final u = billingPageUri('https://x', toss, 'card');
      expect(u.path, '/pay/toss-billing.html');
      expect(u.queryParameters, {
        'channel': 'test_ck_billing',
        'issue': '0b8e6c1e-3f7a-4d2b-9c1d-2a6f0e9b7c45',
        'method': 'card',
        'app': payAppScheme,
      });
    },
  );

  test('the redirect gives the PortOne billingKey or the Toss authKey', () {
    expect(
      billingKeyFromRedirect(
        Uri.parse('https://x/pay/billing-done.html?customerKey=c&authKey=ak_1'),
      ),
      'ak_1',
    );
    expect(
      billingKeyFromRedirect(
        Uri.parse('https://x/pay/billing-done.html?billingKey=bk_1'),
      ),
      'bk_1',
    );
    expect(
      billingKeyFromRedirect(
        Uri.parse('https://x/pay/billing-done.html?authKey=&code=USER_CANCEL'),
      ),
      isNull,
    );
  });
}
