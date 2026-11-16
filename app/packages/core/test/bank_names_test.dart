// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/auth_pay.dart';
import 'package:core/core.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final ko = lookupT(const Locale('ko'));
  final en = lookupT(const Locale('en'));

  test('a PortOne bank code reads as the bank customers know', () {
    expect(ko.bankName('KOOKMIN'), '국민은행');
    expect(en.bankName('KOOKMIN'), 'KB Kookmin Bank');
    expect(ko.bankName('DAEGU'), '대구은행(iM뱅크)');
    expect(ko.bankName('K_BANK'), '케이뱅크');
    expect(en.bankName('KAKAO'), 'KakaoBank');
  });

  test('an unknown or empty code is shown as sent', () {
    expect(ko.bankName('SOME_NEW_BANK'), 'SOME_NEW_BANK');
    expect(en.bankName(''), '');
  });
}
