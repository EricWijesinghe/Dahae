// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter_test/flutter_test.dart';
import 'package:ui/ui.dart';

void main() {
  test('KRW reads exactly as formatWon in every language', () {
    for (final locale in ['ko', 'en', 'ar']) {
      for (final n in [0, 900, 52000, 1234567]) {
        expect(formatMoney(n, Currency.krw, locale), formatWon(n, locale));
      }
      expect(moneyUnit(Currency.krw, locale), currencyUnit(locale));
    }
    expect(formatMoney(52000, Currency.krw, 'ko'), '52,000원');
    expect(formatMoney(52000, Currency.krw, 'en'), '₩52,000');
  });

  test('minor units follow the currency', () {
    expect(formatMoney(1050, const Currency('EUR', 2), 'en'), '€10.50');
    expect(formatMoney(1050, const Currency('EUR', 2), 'de'), '10,50 €');
    expect(formatMoney(1500, const Currency('JPY', 0), 'en'), '¥1,500');
    expect(
      formatMoney(1234, const Currency('KWD', 3), 'en'),
      contains('1.234'),
    );
    expect(moneyUnit(const Currency('GBP', 2), 'en'), '£');
  });

  test('a pack currency object reads, anything else does not', () {
    final eur = Currency.fromJson({'code': 'EUR', 'minor_units': 2});
    expect((eur?.code, eur?.minorUnits), ('EUR', 2));
    expect(Currency.fromJson({'code': 'EUR'}), isNull);
    expect(Currency.fromJson({'code': 'EURO', 'minor_units': 2}), isNull);
    expect(Currency.fromJson('KRW'), isNull);
  });
}
