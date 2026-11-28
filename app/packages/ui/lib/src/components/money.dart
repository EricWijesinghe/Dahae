// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:math' as math;

import 'package:intl/intl.dart';

import 'basics.dart';

/// One currency's money rules, as the server's `domain::currency::Currency` and a pack's `currency` carry them:
/// the ISO 4217 code and the decimal places of its minor unit (KRW 0, EUR 2, KWD 3). Amounts stay `int` minor units.
class Currency {
  const Currency(this.code, this.minorUnits);
  final String code;
  final int minorUnits;

  static const krw = Currency('KRW', 0);

  /// A pack's `currency` object; null when it is not one.
  static Currency? fromJson(Object? json) => switch (json) {
    {'code': final String code, 'minor_units': final int minor}
        when code.length == 3 && minor >= 0 && minor <= 4 =>
      Currency(code, minor),
    _ => null,
  };
}

/// [minor] units of [currency], as a customer reads them in [locale]. KRW is [formatWon] byte for byte, so Korean
/// installs read exactly what they always did.
String formatMoney(int minor, Currency currency, String locale) {
  if (currency.code == Currency.krw.code) return formatWon(minor, locale);
  return NumberFormat.simpleCurrency(
    locale: locale,
    name: currency.code,
    decimalDigits: currency.minorUnits,
  ).format(minor / math.pow(10, currency.minorUnits));
}

/// Unit shown next to an amount input in [currency]; KRW is [currencyUnit].
String moneyUnit(Currency currency, String locale) =>
    currency.code == Currency.krw.code
    ? currencyUnit(locale)
    : NumberFormat.simpleCurrency(
        locale: locale,
        name: currency.code,
      ).currencySymbol;
