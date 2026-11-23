// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/screens/setup_business.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ui/ui.dart';

void main() {
  test(
    'every offered brand swatch passes the white-text contrast the apps require',
    () {
      for (final s in brandSwatches) {
        expect(whiteTextReadsOn(parseHex(s)!), isTrue, reason: s);
      }
    },
  );
}
