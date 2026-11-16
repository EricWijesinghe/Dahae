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
      // setup.dart: the PortOne card says "API 키 확인", every other card "연결 확인".
      final (ko, e) = key == 'portone'
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
      6,
      reason:
          'portone, backup, solapi, push, juso and ai each end with the check',
    );
    expect(koT.oActIntegration, contains('[${koT.cSetupTest}]'));
    expect(en.oActIntegration, contains(en.cSetupTest));
  });
}
