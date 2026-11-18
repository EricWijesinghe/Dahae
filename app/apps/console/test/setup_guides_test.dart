// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/screens/setup_guides.dart';
import 'package:core/core.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

void main() {
  test('guides name the check button by its real label', () {
    final en = lookupT(const Locale('en'));
    var named = 0;
    for (final MapEntry(:key, value: g) in guides.entries) {
      // setup.dart: the payment gateway cards say "API 키 확인", every other card "연결 확인".
      final (ko, e) = key == 'portone' || key == 'tosspayments'
          ? (koT.cSetupTestApiKey, en.cSetupTestApiKey)
          : (koT.cSetupTest, en.cSetupTest);
      for (final s in g.steps.where(
        (s) => s.ko.contains('[연결') || s.ko.contains('[API 키'),
      )) {
        named++;
        expect(s.ko, contains('[$ko]'), reason: key);
        expect(s.en, contains(e), reason: key);
      }
    }
    expect(
      named,
      8,
      reason:
          'portone, backup, solapi, push, juso and ai each end with the check; '
          'tosspayments checks the test keys, then the live keys',
    );
    expect(koT.oActIntegration, contains('[${koT.cSetupTest}]'));
    expect(en.oActIntegration, contains(en.cSetupTest));
  });

  test('the Toss Payments card has a label and hint for every field', () {
    final g = guides['tosspayments']!;
    const fields = [
      'client_key',
      'secret_key',
      'billing_client_key',
      'billing_secret_key',
      'security_key',
      'active',
    ];
    expect(g.fields.keys, unorderedEquals(fields));
    for (final f in fields) {
      expect(g.fields[f]!.hint, isNotNull, reason: f);
    }
    // The switch tells the owner what moves and what stays on PortOne.
    final active = g.fields['active']!.hint!;
    expect(active.ko, allOf(contains('PortOne'), contains('PASS')));
    expect(active.en, allOf(contains('PortOne'), contains('PASS')));
    expect(g.steps.map((s) => s.ko).join(), contains('DEPOSIT_CALLBACK'));
  });

  test('Toss Payments setup codes get their own words', () {
    final en = lookupT(const Locale('en'));
    for (final (code, text) in [
      ('invalid_client_key', en.err_invalid_client_key),
      ('key_mode_mismatch', en.err_key_mode_mismatch),
      ('billing_invalid_key', en.cSetupBillingInvalidKey),
    ]) {
      expect(setupMessage(en, code), text, reason: code);
      expect(setupMessage(en, code), isNot(en.cInvalidValue), reason: code);
    }
  });
}
